const express = require('express');
const { v4: uuidv4 } = require('uuid'); // For generating UUIDs
const bodyParser = require('body-parser');
const app = express();
const port = 3001;
const { exec } = require('child_process');

app.use(bodyParser.json());
const AXELAR_SENDER = "mykey";
const CHAIN_ID = "devnet-its";
const GAS_PRICES = "0.00005uits";
const RPC_URL = "http://k8s-devnetit-coresent-3ea294cee9-0949c478b885da8a.elb.us-east-2.amazonaws.com:26657";

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
                tx_id: "1757F2CBE522A273A5B9FEEBDC8C49F4B4FDAA5D8FE88E7204DBAC455A6A5498",
                source_address: "rUicSJrWuWo8Prpx98fAmfBx2J9MLB4p2K",
                destination_chain: "avalanche",
                destination_address: Buffer.from("avax10f8305248c0wsfsdempdtpx7lpkc30vwzl9y9q", 'utf8').toString('hex'),
                payload_hash: "0000000000000000000000000000000000000000000000000000000000000000",
                amount: { drops: 1234560 }
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
let verify_returned = false;           // True after we have returned the VERIFY task once
let proof_broadcast_received = false;  // True after a verify_messages broadcast is posted
let proof_returned = false;            // True after we have returned the CONSTRUCT_PROOF task once

// Initially returns no tasks. After event posted, returns VERIFY once.
// After verify_messages broadcast, returns CONSTRUCT_PROOF once.
app.get('/chains/xrpl/tasks', (req, res) => {
    // If event is received but VERIFY not returned yet, return VERIFY task once
    if (!verify_returned) {
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
app.post('/contracts/:contract/broadcasts', (req, res) => {
    const contract = req.params.contract;

    console.log(`Received broadcast for contract ${contract}:`);
    console.log(JSON.stringify(req.body, null, 2));

    const fromKey = "axelar1fxa3h6amruu77qvlkzezhm2qjvx37wq0z7mz56";
    const chainId = "devnet-amplifier";
    const gasPrices = "0.007uamplifier";
    const command = `axelard tx wasm execute ${contract} '${JSON.stringify(req.body)}' \
        --from ${AXELAR_SENDER} \
        --chain-id ${CHAIN_ID} \
        --gas auto \
        --gas-adjustment 1.4 \
        --gas-prices ${GAS_PRICES} \
        --output json \
        --keyring-backend test \
        --node ${RPC_URL} \
        -y`;

    console.log(`Command: ${command}`);
    exec(command, (error, stdout, stderr) => {
        console.log(`Command: ${command}`);
        if (error) {
            console.error(`\tError: ${error.message}`);
            res.status(500).send('Error executing query' + error.message);
            return;
        }

        // if (stderr) {
        //     console.error(`\tstderr: ${stderr}`);
        //     res.status(500).send('Error executing query' + stderr);
        //     return;
        // }

        console.log(`\tResponse: ${stdout}`);
        res.json(JSON.parse(stdout));
    });
});

const XRPL_GATEWAY_ADDRESS = 'axelar1qhqra0tjsgv9wy5zz68g7x7wteqzg7f2ne822kc4gf6dkxzsa5zsu7mqjq';
app.post('/contracts/:contract/queries', (req, res) => {
    const contract = req.params.contract;

    return res.json({ foo: 'bar' });
    console.log(`Received query for contract ${contract}:`);
    console.log(JSON.stringify(req.body, null, 2));

    const query = JSON.stringify(req.body);
    const command = `axelard q wasm contract-state smart ${XRPL_GATEWAY_ADDRESS} '${query}'`;
    console.log("Executing axelard command: " + command);
    exec(command, (error, stdout, stderr) => {
        console.log(`Command: ${command}`);
        if (error) {
            console.error(`\tError: ${error.message}`);
            res.status(500).send('Error executing query' + error.message);
            return;
        }

        if (stderr) {
            console.error(`\tstderr: ${stderr}`);
            res.status(500).send('Error executing query' + stderr);
            return;
        }

        console.log(`\tResponse: ${stdout}`);
        res.json(JSON.parse(stdout));
    });

    // res.json({
    //     messageID: "msg-123",
    //     sourceChain: "chainA",
    //     sourceAddress: "0xSource123",
    //     destinationAddress: "0xDest456",
    //     payloadHash: "YWJjZDEyMzQ="
    // });
});

app.listen(port, () => {
    console.log(`Server is running on http://localhost:${port}`);
});
