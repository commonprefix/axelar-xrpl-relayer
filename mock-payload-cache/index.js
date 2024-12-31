require('dotenv').config({ path: __dirname + '/../.env' });
const express = require('express');
const keccak256 = require('keccak256');
const bodyParser = require('body-parser');
const swaggerUi = require('swagger-ui-express');
const fs = require("fs")
const YAML = require('yaml')
const file  = fs.readFileSync(`${__dirname}/swagger.yaml`, 'utf8')
const swaggerDocument = YAML.parse(file)

const store = { "d86057bf496f39aa6c4cc1bb2afcda866136aeb3b9c9305b4bdd23ae424c783a": "0000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000047872706c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000000002f2a4001b68aa7f3c57af787bf256b7de021046bafcfdc1488c640a3a43290a600000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000001b69b4bacd05f15000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000000140a90c0af1b07f6ac34f3520348dbfae73bda358e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000014928846baf59bd48c28b131855472d04c93bbd0b70000000000000000000000000000000000000000000000000000000000000000000000000000000000000000" };
const AUTH_TOKEN = process.env.PAYLOAD_CACHE_AUTH_TOKEN;

const app = express();
const port = 5001;

app.use('/docs', swaggerUi.serve, swaggerUi.setup(swaggerDocument));

app.use(bodyParser.text({ type: 'text/plain' }));

app.use((req, res, next) => {
    const authHeader = req.headers['authorization'];

    if (!authHeader || authHeader !== `Bearer ${AUTH_TOKEN}`) {
        return res.status(403).json({ error: 'Forbidden - invalid or missing token' });
    }

    next();
});

app.post('/payload', (req, res) => {
    const body = req.body;

    if (!body || typeof body !== 'string') {
        return res.status(400).json({ error: 'Invalid payload. Expecting a hex-encoded string.' });
    }

    let buffer;
    try {
        buffer = Buffer.from(body, 'hex');
    } catch (error) {
        return res.status(400).json({ error: 'Invalid hex string.' });
    }

    const hashBuffer = keccak256(buffer);
    const hash = hashBuffer.toString('hex').toLowerCase();

    store[hash] = body;

    res.status(200).json({ hash });
});

app.get('/payload', (req, res) => {
    const queryHash = req.query.hash;

    if (!queryHash) {
        return res.status(400).json({ error: 'Missing hash query parameter.' });
    }

    const payload = store[queryHash];

    if (payload) {
        return res.status(200).type('text/plain').send(payload);
    } else {
        return res.status(404).json({ error: 'Not found' });
    }
});

app.get('/debug', (req, res) => {
    res.status(200).type('application/json').send(JSON.stringify(store, null, 2));
});

app.use((req, res) => {
    res.status(404).json({ error: 'Not found' });
});

app.listen(port, () => {
    console.log(`Express server is listening on port ${port}`);
    console.log(`Swagger UI available at http://localhost:${port}/docs`);
});