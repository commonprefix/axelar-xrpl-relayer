package main

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/sha512"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"log/slog"
	"math/big"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"github.com/cenkalti/backoff/v4"
	"github.com/ethereum/go-ethereum"
	abi "github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	gethCommon "github.com/ethereum/go-ethereum/common"
	gethTypes "github.com/ethereum/go-ethereum/core/types"
	gethCrypto "github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/joho/godotenv"
)

type CCID struct {
	Chain string `json:"chain"`
	Id    string `json:"id"`
}

type AddressBook struct {
	SourceGateway      string
	DestinationGateway string
	VotingVerifier     string
	MultisigProver     string
}

type Message struct {
	CcId               CCID   `json:"cc_id"`
	SourceAddress      string `json:"source_address"`
	DestinationChain   string `json:"destination_chain"`
	DestinationAddress string `json:"destination_address"`
	PayloadHash        string `json:"payload_hash"`
}

type Attribute struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

type Event struct {
	Type       string      `json:"type"`
	Attributes []Attribute `json:"attributes"`
}

func (e *Event) ToMap() map[string]string {
	attrsMap := map[string]string{}
	for _, a := range e.Attributes {
		attrsMap[a.Key] = a.Value
	}
	return attrsMap
}

func (e *Event) Exists() bool {
	return e.Type != ""
}

// EventLog represents the structure of the logs in the command output.
type EventLog struct {
	Events []Event `json:"events"`
}

// CommandOutput represents the expected JSON structure of the axelard command output.
type CommandOutput struct {
	Logs []EventLog `json:"logs"`
}

type AxelarClient struct {
	RPCUrl     string
	FromWallet string
}

type BatchConstructProof struct {
	MessageIDs []CCID `json:"message_ids"`
}

type BatchConstructProofCommand struct {
	ConstructProof BatchConstructProof `json:"construct_proof"`
}

type XRPLConstructProof struct {
	MessageID CCID `json:"message_id"`
	Coin      Coin `json:"coin"`
}

type XRPLConstructProofCommand struct {
	ConstructProof XRPLConstructProof `json:"construct_proof"`
}

type Coin struct {
	Amount string `json:"amount"` // todo: uint128
	Denom  string `json:"denom"`
}

func (c *Coin) String() string {
	return fmt.Sprintf("%s%s", c.Amount, c.Denom)
}

type RippleClient struct {
	RPCUrl string
}

type AccountTxParams struct {
	Account        string `json:"account"`
	LedgerIndexMin int64  `json:"ledger_index_min"`
	LedgerIndexMax int64  `json:"ledger_index_max"`
}

type RequestPayload struct {
	Method string            `json:"method"`
	Params []AccountTxParams `json:"params"`
}

type Transaction struct {
	TransactionType string
	Hash            string `json:"hash"`
	Account         string
	Amount          interface{} `json:"Amount"`
	Destination     string      `json:"Destination,omitempty"`
	Memos           []Memo      `json:"Memos,omitempty"`
}

func (tx *Transaction) GetMemo(byType string) (string, error) {
	for _, memo := range tx.Memos {
		memoTypeBytes, err := hex.DecodeString(memo.Memo.MemoType)
		if err != nil {
			fmt.Println("Error decoding memo type:", err)
			continue
		}
		memoType := string(memoTypeBytes)
		if memoType != byType {
			continue
		}
		if byType == "destination_address" || byType == "payload_hash" {
			return memo.Memo.MemoData, nil
		} else {
			memoDataBytes, err := hex.DecodeString(memo.Memo.MemoData)
			if err != nil {
				fmt.Println("Error decoding memo data:", err)
				return "", err
			}
			memoData := string(memoDataBytes)
			return memoData, nil
		}
	}
	return "", fmt.Errorf("memo type %s not found", byType)
}

type Memo struct {
	Memo MemoDetails `json:"Memo"`
}

type MemoDetails struct {
	MemoType string `json:"MemoType"`
	MemoData string `json:"MemoData"`
}

type AccountTxResponsePayload struct {
	Result struct {
		Transactions []struct {
			Tx Transaction `json:"tx"`
		} `json:"transactions"`
	} `json:"result"`
}

type MessageWithCoin struct {
	Message Message
	Coin    Coin
}

type XRPLSigner struct {
	Account       string
	SigningPubKey string
}

type XRPLSignerWrapper struct {
	Signer XRPLSigner
}

type TXRPCResult struct {
	Status  string `json:"status"`
	Signers []XRPLSigner
}

type TXRPCResponse struct {
	Result TXRPCResult `json:"result"`
}

type PublicKey struct {
	ECDSA string `json:"ecdsa"`
}

type MultisigPaymentTx struct {
	Hash         string `json:"hash"`
	Account      string
	Destination  string
	Fee          string
	Flags        int64
	Sequence     int64
	Validated    bool `json:"validated"`
	Signers      []XRPLSignerWrapper
	Meta         struct{ TransactionResult string } `json:"meta"`
	Status       string                             `json:"status"`
	ErrorMessage string                             `json:"error_message,omitempty"`
}

func (tx *MultisigPaymentTx) SignerPubKeys() []PublicKey {
	pubKeys := make([]PublicKey, len(tx.Signers))
	for i, signer := range tx.Signers {
		pubKeys[i] = PublicKey{signer.Signer.SigningPubKey}
	}
	return pubKeys
}

