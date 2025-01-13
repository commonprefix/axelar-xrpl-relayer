require("./instrument.js");
const express = require('express');
const bodyParser = require('body-parser');
const { logError, delay, fetchEvents, processEvent, getCurrentAxelarHeight, isKnownBroadcastType, spawnAsync, initTaskAutoIncrement, updateTaskAutoIncrement } = require('./utils');
const { createClient } = require('redis');

const app = express();
const port = 3001;
const redisClient = createClient({ url: process.env.REDIS_SERVER });

const AXELAR_SENDER = process.env.GMP_API_WALLET;
const START_HEIGHT = 0;
let tasks = [];
let task_autoincrement = 0;


async function mainLoop() {
    let latestHeight = START_HEIGHT || await getCurrentAxelarHeight();

    while (true) {
        try {
            const allEvents = await Promise.all([
                fetchEvents('quorum_reached', process.env.XRPL_VOTING_VERIFIER_ADDRESS, latestHeight),
                fetchEvents('routing_outgoing', process.env.XRPL_GATEWAY_ADDRESS, latestHeight),
                fetchEvents('signing_completed', process.env.MULTISIG_CONTRACT_ADDRESS, latestHeight)
            ]);

            // Flatten the array of arrays
            const events = allEvents.flat();

            for (let { event, height } of events) {
                height = Number(height);
                let task = await processEvent(event, height);

                if (task) {
                    task.id = task_autoincrement++;
                    console.log(`Creating task: ${JSON.stringify(task, null, 2)}`);
                    tasks.push(task);
                    updateTaskAutoIncrement(redisClient, task_autoincrement);
                }
            }

            if (events.length > 0) {
                const maxEventHeight = Math.max(...events.map(e => Number(e.height)));
                latestHeight = Math.max(latestHeight, maxEventHeight);
            }

            await delay(2000);
        } catch (error) {
            logError('Error in mainLoop:', error.message);
            await delay(2000);
        }
    }
};

app.use(bodyParser.text());

app.get('/chains/xrpl/tasks', (req, res) => {
    const afterParam = req.query.after;

    const after = afterParam ? Number(afterParam) : null;

    const filteredTasks = after !== null && !isNaN(after)
        ? tasks.filter(task => task.id > after)
        : tasks;

    res.json({ tasks: filteredTasks });
});

app.post('/chains/xrpl/events', (req, res) => {
    console.log("Received event: ");
    let bodyJson
    try {
        bodyJson = JSON.parse(req.body);
    } catch (err) {
        logError('Invalid JSON in request body:', err.message);
        return res.status(400).json({ error: 'Invalid JSON' });
    }
    console.log(JSON.stringify(bodyJson, null, 2));

    for (let event of bodyJson.events) {
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
            updateTaskAutoIncrement(redisClient, task_autoincrement);
        }
    }
    let response = { results: bodyJson.events.map((_, index) => ({ status: "ACCEPTED", index })) }
    res.json(response);
});

app.post('/contracts/:contract/broadcasts', async (req, res) => {
    const contract = req.params.contract;
    console.log(`Received broadcast for contract ${contract}:`);

    let parsedBody;
    try {
        parsedBody = JSON.parse(req.body);
    } catch (err) {
        // If body is not valid JSON, store it as string
        parsedBody = req.body;
    }
    console.log(JSON.stringify(parsedBody, null, 2));

    const args = [
        'tx', 'wasm', 'execute', contract, JSON.stringify(parsedBody),
        '--from', AXELAR_SENDER, '--chain-id', process.env.AXELAR_CHAIN_ID,
        '--gas', 'auto', '--gas-adjustment', '1.4', '--gas-prices', process.env.AXELAR_GAS_PRICES,
        '--output', 'json', '--keyring-backend', 'test', '--node', process.env.AXELAR_RPC_URL, '-y'
    ]

    console.log('Executing axelard command: axelard', args.join(' '));
    try {
        if (isKnownBroadcastType(parsedBody)) {
            const { stdout } = await spawnAsync('axelard', args);
            return res.json(stdout);
        } else {
            return res.status(404).json({ error: 'Unknown broadcast type' });
        }
    } catch (error) {
        logError('Broadcast error:', error);
        return res.status(500).json({ error: error.message });
    }
});

app.post('/contracts/:contract/queries', async (req, res) => {
    const contract = req.params.contract;
    console.log(`Received query for contract ${contract}:`);

    let bodyJson;
    try {
        bodyJson = JSON.parse(req.body);
        console.log(JSON.stringify(bodyJson, null, 2));
    } catch (err) {
        logError('Invalid JSON in query:', err.message);
        return res.status(400).json({ error: 'Invalid JSON' });
    }

    const args = [
        'q', 'wasm', 'contract-state', 'smart', contract, JSON.stringify(bodyJson),
        '--node', process.env.AXELAR_RPC_URL,
        '--output', 'json'
    ]

    console.log("Executing axelard command: axelard " + args.join(' '));

    let { stdout, stderr } = await spawnAsync('axelard', args);
    if (stderr) {
        logError(`Error executing query: ${stderr}`);
        return res.status(500).json({ error: stderr });
    }

    try {
        const parsed = JSON.parse(stdout);
        res.json(parsed.data);
    } catch (parseErr) {
        logError('Could not parse axelard query stdout:', parseErr.message);
        res.status(500).send('Failed to parse query result');
    }
});

(async function start() {
    redisClient.on('error', err => console.log('Redis Client Error', err));
    await redisClient.connect();

    task_autoincrement = await initTaskAutoIncrement(redisClient);

    mainLoop();

    app.listen(port, () => {
        console.log(`Server is running on http://localhost:${port}`);
    });
})();
