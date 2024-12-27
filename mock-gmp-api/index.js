require('dotenv').config({path: __dirname + '/../.env'});
const express = require('express');
const bodyParser = require('body-parser');
const app = express();
const port = 3001;
const util = require('util');
const axios = require('axios');
const exec = util.promisify(require('child_process').exec);
const { spawn } = require('child_process');

app.use(bodyParser.text());
const AXELAR_SENDER = "governance";
const START_HEIGHT = 0;

function spawnAsync(command, args = [], options = {}) {
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, options);

        let stdout = '';
        let stderr = '';

        // Collect data from stdout
        child.stdout.on('data', (data) => {
            stdout += data;
        });

        // Collect data from stderr
        child.stderr.on('data', (data) => {
            stderr += data;
        });

        // Handle error events
        child.on('error', (err) => {
            reject(err);
        });

        // Close event is emitted when the process has finished
        child.on('close', (code) => {
            // Resolve if exit code is 0, otherwise reject
            if (code === 0) {
                resolve({ stdout, stderr });
            } else {
                const err = new Error(`Child process exited with code ${code}`);
                err.code = code;
                err.stdout = stdout;
                err.stderr = stderr;
                reject(err);
            }
        });
    });
}

function delay(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function get_current_axelar_height() {
    const command = 'axelard';
    const args = [
        'status',
        '--node', process.env.AXELAR_RPC_URL,
    ];

    let result = JSON.parse((await spawnAsync(command, args)).stderr); // stderr contains the JSON output
    return parseInt(result.SyncInfo.latest_block_height);
}

// Initially returns no tasks. After event posted, returns VERIFY once.
// After verify_messages broadcast, returns CONSTRUCT_PROOF once.
async function fetch_events(event_type, contract, from_height) {
    const command = `axelard q txs \
    --events 'tx.height>${from_height} AND wasm-${event_type}._contract_address=${contract}' \
    --node ${process.env.AXELAR_RPC_URL} \
    --limit 100 \
    --output json`;

    try {
        let result = JSON.parse((await exec(command)).stdout);
        if (result.total_count > 100) {
            console.warn(`More than 100 events found for ${event_type}!`);
        }
        const events = [];
        if (result && result.txs && Array.isArray(result.txs)) {
            for (let tx of result.txs) {
                for (let log of tx.logs) {
                    let event = log.events.find(event => event.type === `wasm-${event_type}`);
                    if (event) {
                        events.push({ event, height: tx.height });
                    }
                }
            }
        }
        return events;
    } catch (error) {
        console.error(`Error executing command: ${error.message}`);
        return [];
    }
}
let cc_id_to_message = {};
(async () => {
    let latest_height = START_HEIGHT || await get_current_axelar_height();
    while (true) {
        let events = (await Promise.all([
            fetch_events('quorum_reached', process.env.XRPL_VOTING_VERIFIER_ADDRESS, latest_height),
            fetch_events('routing_outgoing', process.env.XRPL_GATEWAY_ADDRESS, latest_height),
            fetch_events('signing_completed', process.env.MULTISIG_CONTRACT_ADDRESS, latest_height)
        ])).flat();
        for (let { event, height } of events) {
            height = Number(height);

            let task;
            if (event.type === "wasm-quorum_reached") {
                task = {
                    id: height,
                    chain: "xrpl",
                    timestamp: new Date().toISOString(),
                    type: "REACT_TO_WASM_EVENT",
                    meta: event.meta, // TODO: how to get this
                    task: {
                        message: JSON.parse(event.attributes.find(attr => attr.key === "content").value),
                        event_name: "wasm-quorum-reached"
                    }
                };
            } else if (event.type === "wasm-routing_outgoing") {
                let message_id = event.attributes.find(attr => attr.key === "message_id").value;
                let source_chain = event.attributes.find(attr => attr.key === "source_chain").value;
                let destination_chain = event.attributes.find(attr => attr.key === "destination_chain").value;
                let payload_hash = event.attributes.find(attr => attr.key === "payload_hash").value;
                let cc_id = `${source_chain}_${message_id}`;
                if (destination_chain !== 'xrpl') {
                    continue;
                }

                let payload;
                try {
                    payload = (await axios.get(`http://localhost:5001?hash=${payload_hash}`)).data;
                } catch (error) {
                    console.error(`Could not find payload for ${cc_id}`);
                    console.error(error.message);
                    continue;
                }

                task = {
                    id: height,
                    chain: "xrpl",
                    timestamp: new Date().toISOString(),
                    type: "CONSTRUCT_PROOF",
                    meta: null,
                    task: {
                        message: {
                            messageID: message_id,
                            sourceChain: source_chain,
                            sourceAddress: event.attributes.find(attr => attr.key === "source_address").value,
                            destinationAddress: event.attributes.find(attr => attr.key === "destination_address").value,
                            payloadHash: payload_hash
                        },
                        payload
                    }
                };
            } else if (event.type === "wasm-signing_completed") {
                let chain = event.attributes.find(attr => attr.key === "chain").value;
                let session_id = event.attributes.find(attr => attr.key === "session_id").value;
                if (chain !== 'xrpl') {
                    continue;
                }
                let query = {
                    proof: {
                        multisig_session_id: session_id
                    }
                };
                const command = `axelard q wasm contract-state smart ${process.env.XRPL_MULTISIG_PROVER_ADDRESS} '${JSON.stringify(query)}' --node ${process.env.AXELAR_RPC_URL} --output json`;
                let execute_res = JSON.parse((await exec(command)).stdout);
                task = {
                    id: height,
                    chain: "xrpl",
                    timestamp: new Date().toISOString(),
                    type: "GATEWAY_TX",
                    meta: null,
                    task: {
                        executeData: execute_res.data.tx_blob
                    }
                };
            }

            if (task) {
                console.log(`Creating task: ${JSON.stringify(task, null, 2)}`);
                tasks.push(task);
            }
        }
        latest_height = Math.max(latest_height, ...events.map(event => event.height));

        await delay(2000)
    }
})();

let tasks = [];
app.get('/chains/xrpl/tasks', (req, res) => {
    // TODO: listen for message routed event on gateway. When it comes, emit a construct proof task
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
    const body_json = JSON.parse(req.body);
    console.log(JSON.stringify(body_json, null, 2));

    for (let event of body_json.events) {
        if (event.type === "CALL") {
            tasks.push({
                id: task_autoincrement++,
                chain: "xrpl",
                timestamp: new Date().toISOString(),
                type: "VERIFY",
                meta: event.meta,
                task: {
                    message: {
                        messageID: event.message.cc_id.message_id, // TODO ?
                        sourceChain: event.message.cc_id.source_chain,
                        sourceAddress: event.message.source_address,
                        destinationAddress: event.message.destination_address,
                        payloadHash: event.message.payload_hash
                    },
                    payload: event.payload
                }
            });
        }
    }
    let response = { results: body_json.events.map((_, index) => ({ status: "ACCEPTED", index })) }
    res.json(response);
});

// After we post a broadcast of type verify_messages, we trigger the return of CONSTRUCT_PROOF.
app.post('/contracts/:contract/broadcasts', async (req, res) => {
    try {
        const contract = req.params.contract;

        console.log(`Received broadcast for contract ${contract}:`);
        let body;
        try {
            body = JSON.parse(req.body);
        } catch (_) {
            body = req.body;
        }
        console.log(JSON.stringify(body, null, 2));

        const command = `axelard tx wasm execute ${contract} '${JSON.stringify(body)}' \
        --from ${AXELAR_SENDER} \
        --chain-id ${process.env.AXELAR_CHAIN_ID} \
        --gas auto \
        --gas-adjustment 1.4 \
        --gas-prices ${process.env.AXELAR_GAS_PRICES} \
        --output json \
        --keyring-backend test \
        --node ${process.env.AXELAR_RPC_URL} \
        -y`;

        console.log("Executing axelard command: " + command);
        if (body.verify_messages) {
            let command_res = (await exec(command)).stdout;

            // TODO: check if it was successfull
            // console.log(`\tVerifyMessages Response: ${command_res}`);
            return res.json(command_res);
        } else if (body.route_incoming_messages) {
            let command_res = (await exec(command)).stdout;

            // console.log(`\tRouteIncomingMessages Response: ${command_res}`);
            return res.json(command_res);
        } else if (body.construct_proof) {
            let command_res = (await exec(command)).stdout;
            // console.log(`\ConstructProof response: ${command_res}`);
            return res.json(command_res);
        } else if (body.confirm_tx_status) {
            let command_res = (await exec(command)).stdout;
            // console.log(`\SignMessages response: ${command_res}`);
            return res.json(command_res);
        } else if (body === "ticket_create") {
            return res.json((await exec(command)).stdout);
        }
        return res.status(404).json({ error: 'Unknown broadcast type' });
    } catch (error) {
        console.error(error);
        return res.status(500).json({ error: error.message });
    }
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

app.post('/contracts/:contract/queries', (req, res) => {
    const contract = req.params.contract;

    console.log(`Received query for contract ${contract}:`);
    console.log(JSON.stringify(JSON.parse(req.body), null, 2));

    const query = req.body;
    const command = `axelard q wasm contract-state smart ${contract} '${query}' --node ${process.env.AXELAR_RPC_URL} --output json`;
    console.log("Executing axelard command: " + command);
    exec(command, (error, stdout, stderr) => {
        if (error) {
            console.error(`\tError: ${error.message}`);
            res.status(500).send('Error executing query' + error.message);
            return;
        }

        res.json(JSON.parse(stdout).data);
    });
});

app.listen(port, () => {
    console.log(`Server is running on http://localhost:${port}`);
});