func main() {
	if len(os.Args) < 2 {
		fmt.Printf("Usage: %s [xrpl|ethereum]\n", os.Args[0])
		return
	}
	sourceChain := os.Args[1]
	if sourceChain != "xrpl" && sourceChain != "ethereum" {
		fmt.Printf("Usage: %s [xrpl|ethereum]\n", os.Args[0])
		return
	}

	lvl := new(slog.LevelVar)
	lvl.Set(slog.LevelInfo)

	logger := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{
		Level: lvl,
	}))

	err := godotenv.Load()
	if err != nil {
		log.Fatal("Error loading .env file")
	}

	var addressBook AddressBook

	if sourceChain == "xrpl" {
		addressBook = AddressBook{
			SourceGateway:      os.Getenv("XRPL_GATEWAY"),
			VotingVerifier:     os.Getenv("XRPL_VOTING_VERIFIER"),
			MultisigProver:     os.Getenv("ETHEREUM_MULTISIG_PROVER"),
			DestinationGateway: os.Getenv("ETHEREUM_GATEWAY"),
		}
	} else {
		addressBook = AddressBook{
			SourceGateway:      os.Getenv("ETHEREUM_GATEWAY"),
			VotingVerifier:     os.Getenv("ETHEREUM_VOTING_VERIFIER"),
			MultisigProver:     os.Getenv("XRPL_MULTISIG_PROVER"),
			DestinationGateway: os.Getenv("AXRPL_GATEWAY"),
		}
	}

	multisigAddress := os.Getenv("XRPL_MULTISIG_ADDRESS")

	axelarClient := AxelarClient{os.Getenv("AXELAR_RPC_URL"), os.Getenv("AXELARD_WALLET_NAME")}
	rippleClient := &RippleClient{os.Getenv("RIPPLE_RPC_URL")}

	httpEthereumClient, err := ethclient.Dial(os.Getenv("ETH_RPC_URL"))
	if err != nil {
		panic(err)
	}

	privateKey, err := gethCrypto.HexToECDSA(os.Getenv("ETHEREUM_PRIVATE_KEY"))
	if err != nil {
		log.Fatalf("Failed to parse private key: %v", err)
	}

	if sourceChain == "xrpl" {
		msgChan := make(chan MessageWithCoin, 10)
		go func() {
			processed := map[string]bool{}
			for {
				lastValidatedLedger, err := getXRPLBlockHeight(rippleClient)
				if err != nil {
					logger.Error("failed to get last validated ledger, retrying")
					continue
				}
				messagesWithCoin, err := RippleFetchDepositMessages(rippleClient, multisigAddress, lastValidatedLedger-10000, lastValidatedLedger)
				if err != nil {
					panic(err)
				}

				countSkipped := 0
				countNew := 0
				for _, messageWithCoin := range messagesWithCoin {
					if _, ok := processed[messageWithCoin.Message.CcId.Id]; ok {
						countSkipped += 1
						continue
					}
					logger.Info("processing message", "message", messageWithCoin)
					processed[messageWithCoin.Message.CcId.Id] = true
					countNew += 1
					msgChan <- messageWithCoin
				}
				logger.Info("found messages\n", "count", len(messagesWithCoin), "new", countNew, "skipped", countSkipped, "last_validated_ledger", lastValidatedLedger)
				time.Sleep(time.Second * 10)
			}
		}()

		for {
			select {
			case messageWithCoin := <-msgChan:
				operation := func() error {
					fmt.Println("starting operation", messageWithCoin.Message)
					err := processMessage(logger, sourceChain, axelarClient, rippleClient, addressBook, privateKey, httpEthereumClient, messageWithCoin.Message, messageWithCoin.Coin)
					fmt.Println("operation", messageWithCoin.Message, "failed with", err)
					return err
				}

				b := backoff.NewExponentialBackOff()
				b.MaxElapsedTime = 5 * time.Minute

				err = backoff.Retry(operation, b)
				if err != nil {
					log.Printf("Operation failed despite retries: %v", err)
				}
			}
		}
	} else {
		wssEthereumClient, err := ethclient.Dial(strings.Replace(os.Getenv("ETH_RPC_URL"), "https://", "wss://", 1))
		if err != nil {
			panic(err)
		}

		latestBlock, err := CurrentEthereumBlockHeight(httpEthereumClient)
		if err != nil {
			log.Fatalln("failed to get current ethereum block height")
		}
		contractAddr := gethCommon.HexToAddress(os.Getenv("ETH_AXELAR_GATEWAY"))
		query := ethereum.FilterQuery{
			Addresses: []gethCommon.Address{contractAddr},
			FromBlock: big.NewInt(latestBlock - 100),
			Topics:    [][]gethCommon.Hash{{gethCommon.HexToHash("0x30ae6cc78c27e651745bf2ad08a11de83910ac1e347a52f7ac898c0fbef94dae")}},
		}
		fmt.Println("query", query)

		ctx := context.Background()

		// subscribe to future events
		logsChan := make(chan gethTypes.Log, 10)
		sub, err := wssEthereumClient.SubscribeFilterLogs(ctx, query, logsChan)
		if err != nil {
			log.Fatal(err)
		}

		// add old events to the channel
		logs, err := httpEthereumClient.FilterLogs(ctx, query)
		if err != nil {
			log.Fatal(err)
		}

		fmt.Println("got old events", logs)

		// write past logs
		for _, log := range logs {
			fmt.Println("writing")
			logsChan <- log
			fmt.Println("wrote")
		}

		for {
			select {
			case err := <-sub.Err():
				log.Fatal(err)
			case vLog := <-logsChan:
				fmt.Println("got log", vLog)
				message, coin, err := LogToMessage(vLog)
				if err != nil {
					logger.Error("unable to parse log", "log", vLog)
					return
				}

				operation := func() error {
					fmt.Println("starting operation", message)
					err := processMessage(logger, sourceChain, axelarClient, rippleClient, addressBook, privateKey, httpEthereumClient, message, coin)
					fmt.Println("operation", message, "failed with", err)
					return err
				}

				b := backoff.NewExponentialBackOff()
				b.MaxElapsedTime = 5 * time.Minute

				err = backoff.Retry(operation, b)
				if err != nil {
					log.Printf("Operation failed despite retries: %v", err)
				}
			}
		}
	}
}

// CurrentEthereumBlockHeight returns the current block height of the Ethereum blockchain.
// It takes a pointer to an initialized ethereum.Client as its parameter.
// On success, it returns the block number (height) as int64 and nil error.
// On failure, it returns -1 and the error encountered.
func CurrentEthereumBlockHeight(client *ethclient.Client) (int64, error) {
	// Ensure the client is not nil to avoid a panic.
	if client == nil {
		return -1, fmt.Errorf("Ethereum client is nil")
	}

	header, err := client.HeaderByNumber(context.Background(), nil)
	if err != nil {
		return -1, err
	}

	return header.Number.Int64(), nil
}

