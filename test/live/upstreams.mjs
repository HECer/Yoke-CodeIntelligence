/** Mandatory live assertions; missing packages FAIL, never turn into simulated passes. */
import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../helpers.mjs';
import { runBridge } from '../../dist/adapters/bridges.js';
import { runSerena } from '../../dist/adapters/serena-command.js';
import { normalizeSerenaSymbols } from '../../dist/adapters/graph-import.js';
if (!process.env.CI_LIVE_UPSTREAMS)
    throw Error('Run only with explicit CI_LIVE_UPSTREAMS=1 after reviewing installed providers');
for (const backend of ['graft', 'graphify'])
    test(`LIVE installed ${backend} pinned structural extraction`, async (t) => {
        const prefix = '# 🌍\r\n', source = `${prefix}def greet(name):\r\n    return name\r\n`;
        const caller = 'from a import greet\r\nx=greet("one")\r\n';
        const f = fixture(t, { 'a.py': source, 'b.py': caller }), s = f.snapshot();
        const r = await runBridge(f.engine, backend, s.id, { trusted: true, timeoutMs: 120000 });
        assert.equal(r.imported, true);
        const symbol = f.ws.store.graph(s.id).entities.find(e => e.name === 'greet' && e.path === 'a.py');
        assert(symbol, 'Actual provider must return the captured declaration');
        assert.equal(symbol.startLine, 2);
        assert.equal(symbol.startByte, Buffer.byteLength(prefix), 'Original UTF-8/CRLF bytes must define the source anchor');
        assert(Buffer.from(source).subarray(symbol.startByte, symbol.endByte).toString('utf8').startsWith('def greet('));
        assert.equal(f.read('a.py'), source);
        assert.equal(f.read('b.py'), caller);
    });
test('LIVE installed Serena capability discovery and selected semantic symbol query', async (t) => {
    assert(process.env.CI_SERENA_COMMAND, 'Set reviewed CI_SERENA_COMMAND and JSON CI_SERENA_ARGS including {workspace}');
    const args = JSON.parse(process.env.CI_SERENA_ARGS ?? '[]');
    const source = 'def greet(name):\n    return name\n';
    const f = fixture(t, { 'a.py': source });
    const snapshot = f.snapshot();
    const options = { trusted: true, command: process.env.CI_SERENA_COMMAND, args, snapshotId: snapshot.id, timeoutMs: 120000 };
    const caps = await runSerena(f.engine, 'capabilities', undefined, {}, options);
    assert(caps.capabilities.some(tool => tool.name === 'find_symbol'));
    const response = await runSerena(f.engine, 'read', 'find_symbol', { name_path_pattern: 'greet', relative_path: 'a.py', include_body: false }, options);
    assert.equal(response.sourceFilesUnchanged, true);
    assert.equal(response.snapshotId, snapshot.id);
    assert.equal(f.read('a.py'), source);
    let payload = response.data;
    if (Array.isArray(payload) && payload.length === 1 && payload[0]?.type === 'text')
        payload = payload[0].text;
    if (typeof payload?.result === 'string')
        payload = payload.result;
    assert(normalizeSerenaSymbols(payload).some(symbol => symbol.namePath === 'greet' && symbol.path === 'a.py'));
});
