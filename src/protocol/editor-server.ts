import { createServer } from 'node:http';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { CodeIntelligence } from '../engine.js';
import { EditorBuffers } from '../core/editor.js';
import { object, assert, Deadline, CIError } from '../core/util.js';
/** Editor capability is loopback only and grants buffer storage, never source writes. */
export async function serveEditor(engine: CodeIntelligence, port = 0): Promise<{
    endpoint: string;
    token: string;
    close: () => void;
}> {
    assert(Number.isInteger(port) && port >= 0 && port <= 65535, 'INVALID_PORT', 'Invalid editor port');
    const token = randomBytes(32).toString('hex'), editor = new EditorBuffers(engine.workspace);
    let host = '';
    let pending = Promise.resolve();
    const readTools = new Set(['ci_status', 'ci_find', 'ci_read', 'ci_trace', 'ci_context']);
    const server = createServer((req, res) => {
        const respond = (status: number, data: unknown) => { if (res.writableEnded)
            return; res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' }); res.end(JSON.stringify(data)); };
        if (req.headers.host !== host || req.headers.origin && req.headers.origin !== `http://${host}`) {
            respond(403, { error: 'Invalid origin or host' });
            return;
        }
        const auth = req.headers.authorization ?? '';
        if (Buffer.byteLength(auth) !== token.length + 7 || !timingSafeEqual(Buffer.from(auth), Buffer.from('Bearer ' + token))) {
            respond(401, { error: 'Authorization required' });
            return;
        }
        if (req.method !== 'POST' || req.url !== '/editor') {
            respond(404, { error: 'Not found' });
            return;
        }
        let size = 0, rejected = false;
        const chunks: Buffer[] = [];
        req.on('data', (b: Buffer) => { size += b.length; if (size > engine.workspace.config.maxFileBytes * 6 + 64000) {
            rejected = true;
            respond(413, { error: 'Request too large' });
        }
        else if (!rejected)
            chunks.push(b); });
        req.on('end', () => {
            if (rejected)
                return;
            // Serialize editor mutations; versions still protect multiple clients.
            pending = pending.then(async () => {
                try {
                    const input = object(JSON.parse(Buffer.concat(chunks).toString('utf8')));
                    let data: unknown;
                    const id = String(input.sessionId ?? '');
                    switch (input.operation) {
                        case 'open':
                            data = editor.create();
                            break;
                        case 'buffer':
                            data = editor.update(id, object(input.buffer) as any);
                            break;
                        case 'closeFile':
                            editor.closeFile(id, String(input.path), input.version as number | undefined);
                            data = { closed: true };
                            break;
                        case 'close':
                            editor.close(id);
                            data = { closed: true };
                            break;
                        case 'status':
                            data = editor.session(id);
                            break;
                        case 'capture':
                            data = editor.capture(id, new Deadline(engine.workspace.config.timeoutMs));
                            break;
                        case 'tool': {
                            assert(readTools.has(String(input.tool)), 'CAPABILITY_UNAVAILABLE', 'Editor API exposes read-only agent tools');
                            const snapshot = editor.capture(id, new Deadline(engine.workspace.config.timeoutMs));
                            data = await engine.invoke(String(input.tool), { ...object(input.arguments ?? {}), snapshotId: snapshot.id });
                            break;
                        }
                        default: throw new CIError('INVALID_ARGUMENT', 'Unknown editor operation');
                    }
                    respond(200, { ok: true, data });
                }
                catch (error) {
                    respond(400, { ok: false, error: { code: error instanceof CIError ? error.code : 'INVALID_REQUEST', message: (error as Error).message.slice(0, 300) } });
                }
            }).catch(() => respond(500, { error: 'Editor operation failed' }));
        });
    });
    server.requestTimeout = 15000;
    server.headersTimeout = 10000;
    await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', () => { const a = server.address(); assert(a && typeof a !== 'string', 'SERVER_FAILED', 'No editor address'); host = `127.0.0.1:${a.port}`; resolve(); }); });
    return { endpoint: `http://${host}/editor`, token, close: () => { server.closeAllConnections(); server.close(); } };
}