const contractCallABIJSON = `[{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"sender","type":"address"},{"indexed":false,"internalType":"string","name":"destinationChain","type":"string"},{"indexed":false,"internalType":"string","name":"destinationContractAddress","type":"string"},{"indexed":true,"internalType":"bytes32","name":"payloadHash","type":"bytes32"},{"indexed":false,"internalType":"bytes","name":"payload","type":"bytes"}],"name":"ContractCall","type":"event"}]`

func LogToMessage(log gethTypes.Log) (Message, Coin, error) {
	fmt.Printf("Log Block Number: %v\n", log.BlockNumber)
	fmt.Printf("Log Index: %v\n", log.Index)

	fmt.Println("log", log)

	event := struct {
		Sender                     gethCommon.Address
		PayloadHash                [32]byte
		DestinationChain           string
		DestinationContractAddress string
		Payload                    []byte
	}{}

	contractCallABI, err := abi.JSON(strings.NewReader(contractCallABIJSON))
	if err != nil {
		panic(err)
	}

	indexed := make([]abi.Argument, 0)
	for _, input := range contractCallABI.Events["ContractCall"].Inputs {
		if input.Indexed {
			indexed = append(indexed, input)
		}
	}

	err = contractCallABI.UnpackIntoInterface(&event, "ContractCall", log.Data)
	if err != nil {
		return Message{}, Coin{}, Wrap(err, "failed to unpack event %s:%s", log.TxHash, log.Index)
	}

	// parse topics without event name
	if err := abi.ParseTopics(&event, indexed, log.Topics[1:]); err != nil {
		panic(err)
	}

	fmt.Printf("Event: %+v\n", event)

	data, err := ABIDecode([]string{"string", "uint256", "bytes"}, event.Payload)
	if err != nil {
		return Message{}, Coin{}, Wrap(err, "failed to abi decode event payload %s:%s", log.TxHash, log.Index)
	}

	innerPayloadHash := hex.EncodeToString(gethCrypto.Keccak256(data[2].([]byte)))
	denom := data[0].(string)
	amount := data[1].(*big.Int)
	payloadHash, err := HashPayload(denom, amount, innerPayloadHash)
	if err != nil {
		return Message{}, Coin{}, Wrap(err, "failed to hash payload %s:%s", log.TxHash, log.Index)
	}

	return Message{
		CcId:               CCID{Chain: "ethereum", Id: log.TxHash.String() + ":" + strconv.FormatUint(uint64(log.Index), 10)},
		SourceAddress:      gethCommon.BytesToAddress(event.Sender[:]).String(),
		DestinationChain:   event.DestinationChain,
		DestinationAddress: event.DestinationContractAddress,
		PayloadHash:        payloadHash,
	}, Coin{Denom: denom, Amount: amount.String()}, nil
}

func processMessage(logger *slog.Logger, sourceChain string, axelarClient AxelarClient, rippleClient *RippleClient, addressBook AddressBook, privateKey *ecdsa.PrivateKey, ethereumClient *ethclient.Client, message Message, coin Coin) error {
	logger = logger.With("cc_id", message.CcId, "source_chain", sourceChain)

	pollId, expiresAt, err := startPoll(addressBook.SourceGateway, addressBook.VotingVerifier, axelarClient, message)
	if err != nil {
		return err
	}
	logger.Info("poll started", "poll_id", pollId, "expires_at", expiresAt)

	for {
		height, err := getAxelarBlockHeight(axelarClient)
		if err != nil {
			return err
		}
		logger.Info("waiting for poll to end", "current_height", height, "expires_at", expiresAt)
		if height >= expiresAt {
			break
		}
		time.Sleep(time.Second * 2)
	}

	status, err := endPoll(addressBook.VotingVerifier, axelarClient, pollId)
	if err != nil && strings.Contains(err.Error(), "poll is not in progress") {
		logger.Info("Poll was not in progress, end poll not needed.")
		// continue
	} else if err != nil {
		return err
	} else if status != "succeeded_on_chain" {
		return fmt.Errorf("got message status %s for %s", status, message.CcId)
	}

	err = routeMessage(addressBook.SourceGateway, axelarClient, message)
	if err != nil {
		return err
	}

	// SEPARATE FLOWS HERE

	sessionId, err := constructProof(addressBook.MultisigProver, axelarClient, message.CcId, coin)
	if err != nil {
		return err
	}
	logger.Info("got session id", "session_id", sessionId)

	for {
		completed, expiresAt, err := queryMultisigState(axelarClient, sessionId)
		if err != nil {
			return err
		}
		height, err := getAxelarBlockHeight(axelarClient)
		if err != nil {
			return err
		}
		logger.Info("waiting signing session to end", "current_height", height, "expires_at", expiresAt)
		if completed {
			break
		}
		if height > expiresAt {
			return fmt.Errorf("multisig session %d expired before completion", sessionId)
		}

		time.Sleep(2 * time.Second)
	}

	proof, proofId, err := getProof(addressBook.MultisigProver, axelarClient, sessionId)
	if err != nil {
		panic(err)
	}
	logger.Info("got proof", "proof_id", proofId, "proof", proof)

	if sourceChain == "xrpl" {
		isExecuted, err := checkIfCommandExecutedOnEthereum(ethereumClient, proofId)
		if err != nil {
			return err
		}

		if isExecuted {
			logger.Info("already executed")
		} else {
			txHash, err := ethereumExecuteProof(ethereumClient, proof, privateKey)
			if err != nil {
				return err
			}

			logger.Info("broadcasted tx", "tx_hash", txHash)
		}

		// TODO: run it only if needed
		txHash, err := ValidateTokenTransfer(ethereumClient, os.Getenv("ETH_AXELAR_GATEWAY"), privateKey, proofId, message, coin)
		if err != nil {
			logger.Info("failed to validate token transfer (already executed or not a token transfer)", "err", err)
		} else {
			logger.Info("validated token transfer", "tx_hash", txHash, "err", err)
		}
	} else {
		txHash := SHA512Half("54584E00", proof)
		tx, err := rippleFetchTransaction(rippleClient, txHash)
		if (err != nil && strings.Contains(err.Error(), "not found")) || (err == nil && tx.Meta.TransactionResult == "") {
			// not found, execute
			logger.Info("did not find transaction, executing...")
			time.Sleep(5 * time.Second)
			_, err := rippleExecuteProof(rippleClient, proof)
			if err != nil {
				return err
			}
		} else if err != nil {
			return err
		}

		timeout := time.Now().Add(time.Second * 120)
		for {
			logger.Info("waiting for tx to be validated", "tx", tx)
			if tx.Meta.TransactionResult != "" && tx.Validated {
				break
			}
			if time.Now().After(timeout) {
				return errors.New("transaction validation timed out")
			}
			time.Sleep(time.Second * 2)
			tx, err = rippleFetchTransaction(rippleClient, txHash)
			if err != nil {
				return err
			}
		}

		xrplVotingVerifier := os.Getenv("XRPL_VOTING_VERIFIER")
		xrplProver := os.Getenv("XRPL_MULTISIG_PROVER")

		zero := [32]byte{}
		message := Message{CcId: CCID{Id: txHash + ":0", Chain: "xrpl"}, SourceAddress: os.Getenv("XRPL_MULTISIG_ADDRESS"), DestinationChain: "xrpl", DestinationAddress: tx.Destination, PayloadHash: hex.EncodeToString(zero[:])}
		pollId, expiresAt, err := startPoll(os.Getenv("XRPL_GATEWAY"), xrplVotingVerifier, axelarClient, message)
		if err != nil {
			return err
		}
		logger.Info("got poll for submitted tx", "poll_id", pollId, "expires_at", expiresAt)

		for {
			height, err := getAxelarBlockHeight(axelarClient)
			if err != nil {
				return err
			}
			logger.Info("waiting for poll to end", "current_height", height, "expires_at", expiresAt)
			if height >= expiresAt {
				break
			}
			time.Sleep(time.Second * 2)
		}

		_, err = endPoll(xrplVotingVerifier, axelarClient, pollId)
		if err != nil && strings.Contains(err.Error(), "poll is not in progress") {
			logger.Info("Poll was not in progress, end poll not needed.")
			// continue
		} else if err != nil {
			return err
		}

		var status string
		if tx.Meta.TransactionResult == "tesSUCCESS" {
			status = "succeeded_on_chain"
			logger.Info("poll ended. updating tx status....")
			updateTxStatus(xrplProver, axelarClient, tx.SignerPubKeys(), sessionId, message.CcId, status)
			logger.Info("tx status updated")
		} else {
			// status = "failed_on_chain"
			// for now do not even update tx status, we need to monitor this case
		}
	}

	logger.Info("done")

	return nil
}

