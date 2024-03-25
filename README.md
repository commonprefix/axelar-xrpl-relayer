# Axelar<>XRPL Testnet Instructions

- Supported chains:
    * Ethereum Sepolia
    * XRPL Testnet

- Supported tokens:
    * [`WETH`](https://sepolia.etherscan.io/token/0x7b79995e5f793a07bc00c21412e50ecae098e7f9) on Ethereum Sepolia (18 decimals)
    * `ETH` on XRPL Testnet (18 decimals)
    * `XRP` on XRPL Testnet (6 decimals)
    * [`axlXRP`](https://sepolia.etherscan.io/token/0x40d5ed73982468499ecfa9c8cc0abb63ff13a409) on Ethereum Sepolia (6 decimals)

## Deployment Addresses

- `AxelarGateway` on Ethereum Sepolia: [`0x72F28C8d7b16088d49F27B5D1D51DB66BA966684`](https://sepolia.etherscan.io/address/0x72F28C8d7b16088d49F27B5D1D51DB66BA966684)
- XRPL multisig account on XRPL Testnet: [`rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb`](https://testnet.xrpl.org/accounts/rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb)

## Disclaimers

- This is a **testnet deployment**. Any mainnet funds that are transferred to any of the addresses mentioned in this doc may be irrecoverable.
- **Please do not use any mainnet wallet** to perform any of the actions outlined in this doc.
- Gas payments are not currently supported. You do not need to call the `AxelarGasService` on Ethereum Sepolia to refund the relayer. Our relayer is running 'pro bono.'
- The bridge does not charge any fees/tolls at the moment.
- The `IAxelarGateway` interface and `AxelarExecutable` smart contract in this repo are _different_ to those currently deployed by Axelar. Instructions you may find elsewhere are likely not entirely compatible with this testnet deployment.
- Only one validator is currently used to secure this testnet bridge.
- Only one relayer is currently active.
- Since this is a testnet deployment, there is no guaranteed SLA. Please let us know if your transactions are taking too long to appear on the destination chain.
- The deployment addresses are subject to change.

## Setup

- These instructions mainly rely on Foundry's `cast` command to perform Ethereum Sepolia transactions (although you can use any library of your choice). Foundry can be installed using the following instructions: [book.getfoundry.sh/getting-started/installation](https://book.getfoundry.sh/getting-started/installation).
- You can use some RPC provider like Alchemy or Infura to broadcast transcations to Ethereum Sepolia. These instructions assume that you have set the `SEPOLIA_RPC_URL` environment variable to a working Sepolia RPC URL.
- Generate a new wallet for each side. For Ethereum Sepolia, you can use the `cast wallet new` command. These instructions assume that you have set the `PRIVATE_KEY` environment variable to a funded Sepolia wallet private key.
- Fund your accounts using a faucet, such as these:
    * Ethereum Sepolia `ETH`: [alchemy.com/faucets/ethereum-sepolia](https://www.alchemy.com/faucets/ethereum-sepolia)
    * XRPL Testnet `XRP`: [faucet.tequ.dev](https://faucet.tequ.dev/)
- To bridge to XRPL Testnet, you can use an XRPL library of your preference, such as `xrpl.js` or `xrpl-py`.
- To perform general message passing (GMP) from XRPL, the Ethereum Sepolia smart contract (that you wish to call) needs to implement the [`AxelarExecutable`](https://github.com/commonprefix/axelar-xrpl-solidity/blob/main/src/executable/AxelarExecutable.sol) contract. Please keep in mind that this contract is not the standard `AxelarExecutable` contract that you will find on other Axelar resources.

## Token Bridging

This section outlines how to transfer any of the supported tokens between Ethereum Sepolia and XRPL Testnet.

### Bridge Ethereum Sepolia `ETH` to XRPL Testnet

1. Create a Trust Line on XRPL Testnet between the recipient user and the XRPL multisig, using a `TrustSet` transaction, such as the following:
```javascript
{
    "TransactionType": "TrustSet",
    "Account": user.address, // the XRPL address of the ETH transfer recipient
    "LimitAmount": {
        "currency": "ETH",
        "issuer": "rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb",
        "value": "10000000000",
    },
    ...
},
```
2. Wrap `ETH` into `WETH` on Ethereum Sepolia:
    - Using `cast`:
    ```sh
    WETH=0x7b79995e5f793a07bc00c21412e50ecae098e7f9
    cast send $WETH "deposit()" --value 0.1ether --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
    ```
    - Alternatively, [via Etherescan](https://sepolia.etherscan.io/token/0x7b79995e5f793a07bc00c21412e50ecae098e7f9#writeContract), select "Connect to Web3" to connect your wallet, expand the `deposit` dropdown, type the amount of `ETH` that you would like to bridge (e.g., `0.1`) in the input, and click "Write" to perform the transcation.
3. Approve the `AxelarGateway` contract to spend your `WETH` on Ethereum Sepolia:
    - Using `cast`:
    ```sh
    WETH=0x7b79995e5f793a07bc00c21412e50ecae098e7f9
    AXELAR_GATEWAY=0x72F28C8d7b16088d49F27B5D1D51DB66BA966684
    cast send $WETH "approve(address guy, uint256 amount)" $AXELAR_GATEWAY $(cast to-wei 0.1) --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
    ```
    - Alternatively, [via Etherescan](https://sepolia.etherscan.io/token/0x7b79995e5f793a07bc00c21412e50ecae098e7f9#writeContract), select "Connect to Web3" to connect your wallet, expand the `approve` dropdown, type the `AxelarGateway`'s address (`0x832B2dfe8a225550C144626111660f61B8690efD`) in the `guy` input, and the amount of `ETH` that you would like to bridge (e.g., `0.1`) in the input under `deposit`, and click "Write" to perform the transcation.
4. On Ethereum Sepolia, call `AxelarGateway.sendToken()` to bridge your `WETH`:
    - Using `cast`:
    ```sh
    AXELAR_GATEWAY=0x72F28C8d7b16088d49F27B5D1D51DB66BA966684
    XRPL_DESTINATION= # your XRPL recipient address
    cast send $AXELAR_GATEWAY "sendToken(string destinationChain, string destinationAddress, string symbol, uint256 amount)" "xrpl" $XRPL_DESTINATION "WETH" $(cast to-wei 0.1) --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
    ```
5. Within a few minutes, your designated recipient address should receive the bridged `ETH` on XRPL.
    - Check that your recipient account received the ETH tokens via the [XRPL Testnet explorer](https://testnet.xrpl.org/)

### Bridge wrapped `ETH` back to Ethereum Sepolia

1. Deposit `ETH` into the XRPL multisig account (`rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb`) using a `Payment` transaction of the following format:
```javascript
{
    TransactionType: "Payment",
    Account: user.address,
    Amount: {
        currency: "ETH",
        value: "0.001", // = 10^15 wei - the amount of ETH you want to bridge, in ETH
        issuer: "rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb",
    },
    Destination: "rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb", 
    Memos: [
        {
            Memo: {
                MemoData: "605459C28E6bE7B31B8b622FD29C82B3059dB1C6", // your ETH recipient address, without the 0x prefix
                MemoType: "64657374696E6174696F6E5F61646472657373", // hex("destination_address")
            },
        },
        {
            Memo: {
                MemoData: "657468657265756D", // hex("ethereum")
                MemoType: "64657374696E6174696F6E5F636861696E", // hex("destination_address")
            },
        },
        {
            Memo: {
                MemoData: "0000000000000000000000000000000000000000000000000000000000000000", // bytes32(0) indicates pure token transfer, without GMP
                MemoType: "7061796C6F61645F68617368", // hex("payload_hash")
            },
        },
    ],
    ...
}
```
2. Within a few minutes, your designated recipient address should receive the bridged `ETH` on Ethereum Sepolia, as `WETH`:
    - Check that your recipient account received the `WETH` tokens via an [Ethereum Sepolia explorer](https://sepolia.etherscan.io/).
    - Please keep in mind that the explorer might take a few minutes to index the transaction.

### Bridge `XRP` to Ethereum Sepolia

1. Deposit `XRP` into the XRPL multisig account (`rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb`) using a `Payment` transaction of the following format:
```javascript
{
    TransactionType: "Payment",
    Account: user.address,
    Amount: "1000000", // = 1 XRP - the amount of XRP you want to bridge, in drops
    Destination: "rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb", 
    Memos: [
        {
            Memo: {
                MemoData: "605459C28E6bE7B31B8b622FD29C82B3059dB1C6", // your ETH recipient address, without the 0x prefix
                MemoType: "64657374696E6174696F6E5F61646472657373", // hex("destination_address")
            },
        },
        {
            Memo: {
                MemoData: "657468657265756D", // hex("ethereum")
                MemoType: "64657374696E6174696F6E5F636861696E", // hex("destination_address")
            },
        },
        {
            Memo: {
                MemoData: "0000000000000000000000000000000000000000000000000000000000000000", // bytes32(0) indicates pure token transfer, without GMP
                MemoType: "7061796C6F61645F68617368", // hex("payload_hash")
            },
        },
    ],
    ...
}
```
2. Within a few minutes, your designated recipient address should receive the bridged `XRP` on Ethereum Sepolia, as `axlXRP`:
    - Check that your recipient account received the `XRP` tokens via an [Ethereum Sepolia explorer](https://sepolia.etherscan.io/).
    - Please keep in mind that the explorer might take a few minutes to index the transaction.

### Bridge wrapped `XRP` (`axlXRP`) back to XRPL Testnet

1. Approve the `AxelarGateway` contract to spend your `axlXRP` on Ethereum Sepolia:
    - Using Foundry's `cast` command:
    ```sh
    AXL_XRP=0x40d5ed73982468499ecfa9c8cc0abb63ff13a409
    AXELAR_GATEWAY=0x72F28C8d7b16088d49F27B5D1D51DB66BA966684
    cast send $AXL_XRP "approve(address guy, uint256 amount)" $AXELAR_GATEWAY 1000000 --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
    ```
    - Alternatively, [via Etherescan](https://sepolia.etherscan.io/token/0x40d5ed73982468499ecfa9c8cc0abb63ff13a409#writeContract), select "Connect to Web3" to connect your wallet, expand the `approve` dropdown, type the `AxelarGateway`'s address (`0x832B2dfe8a225550C144626111660f61B8690efD`) in the `guy` input and the amount of `ETH` that you would like to bridge (e.g., `0.1`) in the input under `deposit`, and click "Write" to perform the transcation.
2. On Ethereum Sepolia, call `AxelarGateway.sendToken()` to bridge your `axlXRP`:
    - Using Foundry's `cast` command:
    ```sh
    AXELAR_GATEWAY=0x72F28C8d7b16088d49F27B5D1D51DB66BA966684
    XRPL_DESTINATION= # your XRPL recipient address
    cast send $AXELAR_GATEWAY "sendToken(string destinationChain, string destinationAddress, string symbol, uint256 amount)" "xrpl" $XRPL_DESTINATION "axlXRP" 1000000 --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
    ```
3. Within a few minutes, your designated recipient address should receive the bridged `XRP` on XRPL Testnet:
    - Check that your recipient account received the ETH tokens via the [XRPL Testnet explorer](https://testnet.xrpl.org/)

## General Message Passing

This section outlines how to call a function of a smart contract on Ethereum Sepolia from XRPL.

1. Compute the payload (let's call it `gmpPayload`) that you would like to call your Ethereum Sepolia `AxelarExecutable` smart contract's `_execute` function with.
2. Create a `Payment` transaction in the same format as the ones given above, with the following changes:
    * Set the transaction `Amount` depending on what token you would like to bridge to the destination smart contract during GMP. If you just want to GMP, without any token transfer, set the `Amount` to `1` drop.
    * Set the destination address's `MemoData` field to the address of your Ethereum Sepolia `AxelarExecutable` smart contract.
    * Set the payload hash's `MemoData` field to `keccak256(abi.encode(gmpPayload))`. You can use the [`eth-abi`](https://eth-abi.readthedocs.io/en/stable/encoding.html) and [`eth-utils`](https://eth-utils.readthedocs.io/en/stable/utilities.html#keccak-bytes-int-bool-text-str-hexstr-str-bytes) python libraries to compute this hash, e.g.:
    ```py
    from eth_abi import encode
    from eth_utils import keccak
    keccak(encode(['string'], ['hello, world!'])).hex()
    ```
3. Within a few minutes, the relayer will have submitted validator signatures of the XRPL Testnet deposit transaction to the Ethereum Sepolia `AxelarGateway` contract, which records the approval of the payload hash and emits a `ContractCallApproved` event. You can verify that this event was called using an [Ethereum Sepolia explorer](https://sepolia.etherscan.io/address/0x72F28C8d7b16088d49F27B5D1D51DB66BA966684).
4. Call the `execute` function on your `AxelarExecutable` Ethereum Sepolia smart contract:
```sh
AXELAR_EXECUTABLE= # your `AxelarExecutable` contract
COMMAND_ID= # the `commandId` that was emitted in the `ContractCallApproved` event
SOURCE_ADDRESS= # the XRPL address that performed the `Payment` deposit transaction
PAYLOAD= # abi.encode(['string', 'uint256', 'bytes'], [symbol, amount, gmpPayload])
cast send $AXELAR_EXECUTABLE 'function execute(bytes32 commandId, string calldata sourceChain, string calldata sourceAddress, bytes calldata payload)' $COMMAND_ID xrpl $SOURCE_ADDRESS $PAYLOAD --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
```

### Example GMP Call

We have created and [deployed to Ethereum Sepolia](https://sepolia.etherscan.io/address/0x189C2572063f25FEf5Cdd3516DDDd9fA6e9CB187) an example `AxelarExecutable` contract called [`ExecutableSample`](https://github.com/commonprefix/axelar-xrpl-solidity/blob/main/src/executable/examples/ExecutableSample.sol).

Let's call the `ExecutableSample` contract from XRPL, to update its `message` state variable:
1. Let's aim to set the `ExecutableSample.message` state variable to `'Just transferred XRP to Ethereum!'`.
2. Initiate the GMP call by making a `Payment` to the XRPL multisig:
```javascript
const XRPL_RPC_URL = "wss://s.altnet.rippletest.net:51233";
async function gmp(user: xrpl.Wallet) {
    const client = new xrpl.Client(XRPL_RPC_URL);
    await client.connect();

    // const user = xrpl.Wallet.fromSeed(SEED); // Read XRPL wallet seed from environment or generate and fund new wallet:
    const user = xrpl.Wallet.generate();
    await client.fundWallet(user);

    const gmpTx: xrpl.Transaction = {
        TransactionType: "Payment",
        Account: user.address,
        Amount: xrpl.xrpToDrops(1),
        Destination: "rfEf91bLxrTVC76vw1W3Ur8Jk4Lwujskmb",
        SigningPubKey: "",
        Flags: 0,
        Fee: "30",
        Memos: [
            {
                Memo: {
                    MemoData: "189C2572063f25FEf5Cdd3516DDDd9fA6e9CB187", // the `ExecutableSample` contract
                    MemoType: "64657374696E6174696F6E5F61646472657373", // hex("destination_address")
                },
            },
            {
                Memo: {
                    MemoData: "657468657265756D", // hex("ethereum")
                    MemoType: "64657374696E6174696F6E5F636861696E", // hex("destination_address")
                },
            },
            {
                Memo: {
                    MemoData: "df031b281246235d0e8c8254cd731ed95d2caf4db4da67f41a71567664a1fae8", // keccak256(abi.encode(gmpPayload))
                    MemoType: "7061796C6F61645F68617368", // hex("payload_hash")
                },
            },
        ],
    };

    const signed = user.sign(await client.autofill(paymentTx));
    await client.submitAndWait(signed.tx_blob);
    await client.disconnect();
}

gmp();
```
3. Wait for the relayer to call `AxelarGateway.execute()`. Verify that the `ContractCallApproved` event was called using an [Ethereum Sepolia explorer](https://sepolia.etherscan.io/address/0x72F28C8d7b16088d49F27B5D1D51DB66BA966684).
4. Call the `ExecutableSample.execute()`:
```sh
AXELAR_EXECUTABLE=0x189C2572063f25FEf5Cdd3516DDDd9fA6e9CB187
COMMAND_ID= # the `commandId` that was emitted in the `ContractCallApproved` event
SOURCE_ADDRESS= # the XRPL address of the `user` who performed the `Payment` deposit transaction
PAYLOAD=000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000f424000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000661786c58525000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000214a757374207472616e736665727265642058525020746f20457468657265756d2100000000000000000000000000000000000000000000000000000000000000 # encode(['string', 'uint256', 'bytes'], [symbol, amount, encode(['string'], ['Just transferred XRP to Ethereum!'])])
cast send $AXELAR_EXECUTABLE 'function execute(bytes32 commandId, string calldata sourceChain, string calldata sourceAddress, bytes calldata payload)' $COMMAND_ID xrpl $SOURCE_ADDRESS $PAYLOAD --private-key $PRIVATE_KEY --rpc-url $SEPOLIA_RPC_URL
```
5. `AxelarExecutable.message` should now be set to `'Just transferred XRP to Ethereum!'`:
```sh
AXELAR_EXECUTABLE=0x189C2572063f25FEf5Cdd3516DDDd9fA6e9CB187
cast call $AXELAR_EXECUTABLE 'message()(string)' --rpc-url $SEPOLIA_RPC_URL
# Just transferred XRP to Ethereum!
```
