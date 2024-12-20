const http = require('http');
const crypto = require('crypto');

// In-memory store: { hash: payload }
const store = {};

const server = http.createServer((req, res) => {
    if (req.method === 'POST' && req.url === '/') {
        let body = '';

        // Gather the body from the request
        req.on('data', chunk => {
            body += chunk;
        });

        req.on('end', () => {
            // Compute SHA-256 hash of the payload
            const hash = crypto
                .createHash('sha256')
                .update(body)
                .digest('hex');

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

server.listen(3000, () => {
    console.log('HTTP server is listening on port 3000');
});