func RippleFetchDepositMessages(rippleClient *RippleClient, multisigAddress string, ledgerIndexMin int64, ledgerIndexMax int64) ([]MessageWithCoin, error) {
	params := RequestPayload{
		Method: "account_tx",
		Params: []AccountTxParams{
			{
				Account:        multisigAddress,
				LedgerIndexMin: ledgerIndexMin,
				LedgerIndexMax: ledgerIndexMax,
			},
		},
	}
	var responseData AccountTxResponsePayload
	err := JsonRPCRequest(rippleClient.RPCUrl, params, &responseData)
	if err != nil {
		return []MessageWithCoin{}, err
	}

	var messages []MessageWithCoin
	for _, outerTx := range responseData.Result.Transactions {
		tx := outerTx.Tx
		if tx.Destination != multisigAddress || tx.Account == multisigAddress || tx.TransactionType != "Payment" {
			continue
		}

		destination_chain, err := tx.GetMemo("destination_chain")
		if err != nil {
			//log.Println("found payment tx without destination_chain", tx.Hash)
			continue
		}

		destination_address, err := tx.GetMemo("destination_address")
		if err != nil {
			log.Println("found payment tx without destination_address", tx.Hash)
			continue
		}

		innerPayloadHash, err := tx.GetMemo("payload_hash")
		if err != nil {
			log.Println("found payment tx without payload_hash", tx.Hash)
			continue
		}

		var outerPayloadHash string
		var coin Coin

		switch amt := tx.Amount.(type) {
		case string:
			// XRP
			value := new(big.Int)
			value, ok := value.SetString(amt, 10)
			if !ok {
				return []MessageWithCoin{}, errors.New("could not parse native XRP amount")
			}
			outerPayloadHash, err = HashPayload("uxrp", value, innerPayloadHash)
			if err != nil {
				panic(err)
			}
			coin = Coin{Amount: amt, Denom: "uxrp"}
		case map[string]interface{}:
			// Token
			value, err := EthToWei(amt["value"].(string)) // TODO: according to coin
			if err != nil {
				return []MessageWithCoin{}, err
			}
			symbol := xrplTokenToDenom(amt["currency"].(string), amt["issuer"].(string))
			outerPayloadHash, err = HashPayload(symbol, value, innerPayloadHash)
			if err != nil {
				panic(err)
			}
			coin = Coin{Amount: value.String(), Denom: "uweth"}
		}

		message := Message{
			CcId:               CCID{Chain: "xrpl", Id: tx.Hash + ":0"},
			DestinationChain:   destination_chain,
			DestinationAddress: destination_address,
			PayloadHash:        outerPayloadHash,
			SourceAddress:      tx.Account,
		}

		messages = append(messages, MessageWithCoin{Message: message, Coin: coin})
	}
	return messages, nil
}

// EthToWei converts an Ethereum amount (as a decimal string) to wei (as *big.Int),
// without using floating point arithmetic to avoid losing precision.
func EthToWei(ethStr string) (*big.Int, error) {
	parts := strings.Split(ethStr, ".")
	weiValue := new(big.Int)

	wholePart := new(big.Int)
	if _, ok := wholePart.SetString(parts[0], 10); !ok {
		return nil, fmt.Errorf("invalid whole part")
	}
	wholePartWei := new(big.Int).Mul(wholePart, big.NewInt(1e18))
	weiValue.Add(weiValue, wholePartWei)

	// Convert the fractional part, if present.
	if len(parts) == 2 {
		fracPart := parts[1]
		fracLength := len(fracPart)

		// Ensure the fractional part is not longer than 18 digits
		if fracLength > 18 {
			return nil, fmt.Errorf("fractional part too long")
		}

		fracPartWei := new(big.Int)
		if _, ok := fracPartWei.SetString(fracPart, 10); !ok {
			return nil, fmt.Errorf("invalid fractional part")
		}
		// Adjust for the length of the fractional part.
		for i := 0; i < 18-fracLength; i++ {
			fracPartWei.Mul(fracPartWei, big.NewInt(10))
		}
		weiValue.Add(weiValue, fracPartWei)
	}

	return weiValue, nil
}

