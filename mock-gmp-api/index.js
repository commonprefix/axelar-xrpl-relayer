const express = require('express');
const { v4: uuidv4 } = require('uuid'); // For generating UUIDs
const bodyParser = require('body-parser');
const app = express();
const port = 3000;

app.use(bodyParser.json());

// Pre-define tasks
const construct_proof_task = {
    id: uuidv4(),
    chain: "chainA",
    timestamp: new Date().toISOString(),
    type: "CONSTRUCT_PROOF",
    task: {
        message: {
            messageID: "msg-123",
            sourceChain: "chainA",
            sourceAddress: "0xSource123",
            destinationAddress: "0xDest456",
            payloadHash: "YWJjZDEyMzQ=", // base64-encoded payloadHash
        },
        payload: "ZXhhbXBsZV9wYXlsb2Fk" // base64-encoded payload
    }
};

const verify_task = {
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
        message: {
            messageID: "msg-789",
            sourceChain: "chainC",
            sourceAddress: "0xAnotherSource",
            destinationAddress: "0xAnotherDest",
            payloadHash: "ZWZnaDU2Nzg=" // base64-encoded payloadHash
        },
        payload: "bW9yZV9leGFtcGxlX2RhdGE=" // base64-encoded payload
    }
};

// State flags
let event_received = false;            // True after an event is posted
let verify_returned = false;           // True after we have returned the VERIFY task once
let proof_broadcast_received = false;  // True after a verify_messages broadcast is posted
let proof_returned = false;            // True after we have returned the CONSTRUCT_PROOF task once

// Initially returns no tasks. After event posted, returns VERIFY once.
// After verify_messages broadcast, returns CONSTRUCT_PROOF once.
app.get('/chains/xrpl/tasks', (req, res) => {
    // If we haven't received any event yet, no tasks:
    if (!event_received) {
        return res.json({ tasks: [] });
    }

    // If event is received but VERIFY not returned yet, return VERIFY task once
    if (event_received && !verify_returned) {
        verify_returned = true;
        return res.json({ tasks: [verify_task] });
    }

    // If we've received the verify_messages broadcast and not returned the CONSTRUCT_PROOF task yet:
    if (proof_broadcast_received && !proof_returned) {
        proof_returned = true;
        return res.json({ tasks: [construct_proof_task] });
    }

    // Otherwise, no tasks
    res.json({ tasks: [] });
});

// Posting an event here starts the chain reaction: next GET /tasks returns VERIFY once.
app.post('/chains/xrpl/events', (req, res) => {
    console.log("Received event: ");
    console.log(JSON.stringify(req.body, null, 2));
    event_received = true;
    let response = {
        results: [{
            status: "ACCEPTED",
            index: 0
        }, {
            status: "ACCEPTED",
            index: 1
        }
        ]
    }
    res.json(response);
});

// After we post a broadcast of type verify_messages, we trigger the return of CONSTRUCT_PROOF.
app.post('/contracts/contract/broadcasts', (req, res) => {
    console.log("Received broadcast: ");
    console.log(JSON.stringify(req.body, null, 2));

    // Check if this broadcast is the "verify_messages" type
    // Adjust this condition based on the actual structure of your broadcast message
    if (req.body && req.body.type === "verify_messages") {
        proof_broadcast_received = true;
    }

    res.status(200).send('Broadcast received');
});

// /contracts/contract/queries
// Returns a fixed message object for queries:
app.post('/contracts/contract/queries', (req, res) => {
    console.log("Received query: ");
    console.log(JSON.stringify(req.body, null, 2));
    res.json({
        messageID: "msg-123",
        sourceChain: "chainA",
        sourceAddress: "0xSource123",
        destinationAddress: "0xDest456",
        payloadHash: "YWJjZDEyMzQ="
    });
});

app.listen(port, () => {
    console.log(`Server is running on http://localhost:${port}`);
});
