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

const its_message = {
    messageID: "msg-789",
    sourceChain: "chainC",
    sourceAddress: "0xAnotherSource",
    destinationAddress: "0xAnotherDest",
    payloadHash: "ZWZnaDU2Nzg=" // base64-encoded payloadHash
};

// Initially returns no tasks. After event posted, returns VERIFY once.
// After verify_messages broadcast, returns CONSTRUCT_PROOF once.
let tasks = [];
app.get('/chains/xrpl/tasks', (req, res) => {
    const afterParam = req.query.after;

    const after = afterParam ? Number(afterParam) : null;

    const filteredTasks = after !== null && !isNaN(after)
        ? tasks.filter(task => task.id > after)
        : tasks;

    res.json({ tasks: filteredTasks });
});

let task_autoincrement = 0;

// Posting an event here starts the chain reaction: next GET /tasks returns VERIFY once.
app.post('/chains/xrpl/events', (req, res) => {
    console.log("Received event: ");
    console.log(JSON.stringify(req.body, null, 2));

    for (let event of req.body.events) {
        if (event.type === "CALL") {
            // TODO: emit verify task
            tasks.push({
                id: task_autoincrement++,
                chain: "xrpl",
                timestamp: new Date().toISOString(),
                type: "VERIFY",
                meta: event.meta,
                task: {
                    message: its_message,
                    payload: "bW9yZV9leGFtcGxlX2RhdGE=" // base64-encoded payload
                }
            });
        }
    }
    let response = { results: req.body.events.map((_, index) => ({ status: "ACCEPTED", index })) }
    console.log("sending response", response)
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

    if (req.body.verify_messages) {
        for (let message of req.body.verify_messages) {
            tasks.push({
                id: task_autoincrement++,
                chain: "xrpl",
                timestamp: new Date().toISOString(),
                type: "REACT_TO_WASM_EVENT",
                meta: null,
                task: {
                    event_name: "wasm-quorum-reached",
                    message
                }
            })
        }
    } else if (req.body.route_incoming_messages) {
        for (let message of req.body.route_incoming_messages) {
            tasks.push({
                id: task_autoincrement++,
                chain: "xrpl",
                timestamp: new Date().toISOString(),
                type: "CONSTRUCT_PROOF",
                meta: null,
                task: {
                    message: its_message,
                    payload: "bW9yZV9leGFtcGxlX2RhdGE="
                }
            })
        }
    } else if (req.body.construct_proof) {
        let { cc_id, payload } = req.body.construct_proof;

        tasks.push({
            id: task_autoincrement++,
            chain: "xrpl",
            timestamp: new Date().toISOString(),
            type: "GATEWAY_TX",
            meta: null,
            task: {
                executeData: "execute this tx"
            }
        })
    }
    return res.json({ status: "OK" });
    // console.log(`Command: ${command}`);
    // exec(command, (error, stdout, stderr) => {
    //     console.log(`Command: ${command}`);
    //     if (error) {
    //         console.error(`\tError: ${error.message}`);
    //         res.status(500).send('Error executing broadcast' + error.message);
    //         return;
    //     }

    //     console.log(`\tResponse: ${stdout}`);
    //     res.status(500).send('Dont do anything');
    //     // res.json(JSON.parse(stdout));
    // });
});

const XRPL_GATEWAY_ADDRESS = 'axelar1qhqra0tjsgv9wy5zz68g7x7wteqzg7f2ne822kc4gf6dkxzsa5zsu7mqjq';
app.post('/contracts/:contract/queries', (req, res) => {
    const contract = req.params.contract;

    console.log(`Received query for contract ${contract}:`);
    console.log(JSON.stringify(req.body, null, 2));

    return res.json(its_message);
    const query = JSON.stringify(req.body);
    const command = `axelard q wasm contract-state smart ${contract} '${query}'`;
    console.log("Executing axelard command: " + command);
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