// TODO: ITS HUB? ON CHAIN?
func xrplTokenToDenom(currency string, issuer string) string {
	if currency == "ETH" && issuer == os.Getenv("XRPL_MULTISIG_ADDRESS") {
		return "uweth"
	} else if currency == "XRP" && issuer == "" {
		return "uxrp"
	}
	panic(fmt.Errorf("invalid xrpl token currency=%s issuer=%s", currency, issuer))
}

func updateTxStatus(proverAddress string, axelarClient AxelarClient, signerPublicKeys []PublicKey, multisigSessionId int64, messageId CCID, messageStatus string) error {
	command := map[string]map[string]interface{}{
		"update_tx_status": {
			"message_status":      messageStatus,
			"multisig_session_id": strconv.FormatInt(multisigSessionId, 10),
			"message_id":          messageId,
			"signer_public_keys":  signerPublicKeys,
		},
	}
	_, err := axelarWasmExecute(axelarClient, proverAddress, command)
	return Wrap(err, "failed to update tx status for %s", messageId)
}

func rippleFetchTransaction(client *RippleClient, hash string) (MultisigPaymentTx, error) {
	payload := map[string]interface{}{
		"method": "tx",
		"params": []map[string]string{
			{
				"transaction": hash,
			},
		},
	}

	var data struct {
		Result MultisigPaymentTx `json:"result"`
	}
	err := JsonRPCRequest(client.RPCUrl, payload, &data)
	if err == nil && data.Result.Status != "success" {
		return data.Result, fmt.Errorf("failed to fetch transaction %s: %s", hash, data.Result.ErrorMessage)
	}
	return data.Result, err
}

func rippleExecuteProof(client *RippleClient, proofHex string) ([]XRPLSignerWrapper, error) {
	payload := map[string]interface{}{
		"method": "submit",
		"params": []map[string]string{
			{
				"tx_blob": proofHex,
			},
		},
	}

	var data struct {
		Result struct {
			TXJson struct {
				Signers []XRPLSignerWrapper
			} `json:"tx_json"`
		} `json:"result"`
	}
	err := JsonRPCRequest(client.RPCUrl, payload, &data)
	return data.Result.TXJson.Signers, err
}

func JsonRPCRequest(url string, payload interface{}, responseData interface{}) error {
	// Marshal the payload to JSON
	jsonData, err := json.Marshal(payload)
	if err != nil {
		panic(err)
	}

	req, err := http.NewRequest("POST", url, bytes.NewBuffer(jsonData))
	if err != nil {
		panic(err)
	}

	// Set the Content-Type header
	req.Header.Set("Content-Type", "application/json")

	// Initialize an HTTP client and send the request
	httpClient := &http.Client{}
	resp, err := httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	// Read and print the response body
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}

	return json.Unmarshal(body, responseData)
}

const validateTokenTransferABI = `[{"inputs":[{"internalType":"bytes32","name":"commandId","type":"bytes32"},{"internalType":"string","name":"sourceChain","type":"string"},{"internalType":"string","name":"sourceAddress","type":"string"},{"internalType":"address","name":"destinationAddress","type":"address"},{"internalType":"string","name":"denom","type":"string"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"validateTokenTransfer","outputs":[{"internalType":"bool","name":"valid","type":"bool"}],"stateMutability":"nonpayable","type":"function"}]`

// ValidateTokenTransfer calls the similarly named function on the axelar gateway in order to release tokens to the user, after execute
// TODO: unclear if and how tokens will be automatically sent to the user when ITS Hub will be used.
func ValidateTokenTransfer(client *ethclient.Client, contractAddress string, privateKey *ecdsa.PrivateKey, commandIdHex string, message Message, coin Coin) (string, error) {
	parsedABI, err := abi.JSON(strings.NewReader(validateTokenTransferABI))
	if err != nil {
		panic(Wrap(err, "failed to parse ValidateTokenTransfer ABI"))
	}

	var commandId [32]byte
	copy(commandId[:], gethCommon.FromHex(commandIdHex))

	amount, ok := big.NewInt(0).SetString(coin.Amount, 10)
	if !ok {
		return "", fmt.Errorf("ValidateTokenTransfer: failed to parse coin amount %s", coin.Amount)
	}

	data, err := parsedABI.Pack("validateTokenTransfer",
		commandId,
		message.CcId.Chain,
		message.SourceAddress,
		gethCommon.HexToAddress(message.DestinationAddress),
		coin.Denom,
		amount,
	)
	if err != nil {
		return "", Wrap(err, "failed to pack data for validateTokenTransfer")
	}

	fromAddress := gethCrypto.PubkeyToAddress(privateKey.PublicKey)
	nonce, err := client.PendingNonceAt(context.Background(), fromAddress)
	if err != nil {
		return "", Wrap(err, "failed to fetch account nonce")
	}

	addr := gethCommon.HexToAddress(contractAddress)

	gasLimit, err := client.EstimateGas(context.Background(), ethereum.CallMsg{
		To:   &addr,
		Data: data,
	})
	if err != nil {
		return "", Wrap(err, "failed to estimate gas: %w")
	}

	gasPrice, err := client.SuggestGasPrice(context.Background())
	if err != nil {
		return "", Wrap(err, "ValidateTokenTransfer: failed to suggest gas price")
	}

	tx := gethTypes.NewTransaction(nonce, gethCommon.HexToAddress(contractAddress), big.NewInt(0), gasLimit, gasPrice, data)

	chainID, err := client.NetworkID(context.Background())
	if err != nil {
		return "", Wrap(err, "ValidateTokenTransfer: failed to get network ID")
	}

	signedTx, err := gethTypes.SignTx(tx, gethTypes.NewEIP155Signer(chainID), privateKey)
	if err != nil {
		return "", Wrap(err, "ValidateTokenTransfer: failed to sign transaction")
	}

	err = client.SendTransaction(context.Background(), signedTx)
	if err != nil {
		return "", Wrap(err, "ValidateTokenTransfer: failed to send transaction")
	}

	return signedTx.Hash().Hex(), nil
}

