# Mock GMP API

## Prerequisites

- [Node.js](https://nodejs.org/)
- [Redis](https://redis.io/) server

## Setup & Run

1. **Install dependencies:**
   
```bash
npm install
```

2. **Update `.env`:**

```bash
# Redis server URL
REDIS_SERVER=redis://127.0.0.1/

# Wallet to use as 'from' address for Axelar
GMP_API_WALLET=my-axelar-wallet

# Addresses for XRPL-related contracts
XRPL_VOTING_VERIFIER_ADDRESS=some-verifier-address
XRPL_GATEWAY_ADDRESS=some-gateway-address
MULTISIG_CONTRACT_ADDRESS=some-multisig-address

# Axelar chain configuration
AXELAR_CHAIN_ID="devnet-its"
AXELAR_GAS_PRICES="0.00005uits"
AXELAR_RPC_URL=http://localhost:1234
```

3. **Start the server**:

```bash
npm start
```

By default, the server listens on port `3001`. If everything is set up correctly, you should see:

```
Server is running on http://localhost:3001
```


## API Endpoints

### GET `/chains/xrpl/tasks`

**Description**  
Returns a list of currently known tasks. Optionally, you can provide a query parameter `after` to get tasks with IDs greater than a specified number.

**Request**  
```
GET /chains/xrpl/tasks
GET /chains/xrpl/tasks?after=<taskID>
```

**Response**  
```json
{
  "tasks": [
    {
      "id": 1,
      "chain": "xrpl",
      "timestamp": "2023-01-01T00:00:00.000Z",
      "type": "VERIFY",
      "meta": {},
      "task": {}
    }
    // ...
  ]
}
```

---

### POST `/chains/xrpl/events`

**Description**  
Simulates the receipt of GMP events from the Relayer.

**Request**  
```json
{
  "events": [
    {
      "type": "CALL",
      "message": {
        "cc_id": {
          "message_id": "some_id",
          "source_chain": "xrpl"
        },
        "source_address": "source_addr",
        "destination_address": "dest_addr",
        "payload_hash": "payload_hash"
      },
      "payload": "some_payload"
    }
  ]
}
```

**Response**  
```json
{
  "results": [
    {
      "status": "ACCEPTED",
      "index": 0
    }
  ]
}
```

---

### POST `/contracts/:contract/broadcasts`

**Description**  
Broadcasts a transaction to a specified contract on Axelar using `axelard`.

**Request**  
```
{
  "some_message": {}
}
```

**Response**  
- **200 OK**: If the broadcast message is recognized, it returns the `stdout` from `axelard`.  
- **404 Not Found**: If the broadcast message is unknown.  
- **500 Internal Server Error**: If the broadcast command fails.

---

### POST `/contracts/:contract/queries`

**Description**  
Queries a contract on Axelar. The server calls `axelard q wasm contract-state smart ...` to fetch data and returns the parsed JSON response.

**Request**  
```
{
  "some_query": "some_value"
}
```

**Response**  
- **200 OK**: Returns the parsed result from the command.  
- **400 Bad Request**: If the request body is invalid JSON.  
- **500 Internal Server Error**: If the query fails or the output cannot be parsed.

---
