require("./instrument.js");
require('dotenv').config({ path: __dirname + '/../.env' });
const axios = require('axios');
const { spawn } = require('child_process');
const { logError } = require("../mock-gmp-api/utils.js");

const AXELAR_SENDER = process.env.AXELAR_RELAYER_WALLET;

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

async function fetch_events(event_type, contract, from_height) {
    const command = 'axelard';
    const args = [
        'q', 'txs',
        '--events', `tx.height>${from_height} AND wasm-${event_type}._contract_address=${contract}`,
        '--node', process.env.AXELAR_RPC_URL,
        '--limit', '100',
        '--output', 'json'
    ]

    try {
        // let result = JSON.parse((await exec(command)).stdout);
        let result = JSON.parse((await spawnAsync(command, args)).stdout);
        if (result.total_count > 100) {
            console.warn(`More than 100 events found for ${event_type}!`);
        }
        const events = [];
        if (result && result.txs && Array.isArray(result.txs)) {
            for (let tx of result.txs) {
                if (Number(tx.height) <= from_height) {
                    continue;
                }
                for (let log of tx.logs) {
                    let event = log.events.find(event => event.type === `wasm-${event_type}`);
                    if (event) {
                        events.push({ event, height: tx.height, tx });
                    }
                }
            }
        }
        return events;
    } catch (error) {
        logError(`Error executing command: ${error.message}`);
        return [];
    }
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

(async () => {
    let current_height = await get_current_axelar_height();
    // let current_height = 982038;
    console.log("Starting from height:", current_height);

    let latest_height = current_height;
    while (true) {
        try {
            let routing_events = await fetch_events('routing', process.env.AXELARNET_GATEWAY, latest_height);
            for (let { event, height, tx } of routing_events) {
                let destination_chain = event.attributes.find(attr => attr.key === "destination_chain").value;
                if (destination_chain !== 'axelar') {
                    continue;
                }
                let source_chain = event.attributes.find(attr => attr.key === "source_chain").value;
                let message_id = event.attributes.find(attr => attr.key === "message_id").value;
                let original_payload_hash = event.attributes.find(attr => attr.key === "payload_hash").value;

                // Convert the payload on Axelarnet Gateway
                let original_payload = null;
                if (source_chain == 'xrpl') {
                    for (let log of tx.logs) {
                        let contract_called = log.events.find(event => (event.type === "wasm-contract_called" && event.attributes.find(attr => attr.key === "_contract_address").value === process.env.XRPL_GATEWAY_ADDRESS));
                        original_payload = contract_called.attributes.find(attr => attr.key === "payload").value;
                    }
                } else {
                    try {
                        original_payload = (
                            (await axios.get(
                                `${process.env.PAYLOAD_CACHE}?hash=${original_payload_hash}`,
                                {
                                    headers: {
                                        'Authorization': `Bearer ${process.env.PAYLOAD_CACHE_AUTH_TOKEN}`,
                                        'Content-Type': 'text/plain'
                                    }
                                }
                            ))
                        ).data;
                    } catch (error) {
                        logError(`Failed to process event: ${JSON.stringify(event)}\nError fetching payload from cache.`);
                        continue;
                    }
                }
                let execute_msg = {
                    execute: {
                        cc_id: {
                            source_chain,
                            message_id
                        },
                        payload: original_payload
                    }
                };

                const command = 'axelard';
                const args = [
                    'tx', 'wasm', 'execute', process.env.AXELARNET_GATEWAY, JSON.stringify(execute_msg),
                    '--from', AXELAR_SENDER,
                    '--chain-id', process.env.AXELAR_CHAIN_ID,
                    '--gas', 'auto',
                    '--gas-adjustment', '1.4',
                    '--gas-prices', process.env.AXELAR_GAS_PRICES,
                    '--output', 'json',
                    '--keyring-backend', 'test',
                    '--node', process.env.AXELAR_RPC_URL,
                    '-y'
                ];
                try {
                    let execute_res = JSON.parse((await spawnAsync(command, args)).stdout);
                    let payload = null;
                    for (let log of execute_res.logs) {
                        for (let event of log.events) {
                            if (event.type === "wasm-contract_called") {
                                if (event.attributes.find(attr => attr.key === "_contract_address").value === process.env.AXELARNET_GATEWAY) {
                                    its_payload = event.attributes.find(attr => attr.key === "payload").value;
                                    its_payload_hash = event.attributes.find(attr => attr.key === "payload_hash").value;
                                    const payload_hash = (
                                        await axios.post(
                                            process.env.PAYLOAD_CACHE,
                                            its_payload,
                                            {
                                                headers: {
                                                    'Authorization': `Bearer ${process.env.PAYLOAD_CACHE_AUTH_TOKEN}`,
                                                    'Content-Type': 'text/plain'
                                                }
                                            }
                                        )
                                    ).data.hash;
                                    if (payload_hash !== its_payload_hash) {
                                        logError(`Payload hash mismatch; Received hash: ${payload_hash}, expected hash: ${its_payload_hash}`);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                } catch (err) {
                    logError(`Failed to process event: ${JSON.stringify(event)}\nError executing payload on Axelarnet Gateway.`);
                    logError(err);
                    continue;
                }
                latest_height = Math.max(latest_height, height);
            }
        } catch (e) {
            logError('Main loop failed:');
            logError(e);
        }
        await delay(2000);
    }
})()