// ethereumExecuteProof executes a signed message on the AxelarGateway contract on ethereum
func ethereumExecuteProof(client *ethclient.Client, proofHex string, privateKey *ecdsa.PrivateKey) (string, error) {
	publicKey := privateKey.Public()
	publicKeyECDSA, ok := publicKey.(*ecdsa.PublicKey)
	if !ok {
		return "", errors.New("error casting public key to ECDSA")
	}

	fromAddress := gethCrypto.PubkeyToAddress(*publicKeyECDSA)
	nonce, err := client.PendingNonceAt(context.Background(), fromAddress)
	if err != nil {
		return "", Wrap(err, "failed to get nonce")
	}

	value := big.NewInt(0)
	gasLimit := uint64(300000)
	gasPrice, err := client.SuggestGasPrice(context.Background())
	if err != nil {
		return "", Wrap(err, "Failed to suggest gas price")
	}

	data := gethCommon.FromHex(proofHex)

	ethAxelarGatewayAddress := gethCommon.HexToAddress(os.Getenv("ETH_AXELAR_GATEWAY"))

	// Create the transaction
	tx := gethTypes.NewTransaction(nonce, ethAxelarGatewayAddress, value, gasLimit, gasPrice, data)

	chainID, err := client.NetworkID(context.Background())
	if err != nil {
		return "", Wrap(err, "Failed to fetch chainID")
	}

	signedTx, err := gethTypes.SignTx(tx, gethTypes.NewEIP155Signer(chainID), privateKey)
	if err != nil {
		return "", Wrap(err, "Failed to sign transaction")
	}

	err = client.SendTransaction(context.Background(), signedTx)
	if err != nil {
		return "", Wrap(err, "Failed to send transaction")
	}

	return signedTx.Hash().Hex(), nil
}

// checkIfCommandExecutedOnEthereum returns true if command already executed (no need to execute it again)
func checkIfCommandExecutedOnEthereum(client *ethclient.Client, commandId string) (bool, error) {
	contractAbi, err := abi.JSON(strings.NewReader(`[{"constant":false,"inputs":[{"name":"commandId","type":"bytes32"}],"name":"isCommandExecuted","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"view","type":"function"}]`))
	if err != nil {
		panic(err)
	}

	addr := gethCommon.HexToAddress(os.Getenv("ETH_AXELAR_GATEWAY"))
	contract := bind.NewBoundContract(addr, contractAbi, client, client, client)
	var out []interface{}
	commandIDBytes := gethCommon.HexToHash(commandId)

	err = contract.Call(&bind.CallOpts{Context: context.Background()}, &out, "isCommandExecuted", commandIDBytes)
	if err != nil {
		return false, Wrap(err, "failed to call isCommandExecuted for %s", commandId)
	}

	executed := out[0].(bool)
	return executed, nil
}

// getProof returns the signed transaction for a multisig session id
func getProof(proverAddress string, axelarClient AxelarClient, sessionId int64) (string, string, error) {
	query := map[string]map[string]string{
		"get_proof": {"multisig_session_id": strconv.FormatInt(sessionId, 10)},
	}
	data := struct {
		Data struct {
			// both
			Status interface{} `json:"status"`

			// xrpl
			UnsignedTxHash string `json:"unsigned_tx_hash,omitempty"`
			TxBlob         string `json:"tx_blob,omitempty"`

			// eth
			Data interface{} `json:"data"`
		} `json:"data"`
	}{}
	err := axelarWasmQueryState(axelarClient, proverAddress, query, &data)
	if err != nil {
		return "", "", Wrap(err, "failed to get proof")
	}
	if status, ok := data.Data.Status.(string); ok {
		if status == "completed" {
			return data.Data.TxBlob, data.Data.UnsignedTxHash, nil
		} else {
			return "", "", fmt.Errorf("get_proof status was %s", status)
		}
	} else if status, ok := data.Data.Status.(map[string]interface{}); ok {
		executeData, ok := status["completed"].(map[string]interface{})["execute_data"]
		if !ok {
			return "", "", fmt.Errorf("get_proof invalid status")
		}
		commandId, ok := data.Data.Data.(map[string]interface{})["commands"].([]interface{})[0].(map[string]interface{})["id"]
		if !ok {
			return "", "", errors.New("get_proof response inner data bad format")
		}
		return executeData.(string), commandId.(string), nil
	} else {
		return "", "", errors.New("get_proof response bad format")
	}
}

// queryMultisigState returns if multisig is completed and when it will expire
func queryMultisigState(axelarClient AxelarClient, sessionId int64) (bool, int64, error) {
	query := map[string]map[string]string{
		"get_multisig": {"session_id": strconv.FormatInt(sessionId, 10)},
	}
	data := struct {
		Data struct {
			State     interface{} `json:"state"`
			ExpiresAt int64       `json:"expires_at"`
		} `json:"data"`
	}{}
	err := axelarWasmQueryState(axelarClient, os.Getenv("MULTISIG"), query, &data)
	if err != nil {
		return false, 0, Wrap(err, "failed to query multisig state for session %d", sessionId)
	}
	_, ok := data.Data.State.(map[string]interface{})

	return ok, data.Data.ExpiresAt, nil
}

// querySessionId fetches multisig session id for a message, if a session has already started
// XRPLMultisigProver does not resign same message and throws error if a session has started
func querySessionId(proverAddress string, axelarClient AxelarClient, messageId CCID) (int64, error) {
	data := struct {
		Data int64 `json:"data"`
	}{}
	err := axelarWasmQueryState(axelarClient, proverAddress, map[string]map[string]interface{}{
		"get_multisig_session_id": {
			"message_id": messageId,
		},
	}, &data)
	return data.Data, err
}

