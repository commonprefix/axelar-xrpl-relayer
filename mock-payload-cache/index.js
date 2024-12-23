const http = require('http');
const keccak256 = require('keccak256');

// In-memory store: { hash: payload }
const store = { "d86057bf496f39aa6c4cc1bb2afcda866136aeb3b9c9305b4bdd23ae424c783a": "0000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000047872706c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000000002f2a4001b68aa7f3c57af787bf256b7de021046bafcfdc1488c640a3a43290a600000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000001b69b4bacd05f15000000000000000000000000000000000000000000000000000000000000014000000000000000000000000000000000000000000000000000000000000000140a90c0af1b07f6ac34f3520348dbfae73bda358e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000014928846baf59bd48c28b131855472d04c93bbd0b70000000000000000000000000000000000000000000000000000000000000000000000000000000000000000" };

const server = http.createServer((req, res) => {
    if (req.method === 'POST' && req.url === '/') {
        let body = '';

        // Gather the body from the request
        req.on('data', chunk => {
            body += chunk;
        });

        req.on('end', () => {
            const buffer = Buffer.from(body, 'hex');
            const hashBuffer = keccak256(buffer);
            const hash = hashBuffer.toString('hex').toLowerCase();

            // Store in memory
            store[hash] = body;

            // Return the hash as JSON
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ hash }));
        });
    } else if (req.method === 'GET') {
        // Parse the query parameter for 'hash'
        const url = new URL(req.url, `http://${req.headers.host}`);
        const queryHash = url.searchParams.get('hash');

        if (queryHash && store[queryHash]) {
            // If found, return the associated payload
            res.writeHead(200, { 'Content-Type': 'text/plain' });
            res.end(store[queryHash]);
        } else {
            // If not found, return a 404 error as JSON
            res.writeHead(404, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'Not found' }));
        }
    } else {
        // Any other request results in a 404
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Not found' }));
    }
});

server.listen(5001, () => {
    console.log('HTTP server is listening on port 5001');
});
