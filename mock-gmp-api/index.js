const express = require('express');
const { v4: uuidv4 } = require('uuid'); // For generating UUIDs
const bodyParser = require('body-parser')
const app = express();
const port = 3000;

app.use(bodyParser.json());

// Example tasks conforming to the specified schema
const tasks = [
    {
        // A TaskItem with a CONSTRUCT_PROOF task
        id: uuidv4(),
        chain: "chainA",
        timestamp: new Date().toISOString(),
        type: "CONSTRUCT_PROOF",
        task: {
            // ConstructProofTask
            message: {
                messageID: "msg-123",
                sourceChain: "chainA",
                sourceAddress: "0xSource123",
                destinationAddress: "0xDest456",
                payloadHash: "YWJjZDEyMzQ=", // base64-encoded payloadHash
            },
            payload: "ZXhhbXBsZV9wYXlsb2Fk" // base64-encoded payload
        }
    },
    {
        // A TaskItem with a GATEWAY_TX task
        id: uuidv4(),
        chain: "chainB",
        timestamp: new Date().toISOString(),
        type: "GATEWAY_TX",
        task: {
            // GatewayTransactionTask
            executeData: "ZW5jb2RlZEV4ZWN1dGVEYXRh" // base64-encoded data
        }
    },
    {
        // A TaskItem with a VERIFY task
        id: uuidv4(),
        chain: "chainC",
        timestamp: new Date().toISOString(),
        type: "VERIFY",
        meta: {
            sourceContext: {
                user_message: {
                    tx_id: "tx_id_123",
                    source_address: "source_address_123",
                    destination_chain: "destination_chain_123",
                    destination_address: "destination_address_123",
                    payload_hash: "payload_hash_123",
                    amount: "100"
                }
            }
        },
        task: {
            // VerifyTask
            message: {
                messageID: "msg-789",
                sourceChain: "chainC",
                sourceAddress: "0xAnotherSource",
                destinationAddress: "0xAnotherDest",
                payloadHash: "ZWZnaDU2Nzg=" // base64-encoded payloadHash
            },
            payload: "bW9yZV9leGFtcGxlX2RhdGE=" // base64-encoded payload
        }
    }
];

app.get('/chains/xrpl/tasks', (req, res) => {
    res.json({ tasks });
});

app.post('/contracts/contract/broadcasts', (req, res) => {
    console.log(req.body);
    res.status(404).send('Worked');
});

app.listen(port, () => {
    console.log(`Server is running on http://localhost:${port}`);
});