// endPoll finishes a voting verifier poll, must be called after poll expired
func endPoll(votingVerifierAddress string, axelarClient AxelarClient, pollId int64) (string, error) {
	command := map[string]map[string]interface{}{
		"end_poll": {
			"poll_id": strconv.FormatInt(pollId, 10),
		},
	}
	events, err := axelarWasmExecute(axelarClient, votingVerifierAddress, command)
	if err != nil && strings.Contains(err.Error(), "poll is not in progress") {
		return "", err
	}
	event := eventByType(events, "wasm-poll_ended")
	if !event.Exists() {
		return "", errors.New("no wasm-poll_ended event after end_poll")
	}
	jsonResults := event.ToMap()["results"]

	// Unmarshal into a slice of pointer to string
	var results []*string
	if err := json.Unmarshal([]byte(jsonResults), &results); err != nil {
		return "", err
	}
	if len(results) == 0 {
		return "", errors.New("no results in end poll")
	}

	result := results[0]
	if result == nil {
		return "", errors.New("workers did not vote on message, is ampd running?")
	}
	return *result, nil
}

// constructProof starts a signing round for the outgoing message (the proof)
func constructProof(proverAddress string, axelarClient AxelarClient, messageID CCID, coin Coin) (int64, error) {
	var command interface{}

	if proverAddress == os.Getenv("XRPL_MULTISIG_PROVER") {
		sessionId, err := querySessionId(proverAddress, axelarClient, messageID)
		if err != nil {
			// ignore error
			// panic(err)
		}
		if sessionId != 0 {
			return sessionId, nil
		}
		command = XRPLConstructProofCommand{XRPLConstructProof{messageID, coin}}
	} else {
		command = BatchConstructProofCommand{BatchConstructProof{[]CCID{messageID}}}
	}
	events, err := axelarWasmExecute(axelarClient, proverAddress, command)
	if err != nil {
		return 0, Wrap(err, "construct proof contract call failed for %s", messageID.Id)
	}
	event := eventByType(events, "wasm-signing_started")
	attrs := event.ToMap()
	sessionId, ok := attrs["session_id"]
	if !ok {
		return 0, errors.New("no session_id attribute")
	}
	return strconv.ParseInt(sessionId, 0, 64)
}

// routeMessage routes a verified outgoing message to the source gateway axelar contract
func routeMessage(gatewayAddress string, axelarClient AxelarClient, message Message) error {
	command := map[string][]Message{"route_messages": {message}}
	events, err := axelarWasmExecute(axelarClient, gatewayAddress, command)
	event := eventByType(events, "wasm-message_routing_failed")
	if event.Exists() {
		return fmt.Errorf("routing failed for %s", message.CcId)
	}
	return Wrap(err, "failed to route message %s", message.CcId)
}

type ServiceInfoRequestPayload struct {
	Method string        `json:"method"`
	Params []interface{} `json:"params"`
}

func getXRPLBlockHeight(rippleClient *RippleClient) (int64, error) {
	params := ServiceInfoRequestPayload{
		Method: "server_info",
		Params: []interface{}{},
	}
	var responseData struct {
		Result struct {
			Info struct {
				ValidatedLedger struct {
					Seq int64 `json:"seq"`
				} `json:"validated_ledger"`
			} `json:"info"`
		} `json:"result"`
	}
	err := JsonRPCRequest(rippleClient.RPCUrl, params, &responseData)
	if err != nil {
		return 0, err
	}
	return responseData.Result.Info.ValidatedLedger.Seq, nil
}

// getBlockHeight fetches current block height on the axelar network
func getAxelarBlockHeight(axelarClient AxelarClient) (int64, error) {
	var result struct {
		Result struct {
			Block struct {
				Header struct {
					Height string `json:"height"`
				} `json:"header"`
			} `json:"block"`
		} `json:"result"`
	}
	err := tendermintAPIRequest(axelarClient, "/block", map[string]string{}, &result)
	if err != nil {
		return 0, Wrap(err, "failed to fetch latest block height")
	}
	return strconv.ParseInt(result.Result.Block.Header.Height, 0, 64)
}

// startPoll starts a poll to verify a message, or finds already started poll
func startPoll(gatewayAddress string, votingVerifierAddress string, axelarClient AxelarClient, message Message) (int64, int64, error) {
	command := map[string][]Message{"verify_messages": {message}}
	events, err := axelarWasmExecute(axelarClient, gatewayAddress, command)
	if err != nil {
		return 0, 0, Wrap(err, "failed to start poll with axelard for %s", message.CcId)
	}

	event := eventByType(events, "wasm-messages_poll_started")
	pollId, expiresAt, err := parsePollStartedEvent(event)
	if err != nil {
		return 0, 0, Wrap(err, "failed to parse poll started event for %s", message.CcId)
	}

	if pollId != 0 {
		return pollId, expiresAt, nil
	}

	parts := strings.Split(message.CcId.Id, ":")
	eventIndex, err := strconv.ParseInt(parts[1], 0, 64)
	if err != nil {
		panic(err)
	}
	event, err = findPollStartedEvent(axelarClient, votingVerifierAddress, parts[0], eventIndex)
	if err != nil {
		return 0, 0, Wrap(err, "failed to find past poll started event for %s", message.CcId)
	}

	return parsePollStartedEvent(event)
}

// findPollStartedEvent finds past poll started events from tendermint rest api
func findPollStartedEvent(axelarClient AxelarClient, votingVerifierAddress string, hash string, eventIndex int64) (Event, error) {
	var result struct {
		Result struct {
			Txs []struct {
				TxResult struct {
					Log string `json:"log"`
				} `json:"tx_result"`
			} `json:"txs"`
		} `json:"result"`
	}

	err := tendermintAPIRequest(axelarClient, "/tx_search", map[string]string{
		"query": fmt.Sprintf("\"wasm-messages_poll_started.messages CONTAINS '%s' and wasm-messages_poll_started._contract_address = '%s' and wasm-messages_poll_started.messages CONTAINS ':%d,'\"", hash, votingVerifierAddress, eventIndex),
	}, &result)
	if err != nil {
		return Event{}, Wrap(err, "HTTP request to find past poll started events failed")
	}

	if len(result.Result.Txs) == 0 {
		return Event{}, errors.New("no tendermint transactions found")
	}

	log := result.Result.Txs[0].TxResult.Log
	var eventLogs []EventLog
	json.Unmarshal([]byte(log), &eventLogs)

	if len(eventLogs) == 0 {
		return Event{}, errors.New("no event logs on tendermint transaction")
	}

	event := eventByType(eventLogs[0].Events, "wasm-messages_poll_started")
	return event, nil
}

