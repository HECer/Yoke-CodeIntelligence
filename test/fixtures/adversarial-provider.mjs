// Test-only scripted peer. This is NOT a third-party language server.
import { pathToFileURL, fileURLToPath } from 'node:url';
import { writeFileSync } from 'node:fs';
const spec = JSON.parse(process.argv[2] ?? '{}');
let buffer = Buffer.alloc(0), root, pending, opened = [];
const packet = value => { const body = JSON.stringify(value); return Buffer.from(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`); };
const send = value => process.stdout.write(packet(value));
const range = (start = 0, end = 4, line = 0) => ({ start: { line, character: start }, end: { line, character: end } });
function handle(m) {
    if (m.method === 'textDocument/didOpen') {
        opened.push(m.params.textDocument);
        return;
    }
    if (m.method === 'exit') {
        process.exit(0);
        return;
    }
    if (!m.method && m.id === 'server-request') {
        send({ jsonrpc: '2.0', id: pending, result: m });
        return;
    }
    if (m.id === undefined)
        return;
    let result = null;
    if (m.method === 'initialize') {
        root = m.params.rootUri;
        result = { capabilities: { positionEncoding: spec.encoding ?? 'utf-16', definitionProvider: !spec.noDefinition, referencesProvider: true, implementationProvider: true, renameProvider: spec.prepare ? { prepareProvider: true } : true }, serverInfo: { name: 'scripted-fixture', version: '1' } };
    }
    else if (m.method === 'opened')
        result = opened;
    else if (m.method === 'many') {
        const packets = Array.from({ length: 30 }, () => packet({ jsonrpc: '2.0', method: 'fixture/progress', params: { message: 'x'.repeat(100) } }));
        packets.push(packet({ jsonrpc: '2.0', id: m.id, result: 'batch-ok' }));
        process.stdout.write(Buffer.concat(packets));
        return;
    }
    else if (m.method === 'malformed') {
        if (spec.kind === 'utf8') {
            const body = Buffer.concat([Buffer.from('{"jsonrpc":"2.0","id":' + m.id + ',"result":"'), Buffer.from([0xff]), Buffer.from('"}')]);
            process.stdout.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
            return;
        }
        if (spec.kind === 'duplicate-header') {
            process.stdout.write('Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}');
            return;
        }
        if (spec.kind === 'giant') {
            process.stdout.write('Content-Length: 9999999999999999999999\r\n\r\n');
            return;
        }
        if (spec.kind === 'long-header') {
            process.stdout.write('X-Long: ' + 'x'.repeat(9000) + '\r\nContent-Length: 2\r\n\r\n{}');
            return;
        }
        const malformed = spec.kind === 'both' ? { jsonrpc: '2.0', id: m.id, result: 1, error: { code: -1, message: 'bad' } } : spec.kind === 'neither' ? { jsonrpc: '2.0', id: m.id } : spec.kind === 'bad-error' ? { jsonrpc: '2.0', id: m.id, error: 'bad' } : [1, 2];
        send(malformed);
        return;
    }
    else if (m.method === 'callback') {
        pending = m.id;
        send({ jsonrpc: '2.0', id: 'server-request', method: spec.callback ?? 'invented/execute', params: spec.params ?? {} });
        return;
    }
    else if (m.method === 'diagnostic-noise') {
        for (let i = 0; i < 10; i++)
            send({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: { uri: `file:///outside/${i}`, version: 1, diagnostics: [{ message: 'foreign' }] } });
        for (const version of [1, 2])
            send({ jsonrpc: '2.0', method: 'textDocument/publishDiagnostics', params: { uri: opened[0].uri, version, diagnostics: [{ message: version === 1 ? 'current' : 'stale' }] } });
        result = true;
    }
    else if (m.method === 'textDocument/prepareRename')
        result = spec.refusePrepare ? null : { range: range(0, 4), placeholder: 'name' };
    else if (['textDocument/definition', 'textDocument/references', 'textDocument/implementation'].includes(m.method)) {
        const uri = spec.external ? pathToFileURL('/outside/private.ts').href : m.params.textDocument.uri;
        result = spec.noResult ? null : spec.badLocation ? { unexpected: true } : spec.link ? { targetUri: uri, targetRange: range(0, spec.clamp ? 999 : 8), targetSelectionRange: range(0, spec.invalidSelection ? 10 : 4) } : { uri, range: spec.range ?? range() };
        if (spec.duplicate)
            result = [result, result];
    }
    else if (m.method === 'textDocument/rename') {
        const uri = spec.external ? pathToFileURL('/outside/private.ts').href : m.params.textDocument.uri;
        const textEdit = { range: spec.range ?? range(), newText: spec.invalidText ? '\ud800' : m.params.newName, ...(spec.annotation ? { annotationId: 'review' } : {}) };
        const edits = spec.overlap ? [textEdit, { range: range(2, 5), newText: 'x' }] : spec.duplicate ? [textEdit, textEdit] : [textEdit];
        result = spec.resource ? { documentChanges: [{ kind: 'delete', uri }] } : spec.version !== undefined ? { documentChanges: [{ textDocument: { uri, version: spec.version }, edits }] } : { changes: { [uri]: edits } };
        if (spec.both)
            result.documentChanges = [];
    }
    if (spec.mutate && m.method.startsWith('textDocument/') && m.params?.textDocument?.uri)
        writeFileSync(fileURLToPath(m.params.textDocument.uri), 'mutated');
    send({ jsonrpc: '2.0', id: m.id, result });
}
process.stdin.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {
        const end = buffer.indexOf('\r\n\r\n');
        if (end < 0)
            return;
        const size = Number(/Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
        if (buffer.length < end + 4 + size)
            return;
        const body = buffer.subarray(end + 4, end + 4 + size);
        buffer = buffer.subarray(end + 4 + size);
        handle(JSON.parse(body));
    }
});
