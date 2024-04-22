package main

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strconv"
	"time"

	"github.com/cenkalti/backoff"
	pb "github.com/commonprefix/xrpl-relayer/axelar"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/ethereum/go-ethereum/common"
	amqp "github.com/rabbitmq/amqp091-go"
	"google.golang.org/grpc"
	"google.golang.org/protobuf/proto"
)

const CHAIN_NAME = "xrpl"
const STORAGE_FILE_PERMISSIONS = 0644

type CCID struct {
	Chain string `json:"chain"`
	Id    string `json:"id"`
}

type Coin struct {
	Amount string `json:"amount"` // todo: uint128
	Denom  string `json:"denom"`
}

func (c *Coin) String() string {
	return fmt.Sprintf("%s%s", c.Amount, c.Denom)
}

type XRPLConstructProof struct {
	MessageID CCID `json:"message_id"`
	Coin      Coin `json:"coin"`
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

type Memo struct {
	Memo MemoDetails `json:"Memo"`
}

type MemoDetails struct {
	MemoType string `json:"MemoType"`
	MemoData string `json:"MemoData"`
}

type XRPLTransactionInfo struct {
	Meta struct {
		TransactionResult string
	} `json:"meta"`
	Tx        *XRPLTransaction `json:"tx"`
	Validated bool             `json:"validated"`
}

type AccountTxResponsePayload struct {
	Result struct {
		Marker       *json.RawMessage
		Transactions []XRPLTransactionInfo `json:"transactions"`
	} `json:"result"`
}

type Task func(ctx context.Context) error

// forever runs the given task indefinitely until it returns an error or the context is cancelled.
func forever(ctx context.Context, task Task) error {
	for {
		select {
		case <-ctx.Done():
			return nil
		default:
			if err := task(ctx); err != nil {
				return err
			}
		}
	}
}

func main() {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	errChan := make(chan error, 4)

	grpcConn, err := grpc.Dial("your_server_address:port", grpc.WithInsecure())
	if err != nil {
		log.Fatalf("Failed to dial: %v", err)
	}
	defer grpcConn.Close()

	axelarClient := pb.NewAmplifierClient(grpcConn)

	xrplClient := XRPLClient{os.Getenv("XRPL_RPC_URL")}

	rmqConn, err := amqp.Dial("amqp://guest:guest@localhost:5672/")
	if err != nil {
		fmt.Println("Failed to connect to RabbitMQ")
		return
	}

	defer rmqConn.Close()

	sentinelWindowSize, err := strconv.ParseUint(os.Getenv("SENTINEL_WINDOW_SIZE"), 10, 32)
	if err != nil {
		fmt.Println("Invalid SENTINEL_WINDOW_SIZE", err)
		return
	}

	sentinelSleepTimeSeconds, err := strconv.Atoi(os.Getenv("SENTINEL_SLEEP_TIME_SECONDS"))
	if err != nil {
		fmt.Println("Invalid SENTINEL_SLEEP_TIME_SECONDS", err)
		return
	}
	verificationTimeoutSeconds, err := strconv.Atoi(os.Getenv("SENTINEL_VERIFICATION_TIME_SECONDS"))
	if err != nil {
		fmt.Println("Invalid SENTINEL_VERIFICATION_TIMEOUT_SECONDS", err)
		return
	}
	sentinel := Sentinel{
		storagePath:          os.Getenv("SENTINEL_STORAGE_PATH"),
		processingWindowSize: uint32(sentinelWindowSize),
		xrplMultisigAddress:  os.Getenv("XRPL_MULTISIG_ADDRESS"),
		xrplClient:           &xrplClient,
		axelarClient:         axelarClient,
		sleepTime:            time.Duration(sentinelSleepTimeSeconds) * time.Second,
		verificationTimeout:  time.Duration(verificationTimeoutSeconds) * time.Second,
	}
	sentinel.lastProcessedLedgerIndex, err = sentinel.ReadLastProcessedLedgerIndex()
	if err != nil {
		fmt.Println("Failed reading last processed ledger index", err)
		return
	}

	approverChannel, err := rmqConn.Channel()
	if err != nil {
		fmt.Println("Failed to open a rabbitmq channel")
		return
	}
	defer approverChannel.Close()
	approver := Approver{axelarClient: axelarClient, messageQueue: approverChannel, storagePath: os.Getenv("APPROVER_STORAGE_PATH")}
	approver.lastProcessedHeight, err = approver.ReadLastProcessedHeight()
	if err != nil {
		fmt.Println("Failed to read approver last processed height")
		return
	}

	includerChannel, err := rmqConn.Channel()
	if err != nil {
		fmt.Println("Failed to open a rabbitmq channel", err)
		return
	}
	defer includerChannel.Close()

	includerMaxRetries, err := strconv.ParseUint(os.Getenv("INCLUDER_MAX_RETRIES"))
	if err != nil {
		fmt.Println("Invalid INCLUDER_MAX_RETRIES", err)
		return
	}
	includer := Includer{messageQueue: includerChannel, xrplClient: &xrplClient, maxRetries: uint32(includerMaxRetries)}

	broadcasterChannel, err := rmqConn.Channel()
	if err != nil {
		fmt.Println("Failed to open a rabbitmq channel", err)
		return
	}
	defer broadcasterChannel.Close()
	broadcasterMaxRetries, err := strconv.ParseUint(os.Getenv("BROADCASTER_MAX_RETRIES"))
	if err != nil {
		fmt.Println("Invalid BROADCASTER_MAX_RETRIES", err)
		return
	}
	broadcaster := Broadcaster{axelarClient: axelarClient, messageQueue: broadcasterChannel, maxRetries: uint32(broadcasterMaxRetries)}

	tasks := []Task{sentinel.ProcessEvents, approver.ProcessEvents, includer.SubmitProofs, broadcaster.ConsumeQueueAndBroadcast}

	for _, task := range tasks {
		go func(t Task) {
			errChan <- forever(ctx, t)
		}(task)
	}

	if err := <-errChan; err != nil {
		fmt.Println("Received an error:", err)
		cancel()
	}
}

type Sentinel struct {
	storagePath              string
	lastProcessedLedgerIndex uint32
	processingWindowSize     uint32
	xrplMultisigAddress      string
	xrplClient               *XRPLClient
	axelarClient             pb.AmplifierClient
	sleepTime                time.Duration
	verificationTimeout      time.Duration
}

func (s *Sentinel) ProcessEvents(ctx context.Context) error {
	minLedgerIndex := s.lastProcessedLedgerIndex + 1
	validatedLedger, err := s.xrplClient.Ledger(ctx, XRPLLedgerRequest{"validated", false, false, false})
	if err != nil {
		return err
	}
	validatedLedgerIndex, err := strconv.ParseUint(validatedLedger.LedgerIndex, 10, 32)
	if err != nil {
		return err
	}
	maxLedgerIndex := min(minLedgerIndex+s.processingWindowSize, uint32(validatedLedgerIndex))
	var marker *json.RawMessage = nil
	currentLedger := s.lastProcessedLedgerIndex + 1
	processedTransactions := false
	for { // for each page
		// AccountTx(addr, minIndex, maxIndex) assumptions:
		// 1. it will return a non-nil marker if there are more pages to fetch
		// 2. it will return a nil marker if there are no more pages to fetch
		// 3. across all pages, it will return all transactions between minIndex and maxIndex
		// 4. it will not return unvalidated transactions if maxIndex is <= validatedIndex (according to some earlier time)
		txs, marker, err := s.xrplClient.AccountTx(ctx, s.xrplMultisigAddress, minLedgerIndex, maxLedgerIndex, marker)
		if err != nil {
			return err
		}
		for _, tx := range txs {
			if !tx.Tx.Validated {
				panic("found unvalidated tx even if requesting only validated transactions")
			}
			if tx.Tx.LedgerIndex > currentLedger {
				s.lastProcessedLedgerIndex = currentLedger
				currentLedger = tx.Tx.LedgerIndex
				err := s.PersistLastProcessedLedgerIndex()
				if err != nil {
					return err
				}
			}

			stream, err := s.axelarClient.Verify(ctx)
			if err == nil && stream != nil {
				defer stream.CloseSend()
			}
			if err != nil {
				return err
			}

			err = s.TryVerifyTransaction(ctx, tx.Tx, stream)
			if err != nil {
				fmt.Println("Failed verifying transaction", tx.Tx.Hash, "after multiple attempts, giving up. Error:", err)
			}

			processedTransactions = true
		}
		if marker == nil {
			break
		}
	}
	if maxLedgerIndex > s.lastProcessedLedgerIndex {
		s.lastProcessedLedgerIndex = maxLedgerIndex
		err := s.PersistLastProcessedLedgerIndex()
		if err != nil {
			return err
		}
	}
	if !processedTransactions {
		time.Sleep(s.sleepTime)
	}
	return nil
}

// errors returned by TryVerifyTransaction are permanent errors and the failing txs will end up being skipped forever
func (s *Sentinel) TryVerifyTransaction(ctx context.Context, tx *XRPLTransaction, stream pb.Amplifier_VerifyClient) error {
	message, err := s.TransactionToMessage(tx)
	if err != nil {
		// there is no way to recover from error in transforming transactions to messages
		return err
	}

	expB := backoff.NewExponentialBackOff()
	expB.MaxElapsedTime = s.verificationTimeout
	b := backoff.WithContext(expB, ctx)

	// TODO: are we supposed to have a timeout or keep trying forever?
	err = backoff.Retry(func() error {
		if err := stream.Send(&pb.VerifyRequest{Message: message}); err != nil {
			return err
		}
		response, err := stream.Recv()
		if err != nil {
			return err
		}
		log.Printf("Received Verify Response: %v", response)

		// TODO: permanent errors?
		// docs mention an InvalidGMPRequest which is permanent error, but it's not part of proto spec
		return errors.New(response.Error.Error)
	}, b)
	return err
}

func (s *Sentinel) PersistLastProcessedLedgerIndex() error {
	return os.WriteFile(s.storagePath, []byte(strconv.Itoa(int(s.lastProcessedLedgerIndex))), STORAGE_FILE_PERMISSIONS)
}

func (s *Sentinel) ReadLastProcessedLedgerIndex() (uint32, error) {
	value, err := os.ReadFile(s.storagePath)
	if err != nil {
		return 0, err
	}
	height, err := strconv.ParseUint(string(value), 10, 32)
	if err != nil {
		return 0, err
	}
	return uint32(height), nil
}

func (s *Sentinel) TransactionToMessage(tx *XRPLTransaction) (*pb.Message, error) {
	// TODO: this whole function needs to be rewritten when we know ITS Hub specifics
	destinationChain, err := tx.GetMemo("destination_chain")
	if err != nil {
		return nil, err
	}
	destinationAddress, err := tx.GetMemo("destination_address")
	if err != nil {
		return nil, err
	}
	payloadHash, err := tx.GetMemo("payload_hash")
	if err != nil {
		return nil, err
	}
	var amount, tokenId string
	switch amt := tx.Amount.(type) {
	case string:
		// XRP
		amount = amt
		tokenId = "uxrp"
	case map[string]interface{}:
		// Token
		amount = amt["value"].(string)
		tokenId = amt["currency"].(string)
	default:
		panic("Unexpected amount type")
	}
	payload, err := ABIEncode([]string{"string", "string", "bytes32"}, []interface{}{amount, tokenId, common.HexToHash(payloadHash)})
	if err != nil {
		return nil, err
	}
	msg := pb.Message{
		Id:                 tx.Hash,
		SourceChain:        CHAIN_NAME,
		SourceAddress:      tx.Account,
		DestinationChain:   destinationChain,
		DestinationAddress: destinationAddress,
		Payload:            payload,
	}
	return &msg, nil
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

type Approver struct {
	lastProcessedHeight uint64
	axelarClient        pb.AmplifierClient
	messageQueue        *amqp.Channel
	storagePath         string
}

func (a *Approver) ProcessEvents(ctx context.Context) error {
	req := &pb.SubscribeToApprovalsRequest{Chains: []string{CHAIN_NAME}, StartHeight: &a.lastProcessedHeight}
	stream, err := a.axelarClient.SubscribeToApprovals(ctx, req)
	if err != nil {
		return err
	}
	for {
		select {
		case <-ctx.Done():
			return nil
		default:
			// continue
		}

		resp, err := stream.Recv()
		if err != nil {
			return err
		}
		err = a.messageQueue.PublishWithContext(ctx, "", "includer", false, false, amqp.Publishing{
			ContentType: "application/octet-stream",
			Body:        resp.ExecuteData,
		})
		if err != nil {
			return err
		}
		if resp.BlockHeight > a.lastProcessedHeight {
			a.lastProcessedHeight = resp.BlockHeight
			err = a.PersistLastProcessedHeight()
			if err != nil {
				return err
			}
		}
	}
}

func (a *Approver) ReadLastProcessedHeight() (uint64, error) {
	value, err := os.ReadFile(a.storagePath)
	if err != nil {
		return 0, err
	}
	return strconv.ParseUint(string(value), 10, 64)
}

func (a *Approver) PersistLastProcessedHeight() error {
	return os.WriteFile(a.storagePath, []byte(strconv.Itoa(int(a.lastProcessedHeight))), STORAGE_FILE_PERMISSIONS)
}

type Broadcaster struct {
	axelarClient pb.AmplifierClient
	messageQueue *amqp.Channel
	maxRetries   uint32
}

func (b *Broadcaster) ConsumeQueueAndBroadcast(ctx context.Context) error {
	deliveries, err := b.messageQueue.ConsumeWithContext(ctx, "broadcast", "broadcaster", false, true, false, false, amqp.Table{})
	if err != nil {
		return err
	}
	for delivery := range deliveries {
		var req pb.BroadcastRequest
		err := proto.Unmarshal(delivery.Body, &req)
		if err != nil {
			return err
		}
		res, err := b.axelarClient.Broadcast(ctx, &req)
		if err == nil && res.Success {
			delivery.Ack(false)
		} else {
			log.Println("Failed to broadcast: ", err, res)
			nackWithLimit(b.messageQueue, delivery, b.maxRetries)
		}
	}
	return nil
}

type Includer struct {
	messageQueue *amqp.Channel
	xrplClient   *XRPLClient
	maxRetries   uint32
}

func (i *Includer) SubmitProofs(ctx context.Context) error {
	deliveries, err := i.messageQueue.ConsumeWithContext(ctx, "inclusion", "includer", false, true, false, false, amqp.Table{})
	if err != nil {
		return err
	}
	for delivery := range deliveries {
		proof := delivery.Body
		res, err := i.xrplClient.Submit(ctx, proof)
		if err != nil && res.EngineResult == "tesSUCCESS" {
			delivery.Ack(false)
		} else {
			log.Println("Failed to submit transaction", proof, err, res.EngineResult, res.EngineResultCode, res.EngineResultMessage)
			nackWithLimit(i.messageQueue, delivery, i.maxRetries) // TODO: requeue how many times?
		}
	}
	return nil
}

type XRPLClient struct {
	RPCUrl string
}

func (c *XRPLClient) Tx(ctx context.Context, hash string) (XRPLTransaction, error) {
	payload := map[string]interface{}{
		"method": "tx",
		"params": []map[string]string{
			{
				"transaction": hash,
			},
		},
	}

	var data struct {
		Result XRPLTransaction `json:"result"`
	}
	err := JsonRPCRequest(ctx, c.RPCUrl, payload, &data)
	if err == nil && data.Result.Status != "success" {
		return data.Result, fmt.Errorf("failed to fetch transaction %s: %s", hash, data.Result.ErrorMessage)
	}
	return data.Result, err
}

type XRPLSubmitResult struct {
	EngineResult string `json:"engine_result"`
	// TODO: documentation specifies it as "signed integer" but not how many bits
	EngineResultCode    int64  `json:"engine_result_code"`
	EngineResultMessage string `json:"engine_result_message"`
	// TXJson struct {} `json:"tx_json"`
}

type XRPLLedgerRequest struct {
	LedgerIndex  string `json:"ledger_index"`
	Transactions bool   `json:"transactions"`
	Expand       bool   `json:"expand"`
	OwnerFunds   bool   `json:"owner_funds"`
}

type XRPLLedger struct {
	LedgerHash string `json:"ledger_hash"`
	// string type not a mistake, this is what the RPC returns...
	LedgerIndex string `json:"ledger_index"`
	ParentHash  string `json:"parent_hash"`
}

func (c *XRPLClient) Ledger(ctx context.Context, params XRPLLedgerRequest) (XRPLLedger, error) {
	payload := map[string]interface{}{
		"method": "ledger",
		"params": []XRPLLedgerRequest{params},
	}

	var data struct {
		Result struct {
			Ledger XRPLLedger `json:"ledger"`
		} `json:"result"`
	}
	err := JsonRPCRequest(ctx, c.RPCUrl, payload, &data)
	return data.Result.Ledger, err
}

func (c *XRPLClient) Submit(ctx context.Context, blob []byte) (XRPLSubmitResult, error) {
	payload := map[string]interface{}{
		"method": "submit",
		"params": []map[string]string{
			{
				"tx_blob": string(blob),
			},
		},
	}

	var data struct {
		Result XRPLSubmitResult `json:"result"`
	}
	err := JsonRPCRequest(ctx, c.RPCUrl, payload, &data)
	return data.Result, err
}

func (c *XRPLClient) AccountTx(ctx context.Context, account string, ledgerIndexMin uint32, ledgerIndexMax uint32, marker *json.RawMessage) ([]XRPLTransactionInfo, *json.RawMessage, error) {
	params := map[string]interface{}{
		"account":          account,
		"ledger_index_min": ledgerIndexMin,
		"ledger_index_max": ledgerIndexMax,
	}
	if marker != nil {
		params["marker"] = marker
	}

	payload := map[string]interface{}{
		"method": "account_tx",
		"params": []map[string]interface{}{params},
	}

	var responseData AccountTxResponsePayload
	err := JsonRPCRequest(ctx, c.RPCUrl, payload, &responseData)
	if err != nil {
		return []XRPLTransactionInfo{}, nil, err
	}

	return responseData.Result.Transactions, responseData.Result.Marker, nil
}

func JsonRPCRequest(ctx context.Context, url string, payload interface{}, responseData interface{}) error {
	jsonData, err := json.Marshal(payload)
	if err != nil {
		panic(err)
	}

	req, err := http.NewRequestWithContext(ctx, "POST", url, bytes.NewBuffer(jsonData))
	if err != nil {
		panic(err)
	}

	req.Header.Set("Content-Type", "application/json")

	httpClient := &http.Client{}
	resp, err := httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		panic(err)
	}

	return json.Unmarshal(body, responseData)
}

type XRPLTransaction struct {
	TransactionType string
	Hash            string `json:"hash"`
	Account         string
	Amount          interface{} `json:"Amount"`
	Destination     string
	Memos           []Memo `json:"Memos,omitempty"`
	Fee             string
	Flags           int64
	Sequence        int64
	Validated       bool `json:"validated"`
	Signers         []XRPLSignerWrapper
	Meta            struct{ TransactionResult string } `json:"meta"`
	Status          string                             `json:"status"`
	ErrorMessage    string                             `json:"error_message,omitempty"`
	LedgerIndex     uint32                             `json:"LedgerIndex"`
}

func (tx *XRPLTransaction) SignerPubKeys() []PublicKey {
	pubKeys := make([]PublicKey, len(tx.Signers))
	for i, signer := range tx.Signers {
		pubKeys[i] = PublicKey{signer.Signer.SigningPubKey}
	}
	return pubKeys
}

func (tx *XRPLTransaction) GetMemo(byType string) (string, error) {
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

func nackWithLimit(channel *amqp.Channel, msg amqp.Delivery, maxRetries uint32) {
	retryCountHeader := msg.Headers["x-retry-count"]
	retryCount, ok := retryCountHeader.(uint32)
	if !ok {
		retryCount = 0
	}

	if retryCount < maxRetries {
		retryCount++
		msg.Headers["x-retry-count"] = retryCount

		err := msg.Nack(false, true)
		if err != nil {
			log.Printf("Failed to nack and requeue message: %s", err)
		} else {
			log.Printf("Message requeued, attempt %d", retryCount)
		}
	} else {
		log.Printf("Max retries exceeded for message, sending to DLX or handling failure")
		err := msg.Nack(false, false)
		if err != nil {
			log.Printf("Failed to nack message: %s", err)
		}
	}
}