// tendermintAPIRequest fetches data from tendermint REST API of axelar amplifier network
func tendermintAPIRequest(axelarClient AxelarClient, path string, parameters map[string]string, result interface{}) error {
	baseURL, err := url.Parse(axelarClient.RPCUrl)
	if err != nil {
		return err
	}

	// Add the path to the base URL
	baseURL.Path += path

	// Manually construct the query string to ensure proper encoding
	queryParts := make([]string, 0, len(parameters))
	for key, value := range parameters {
		encodedKey := url.PathEscape(key)
		encodedValue := url.PathEscape(value)
		queryParts = append(queryParts, fmt.Sprintf("%s=%s", encodedKey, encodedValue))
	}
	encodedQuery := strings.Join(queryParts, "&")
	baseURL.RawQuery = encodedQuery

	// fmt.Println("querying", baseURL.String())

	// Perform the HTTP GET request
	resp, err := http.Get(baseURL.String())
	if err != nil {
		return Wrap(err, "tendermint rest api call to", path, "failed")
	}
	defer resp.Body.Close()

	// Read the response body
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return Wrap(err, "failed to read tendermint rest api response")
	}

	// Unmarshal the JSON response into the result
	if err := json.Unmarshal(body, result); err != nil {
		return Wrap(err, "tendermint rest api response json decode error")
	}

	return nil
}

// axelarWasmQueryState queries the state of a contract in amplifier network
func axelarWasmQueryState(axelarClient AxelarClient, address string, command any, output any) error {
	commandJson, err := json.Marshal(command)
	if err != nil {
		panic(err)
	}

	cmd := exec.Command("axelard", "query", "wasm", "contract-state", "smart", address, string(commandJson), "--output", "json", "--node", axelarClient.RPCUrl)
	cmd.Stderr = os.Stderr
	strOutput, err := cmd.Output()
	if err != nil {
		return Wrap(err, "failed to query contract state")
	}
	// fmt.Println("queried state", string(strOutput))
	err = json.Unmarshal(strOutput, &output)
	return err

}

// axelarWasmExecute executes a command on a contract on axelar network at designated address and returns list of events
func axelarWasmExecute(axelarClient AxelarClient, address string, command any) ([]Event, error) {
	commandJson, err := json.Marshal(command)
	if err != nil {
		panic(err)
	}

	// var stderrBuf bytes.Buffer
	args := []string{"tx", "wasm", "execute", address, string(commandJson), "--from", axelarClient.FromWallet, "--gas-prices", "0.025uwasm", "--gas", "auto", "--gas-adjustment", "2", "-y", "--node", axelarClient.RPCUrl}
	fmt.Println("calling axelard", args)
	var stderrBuf bytes.Buffer
	cmd := exec.Command("axelard", args...)
	cmd.Stderr = &stderrBuf
	//cmd.Stdout = os.Stdout
	output, err := cmd.Output()
	if stderrBuf.Len() > 0 {
		_, writeErr := os.Stderr.Write(stderrBuf.Bytes())
		if writeErr != nil {
			panic(writeErr)
		}
	}
	if err != nil {
		return []Event{}, Wrap(err, "axelar wasm execute with stderr %s", stderrBuf.Bytes())
	}

	var cmdOutput CommandOutput
	json.Unmarshal(output, &cmdOutput)

	if len(cmdOutput.Logs) == 0 {
		return []Event{}, errors.New("no logs produced")
	}

	return cmdOutput.Logs[0].Events, nil
}

// eventByType finds an event based on type from a list of events.
// returns empty event if not found
func eventByType(events []Event, typ string) Event {
	for _, event := range events {
		if event.Type == typ {
			return event
		}
	}
	return Event{}
}

// parsePollStartedEvent extracts pollId and expiresAt attributes from a poll started event
func parsePollStartedEvent(event Event) (int64, int64, error) {
	var pollID, expiresAt int64
	var err error
	for _, attr := range event.Attributes {
		if attr.Key == "expires_at" {
			expiresAt, err = strconv.ParseInt(attr.Value, 0, 64)
		} else if attr.Key == "poll_id" {
			// poll_id is a quoted number (json string)
			pollID, err = strconv.ParseInt(strings.Trim(attr.Value, "\""), 0, 64)
		}
	}
	return pollID, expiresAt, Wrap(err, "failed to parsePollStartedEvent")
}

func HashPayload(denom string, amount *big.Int, payloadHash string) (hashHex string, err error) {
	// Encode denom, amount, and the payload hash
	encodedData, err := ABIEncode([]string{"string", "uint256", "bytes32"}, []interface{}{denom, amount, gethCommon.HexToHash(payloadHash)})
	if err != nil {
		return "", fmt.Errorf("error encoding data: %v", err)
	}

	//fmt.Println(hex.EncodeToString(encodedData))

	// Compute the Keccak256 hash
	hash := gethCrypto.Keccak256(encodedData)

	return hex.EncodeToString(hash), nil
}

func ABIEncode(types []string, values []interface{}) ([]byte, error) {
	arguments := abi.Arguments{}
	for _, t := range types {
		ty, err := abi.NewType(t, "", nil)
		if err != nil {
			panic(err)
		}
		arguments = append(arguments, abi.Argument{Type: ty})
	}

	return arguments.Pack(values...)
}

func ABIDecode(types []string, data []byte) ([]interface{}, error) {
	arguments := abi.Arguments{}
	for _, t := range types {
		ty, err := abi.NewType(t, "", nil)
		if err != nil {
			panic(err)
		}
		arguments = append(arguments, abi.Argument{Type: ty})
	}

	return arguments.Unpack(data)
}

// Wrapf an error with fmt.Errorf(), returning nil if err is nil.
func Wrap(err error, format string, a ...interface{}) error {
	if err == nil {
		return nil
	}
	return fmt.Errorf(format+": %w", append(a, err)...)
}

func SHA512Half(prefix string, hexInput string) string {
	input, err := hex.DecodeString(prefix + hexInput)
	if err != nil {
		panic(err)
	}
	hash := sha512.Sum512(input)

	// Take the first half of the hash (256 bits / 32 bytes)
	halfHash := hash[:32]

	// Convert the half hash to a hexadecimal string for easy display
	return hex.EncodeToString(halfHash)
}
