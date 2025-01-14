require('dotenv').config({ path: __dirname + '/../.env' });
const Sentry = require('@sentry/node');
const { spawn } = require('child_process');
const axios = require('axios');

const logError = (error) => {
    console.error(error);
    Sentry.captureException(error);
}

/**
 * Spawns a child process and collects stdout/stderr.
 * Resolves with { stdout, stderr } or rejects with an Error.
 */
function spawnAsync(command, args = [], options = {}) {
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, options);
        let stdout = '';
        let stderr = '';

        child.stdout.on('data', (data) => {
            stdout += data;
        });

        child.stderr.on('data', (data) => {
            stderr += data;
        });

        child.on('error', (err) => {
            reject(err);
        });

        child.on('close', (code) => {
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

/**
 * Simple delay helper.
 */
function delay(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Retrieves current Axelar block height using `axelard status`.
 */
async function getCurrentAxelarHeight() {
    const args = ['status', '--node', process.env.AXELAR_RPC_URL];
    const { stderr } = await spawnAsync('axelard', args);
    try {
        const result = JSON.parse(stderr);
        return parseInt(result.SyncInfo.latest_block_height, 10);
    } catch (error) {
        logError('Failed to parse current axelar height:', error);
        throw error;
    }
}

/**
 * Runs an `axelard q txs ...` command to fetch all events
 * matching a specific wasm event type, from a given starting height.
 */
async function fetchEvents(eventType, contract, fromHeight) {
    const args = [
        'q', 'txs',
        '--events', `tx.height>${fromHeight} AND wasm-${eventType}._contract_address=${contract}`,
        '--node', process.env.AXELAR_RPC_URL,
        '--limit', '100',
        '--output', 'json'
    ]

    try {
        const { stdout } = await spawnAsync('axelard', args);
        const result = JSON.parse(stdout);

        if (result.total_count > 100) {
            console.warn(`More than 100 events found for ${eventType}!`);
        }

        const events = [];
        if (result?.txs && Array.isArray(result.txs)) {
            for (const tx of result.txs) {
                for (const log of tx.logs) {
                    const wasmEvent = log.events.find(e => e.type === `wasm-${eventType}`);
                    if (wasmEvent) {
                        events.push({ event: wasmEvent, height: tx.height });
                    }
                }
            }
        }
        return events;
    } catch (error) {
        logError(`Error executing fetchEvents command: ${error.message}`);
        return [];
    }
}

function isKnownBroadcastType(parsedBody) {
    return (
        parsedBody === 'ticket_create' ||
        parsedBody?.verify_messages ||
        parsedBody?.route_incoming_messages ||
        parsedBody?.construct_proof ||
        parsedBody?.confirm_tx_status
    );
}

/**
 * Constructs a REACT_TO_WASM_EVENT task if 
 * the quorum is reached on the source chain.
 */
function handleQuorumReached(event, height) {
    const statusAttr = event.attributes.find(attr => attr.key === 'status');
    const status = statusAttr?.value;

    if (!status || !status.includes('succeeded_on_source_chain')) {
        console.warn('Quorum not reached or invalid status found:', event);
        return null;
    }

    const content = event.attributes.find(attr => attr.key === 'content')?.value;
    if (!content) {
        console.warn('No content found in quorum_reached event.');
        return null;
    }

    return {
        id: height,
        chain: 'xrpl',
        timestamp: new Date().toISOString(),
        type: 'REACT_TO_WASM_EVENT',
        meta: null,
        task: {
            message: JSON.parse(content),
            event_name: 'wasm-quorum-reached'
        }
    };
}

/**
 * Constructs a CONSTRUCT_PROOF task if 
 * the event is for xrpl (destination_chain = 'xrpl').
 */
async function handleRoutingOutgoing(event, height) {
    const messageId = event.attributes.find(attr => attr.key === 'message_id')?.value;
    const sourceChain = event.attributes.find(attr => attr.key === 'source_chain')?.value;
    const destinationChain = event.attributes.find(attr => attr.key === 'destination_chain')?.value;
    const payloadHash = event.attributes.find(attr => attr.key === 'payload_hash')?.value;

    if (destinationChain !== 'xrpl') {
        return null;
    }

    // Attempt to retrieve payload from payload cache.
    let payload;
    try {
        const url = `${process.env.PAYLOAD_CACHE}?hash=${payloadHash}`;
        const response = (
            (await axios.get(
                url,
                {
                    headers: {
                        'Authorization': `Bearer ${process.env.PAYLOAD_CACHE_AUTH_TOKEN}`,
                        'Content-Type': 'text/plain'
                    }
                }
            ))
        );
        payload = response.data;
    } catch (error) {
        logError(`Could not find payload for ${sourceChain}_${messageId} \n ${error.message}`);
        return null;
    }

    const sourceAddress = event.attributes.find(attr => attr.key === 'source_address')?.value;
    const destinationAddress = event.attributes.find(attr => attr.key === 'destination_address')?.value;

    return {
        id: height,
        chain: 'xrpl',
        timestamp: new Date().toISOString(),
        type: 'CONSTRUCT_PROOF',
        meta: null,
        task: {
            message: {
                messageID: messageId,
                sourceChain,
                sourceAddress,
                destinationAddress,
                payloadHash
            },
            payload
        }
    };
}

/**
 * Constructs a GATEWAY_TX task if chain is 'xrpl'.
 * Reads the tx_blob from the wasm contract state.
 */
async function handleSigningCompleted(event, height) {
    const chain = event.attributes.find(attr => attr.key === 'chain')?.value;
    const sessionId = event.attributes.find(attr => attr.key === 'session_id')?.value;

    if (chain !== 'xrpl') {
        return null;
    }

    // Query contract state to get the execute data (tx_blob)
    const query = { proof: { multisig_session_id: sessionId } };
    const args = [
        'q', 'wasm', 'contract-state', 'smart', process.env.XRPL_MULTISIG_PROVER_ADDRESS,
        JSON.stringify(query),
        '--node', process.env.AXELAR_RPC_URL,
        '--output', 'json'
    ];

    try {
        const { stdout } = await spawnAsync('axelard', args);
        const executeRes = JSON.parse(stdout);
        return {
            id: height,
            chain: 'xrpl',
            timestamp: new Date().toISOString(),
            type: 'GATEWAY_TX',
            meta: null,
            task: {
                executeData: executeRes.data.tx_blob
            }
        };
    } catch (error) {
        logError('Error retrieving tx_blob:', error.message);
        return null;
    }
}

/**
 * Takes a single event and the block height, 
 * and returns either a Task object or null.
 */
async function processEvent(event, height) {
    switch (event.type) {
        case 'wasm-quorum_reached':
            return handleQuorumReached(event, height);

        case 'wasm-routing_outgoing':
            return handleRoutingOutgoing(event, height);

        case 'wasm-signing_completed':
            return handleSigningCompleted(event, height);

        default:
            // No recognized event type => no task
            return null;
    }
}

const autoincement_redis_key = 'gmp_api:task_autoincrement';
async function initTaskAutoIncrement(redisClient) {
    try {
        const storedValue = await redisClient.get(autoincement_redis_key);
        if (storedValue) {
            let value = parseInt(storedValue, 10);
            console.log(`(Re)loaded task_autoincrement from Redis: ${value}`);
            return value;
        } else {
            console.log("No existing task_autoincrement in Redis; starting at 0");
            return 0;
        }
    } catch (err) {
        console.error("Error fetching task_autoincrement from Redis:", err.message);
    }
    return 0;
}

async function updateTaskAutoIncrement(redisClient, value) {
    try {
        await redisClient.set(autoincement_redis_key, String(value));
    } catch (err) {
        console.error("Error saving task_autoincrement to Redis:", err.message);
    }
}

module.exports = {
    logError,
    spawnAsync,
    delay,
    getCurrentAxelarHeight,
    fetchEvents,
    isKnownBroadcastType,
    processEvent,
    initTaskAutoIncrement,
    updateTaskAutoIncrement
};
