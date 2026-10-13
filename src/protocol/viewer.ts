import { createServer } from 'node:http';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { CodeIntelligence } from '../engine.js';
import { assert } from '../core/util.js';
/** A read-only loopback explorer. No remote bind, cookies, CDN or mutating tool. */
export async function serveViewer(engine: CodeIntelligence, port = 0): Promise<{
    url: string;
    close: () => void;
}> {
    assert(Number.isSafeInteger(port) && port >= 0 && port <= 65535, 'INVALID_PORT', 'Port must be 0..65535');
    const token = randomBytes(32).toString('hex');
    const html = readFileSync(new URL('../../workers/viewer.html', import.meta.url));
    const script = readFileSync(new URL('../../workers/viewer.js', import.meta.url));
    const css = readFileSync(new URL('../../workers/viewer.css', import.meta.url));
    const allowed = new Set(['ci_status', 'ci_find', 'ci_read', 'ci_trace', 'ci_context']);
    let address = '';
    const server = createServer(async (req, res) => {
        const respond = (status: number, data: string | Buffer, type = 'application/json') => {
            res.writeHead(status, { 'Content-Type': `${type}; charset=utf-8`, 'Cache-Control': 'no-store',
                'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer',
                'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'" });
            res.end(data);
        };
        if (req.headers.host !== address || (req.headers.origin && req.headers.origin !== `http://${address}`)) {
            respond(403, '{"error":"Invalid host or origin"}');
            return;
        }
        if (req.method === 'GET' && req.url === '/') {
            respond(200, html, 'text/html');
            return;
        }
        if (req.method === 'GET' && req.url === '/viewer.js') {
            respond(200, script, 'text/javascript');
            return;
        }
        if (req.method === 'GET' && req.url === '/viewer.css') {
            respond(200, css, 'text/css');
            return;
        }
        if (req.method !== 'POST' || req.url !== '/api') {
            respond(404, '{"error":"Not found"}');
            return;
        }
        const candidate = req.headers.authorization?.replace(/^Bearer /, '') ?? '';
        if (Buffer.byteLength(candidate) !== token.length || !timingSafeEqual(Buffer.from(candidate), Buffer.from(token))) {
            respond(401, '{"error":"Authorization required"}');
            return;
        }
        const abort = new AbortController();
        let body = Buffer.alloc(0), tooLarge = false;
        req.on('aborted', () => abort.abort());
        req.on('data', (chunk: Buffer) => {
            if (tooLarge)
                return;
            if (body.length + chunk.length > 64000) {
                tooLarge = true;
                respond(413, '{"error":"Request too large"}');
                return;
            }
            body = Buffer.concat([body, chunk]);
        });
        req.on('end', () => {
            void (async () => {
                if (tooLarge)
                    return;
                try {
                    const input = JSON.parse(body.toString('utf8'));
                    if (!allowed.has(input.tool) || (input.tool === 'ci_status' && ![undefined, 'status', 'index', 'analyze'].includes(input.arguments?.mode))) {
                        respond(403, '{"error":"Only read-only tools are available"}');
                        return;
                    }
                    const result = await engine.invoke(input.tool, input.arguments ?? {}, abort.signal);
                    respond(200, JSON.stringify(result));
                }
                catch {
                    respond(400, '{"error":"Invalid request"}');
                }
            })();
        });
    });
    server.requestTimeout = 15000;
    server.headersTimeout = 10000;
    await new Promise<void>((resolve, reject) => {
        server.once('error', reject);
        server.listen(port, '127.0.0.1', () => {
            const bound = server.address();
            if (!bound || typeof bound === 'string') {
                reject(new Error('No listening address'));
                return;
            }
            address = `127.0.0.1:${bound.port}`;
            resolve();
        });
    });
    return { url: `http://${address}/#${token}`, close: () => { server.closeAllConnections(); server.close(); } };
}
