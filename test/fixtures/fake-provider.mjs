// Deterministic protocol peer. This is NOT an actual third-party provider.
import { pathToFileURL } from 'node:url';
const mode = process.argv[2];
let buffer = Buffer.alloc(0), root;
function send(v) { const data = JSON.stringify(v); const packet = mode === 'mcp' ? Buffer.from(data + '\n') : Buffer.from(`Content-Length: ${Buffer.byteLength(data)}\r\n\r\n${data}`); process.stdout.write(packet.subarray(0, 9)); setTimeout(() => process.stdout.write(packet.subarray(9)), 1); }
function handle(m) {
    if (m.id === undefined)
        return;
    if (m.method === 'hang')
        return;
    let result = m.params;
    if (m.method === 'initialize') {
        root = m.params.rootUri;
        result = mode === 'mcp' ? { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '1' } } : { capabilities: { positionEncoding: 'utf-16', renameProvider: true } };
    }
    if (m.method === 'textDocument/rename') {
        const uri = mode === 'escape' ? pathToFileURL('/outside/a.ts').href : m.params.textDocument.uri;
        result = { changes: { [uri]: [{ range: { start: { line: 0, character: 16 }, end: { line: 0, character: 19 } }, newText: m.params.newName }] } };
    }
    if (m.method === 'tools/list')
        result = { tools: [{ name: 'find_symbol', inputSchema: { type: 'object' } }] };
    if (m.method === 'tools/call')
        result = { structuredContent: { 'a.ts': [{ name_path: 'one', kind: 12 }] }, content: [] };
    send({ jsonrpc: '2.0', id: m.id, result });
}
process.stdin.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length) {
        let body;
        if (mode === 'mcp') {
            const end = buffer.indexOf(10);
            if (end < 0)
                return;
            body = buffer.subarray(0, end);
            buffer = buffer.subarray(end + 1);
        }
        else {
            const end = buffer.indexOf('\r\n\r\n');
            if (end < 0)
                return;
            const size = Number(/Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
            if (buffer.length < end + 4 + size)
                return;
            body = buffer.subarray(end + 4, end + 4 + size);
            buffer = buffer.subarray(end + 4 + size);
        }
        handle(JSON.parse(body));
    }
});
