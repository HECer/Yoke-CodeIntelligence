import test from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fixture } from './helpers.mjs';
import { Deadline, TextIndex } from '../dist/core/util.js';
import { RpcPeer } from '../dist/adapters/rpc.js';
import { LspAdapter } from '../dist/adapters/lsp.js';
const peerFile = fileURLToPath(new URL('./fixtures/adversarial-provider.mjs', import.meta.url));
const deadline = () => new Deadline(10000);
async function setup(t, spec = {}, source = 'nameText\r\nsecond\r\n') {
    const f = fixture(t, { 'main.ts': source });
    const s = f.snapshot();
    const adapter = new LspAdapter(f.ws);
    f.cleanup(() => adapter.close());
    await adapter.start(process.execPath, [peerFile, JSON.stringify(spec)], s, join(f.ws.store.directory, 'changes', 'lsp', 'tree'), deadline());
    return { ...f, s, adapter };
}
for (const kind of ['both', 'neither', 'bad-error', 'array', 'utf8', 'duplicate-header', 'giant', 'long-header']) {
    test(`RPC rejects malformed ${kind} response without resolving a pending call`, async (t) => {
        const f = fixture(t);
        const p = new RpcPeer('lsp', 1024);
        f.cleanup(() => p.close());
        p.start(process.execPath, [peerFile, JSON.stringify({ kind })], f.root);
        await assert.rejects(() => p.request('malformed', {}, deadline()), e => ['PROTOCOL_ERROR', 'PROTOCOL_LIMIT'].includes(e.code));
    });
}
test('RPC bounds individual frames without rejecting many coalesced valid messages', async (t) => {
    const f = fixture(t), p = new RpcPeer('lsp', 512);
    f.cleanup(() => p.close());
    p.start(process.execPath, [peerFile, '{}'], f.root);
    assert.equal(await p.request('many', {}, deadline()), 'batch-ok');
});
test('unsupported server callbacks receive MethodNotFound, not a false success', async (t) => {
    const f = fixture(t), p = new RpcPeer('lsp');
    f.cleanup(() => p.close());
    p.start(process.execPath, [peerFile, '{}'], f.root);
    const r = await p.request('callback', {}, deadline());
    assert.equal(r.error.code, -32601);
    assert(!('result' in r));
});
test('server applyEdit callbacks are refused and cannot mutate workspace files', async (t) => {
    const f = fixture(t, { 'a.txt': 'original' }), p = new RpcPeer('lsp');
    f.cleanup(() => p.close());
    p.start(process.execPath, [peerFile, JSON.stringify({ callback: 'workspace/applyEdit', params: { edit: {} } })], f.root);
    const r = await p.request('callback', {}, deadline());
    assert.equal(r.result.applied, false);
    assert.equal(f.read('a.txt'), 'original');
});
test('LSP locations normalize both regular Location and LocationLink against source hashes', async (t) => {
    const f = await setup(t, { link: true });
    const r = await f.adapter.locations('definition', 'main.ts', 0, deadline());
    assert.equal(r.items[0].contentHash, f.ws.file(f.s, 'main.ts').hash);
    assert.equal(r.items[0].selection.endByte, 4);
    assert.equal(r.items[0].endByte, 8);
    assert.equal(r.exhaustive, false);
    assert.equal(r.serverInfo.name, 'scripted-fixture');
});
test('LSP external locations are explicitly reported but their files are never read', async (t) => {
    const f = await setup(t, { external: true });
    const r = await f.adapter.locations('definition', 'main.ts', 0, deadline());
    assert.equal(r.items.length, 0);
    assert.equal(r.external.length, 1);
    assert.equal(r.external[0].reason, 'outside_captured_source_not_read');
});
test('LSP null means no matches and malformed locations are a contract error', async (t) => {
    const f = await setup(t, { noResult: true });
    const r = await f.adapter.locations('references', 'main.ts', 0, deadline());
    assert.equal(r.items.length, 0);
});
test('LSP malformed location does not masquerade as a zero-match response', async (t) => {
    const f = await setup(t, { badLocation: true });
    await assert.rejects(() => f.adapter.locations('definition', 'main.ts', 0, deadline()), e => e.code === 'ADAPTER_CONTRACT');
});
test('LSP definition query requires advertised capability', async (t) => {
    const f = await setup(t, { noDefinition: true });
    await assert.rejects(() => f.adapter.locations('definition', 'main.ts', 0, deadline()), e => e.code === 'CAPABILITY_UNAVAILABLE');
});
test('LSP duplicate references do not multiply equivalent source locations', async (t) => {
    const f = await setup(t, { duplicate: true });
    const r = await f.adapter.locations('references', 'main.ts', 0, deadline());
    assert.equal(r.items.length, 1);
});
test('LSP readable location clamps oversized characters to content, not CRLF', async (t) => {
    const f = await setup(t, { link: true, clamp: true });
    const r = await f.adapter.locations('definition', 'main.ts', 0, deadline());
    assert.equal(r.items[0].endByte, 8);
    assert.equal(r.items[0].positionClamped, true);
});
test('LocationLink selection must stay inside its target range', async (t) => {
    const f = await setup(t, { link: true, invalidSelection: true }, 'nameTextLONG\n');
    await assert.rejects(() => f.adapter.locations('definition', 'main.ts', 0, deadline()), e => e.code === 'ADAPTER_CONTRACT');
});
for (const [label, spec, code] of [
    ['CRLF terminator', { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 9 } } }, 'INVALID_RANGE'],
    ['reversed range', { range: { start: { line: 0, character: 4 }, end: { line: 0, character: 0 } } }, 'INVALID_RANGE'],
    ['stale version', { version: 2 }, 'SNAPSHOT_STALE'],
    ['annotation', { annotation: true }, 'CAPABILITY_UNAVAILABLE'],
    ['resource delete', { resource: true }, 'CAPABILITY_UNAVAILABLE'],
    ['overlap', { overlap: true }, 'OVERLAPPING_EDITS'],
    ['both edit formats', { both: true }, 'ADAPTER_CONTRACT'],
    ['unpaired surrogate', { invalidText: true }, 'INVALID_EDIT'],
    ['external target', { external: true }, 'SCOPE_VIOLATION'],
    ['prepareRename refusal', { prepare: true, refusePrepare: true }, 'RENAME_UNAVAILABLE'],
]) {
    test(`LSP rename rejects ${label} and leaves source unchanged`, async (t) => {
        const f = await setup(t, spec);
        await assert.rejects(() => f.adapter.rename('main.ts', 0, 'updated', deadline()), e => e.code === code);
        assert.equal(f.read('main.ts'), 'nameText\r\nsecond\r\n');
    });
}
test('LSP rename validates prepareRename and deduplicates matching replacements', async (t) => {
    const f = await setup(t, { prepare: true, duplicate: true });
    const edits = await f.adapter.rename('main.ts', 0, 'updated', deadline());
    assert.equal(edits.length, 1);
    assert.equal(edits[0].expectedHash, f.ws.file(f.s, 'main.ts').hash);
});
test('LSP refuses a position inside a UTF-16 surrogate pair', async (t) => {
    const f = await setup(t, { range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } } }, '😀text\n');
    await assert.rejects(() => f.adapter.rename('main.ts', 0, 'updated', deadline()), e => e.code === 'INVALID_RANGE');
});
test('CR-only, CRLF and LF positions use the same exact UTF-8 source model', async (t) => {
    const ti = new TextIndex('😀\rx\r\ny\n');
    assert.deepEqual(ti.lineStarts, [0, 3, 6, 8]);
    assert.equal(ti.line(ti.utf16(5)), 2);
    const f = await setup(t, { range: { start: { line: 1, character: 0 }, end: { line: 1, character: 4 } } }, '😀\rname\r');
    const edits = await f.adapter.rename('main.ts', 5, 'updated', deadline());
    assert.equal(edits[0].startByte, 5);
    assert.equal(edits[0].endByte, 9);
});
test('LSP diagnostic notifications are source-scoped and reject stale versions', async (t) => {
    const f = await setup(t);
    await f.adapter.peer.request('diagnostic-noise', {}, deadline());
    assert.equal(f.adapter.diagnostics.size, 1);
    assert.equal([...f.adapter.diagnostics.values()][0][0].message, 'current');
});
test('LSP startup rejects unsupported negotiated encodings and closes the peer', async (t) => {
    const f = fixture(t, { 'main.ts': 'hello' }), adapter = new LspAdapter(f.ws);
    f.cleanup(() => adapter.close());
    await assert.rejects(() => adapter.start(process.execPath, [peerFile, '{"encoding":"utf-8"}'], f.snapshot(), join(f.ws.store.directory, 'changes', 'bad', 'tree'), deadline()), e => e.code === 'UNSUPPORTED_ENCODING');
    await assert.rejects(() => adapter.peer.request('opened', {}, deadline()), e => e.code === 'PEER_CLOSED');
});
test('LSP advertises JSX documents using their correct language identifiers', async (t) => {
    const f = fixture(t, { 'view.tsx': 'export const v=<div/>;', 'view.jsx': 'export const v=<div/>;' }), adapter = new LspAdapter(f.ws);
    f.cleanup(() => adapter.close());
    await adapter.start(process.execPath, [peerFile, '{}'], f.snapshot(), join(f.ws.store.directory, 'changes', 'jsx', 'tree'), deadline());
    const opened = await adapter.peer.request('opened', {}, deadline());
    assert.deepEqual(opened.map(d => d.languageId).sort(), ['javascriptreact', 'typescriptreact']);
    await adapter.shutdown(deadline());
});
