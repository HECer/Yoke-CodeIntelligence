import test from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { spawnSync, execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { readFileSync, unlinkSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { fixture } from './helpers.mjs';
import { serveMcp } from '../dist/protocol/mcp.js';
import { serveViewer } from '../dist/protocol/viewer.js';
import { Deadline } from '../dist/core/util.js';
import { runProcess } from '../dist/core/process.js';
const cli = fileURLToPath(new URL('../dist/cli.js', import.meta.url));
function mcp(t, engine) {
    const input = new PassThrough(), output = new PassThrough();
    const messages = [], waiting = new Map();
    let buffer = '';
    output.on('data', c => {
        buffer += c;
        while (buffer.includes('\n')) {
            const i = buffer.indexOf('\n');
            const m = JSON.parse(buffer.slice(0, i));
            buffer = buffer.slice(i + 1);
            messages.push(m);
            const resolve = waiting.get(m.id);
            if (resolve) {
                waiting.delete(m.id);
                resolve(m);
            }
        }
    });
    const stop = serveMcp(engine, input, output);
    t.after(() => { stop(); input.destroy(); output.destroy(); });
    let next = 1;
    return { input, messages, request(method, params) { const id = next++; return new Promise((resolve, reject) => { const timer = setTimeout(() => reject(new Error('MCP test timed out')), 10000); waiting.set(id, m => { clearTimeout(timer); resolve(m); }); input.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'); }); } };
}
test('MCP initializes, exposes exactly eight tools and returns one complete content payload', async (t) => { const f = fixture(t, { 'a.ts': 'export const a=1' }), c = mcp(t, f.engine); const init = await c.request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test', version: '1' } }); assert.equal(init.result.serverInfo.name, 'code-intelligence'); const tools = await c.request('tools/list', {}); assert.equal(tools.result.tools.length, 8); const result = await c.request('tools/call', { name: 'ci_find', arguments: { query: 'a' } }); assert.equal(result.result.isError, false); assert.equal(result.result.content.length, 1); assert.equal(result.result.structuredContent, undefined); assert(JSON.parse(result.result.content[0].text).data.items.length > 0); });
test('MCP rejects tools before initialization and reports unknown tools', async (t) => { const f = fixture(t, {}), c = mcp(t, f.engine); assert.equal((await c.request('tools/list', {})).error.code, -32002); await c.request('initialize', {}); const r = await c.request('tools/call', { name: 'ci_shell', arguments: {} }); assert.equal(r.result.isError, true); assert.equal(JSON.parse(r.result.content[0].text).error.code, 'TOOL_NOT_FOUND'); });
test('MCP invalid JSON does not poison the next valid request', async (t) => { const f = fixture(t, {}), c = mcp(t, f.engine); c.input.write('garbage\n'); const r = await c.request('ping', {}); assert.deepEqual(r.result, {}); assert(c.messages.some(m => m.error?.code === -32700)); });
test('MCP cancellation reaches the active request AbortSignal', async (t) => {
    let signalSeen = false;
    const engine = { invoke: async (_tool, _args, signal) => new Promise(resolve => { signal.addEventListener('abort', () => { signalSeen = true; resolve({ status: 'error', error: { code: 'CANCELLED' } }); }, { once: true }); }) };
    const c = mcp(t, engine);
    await c.request('initialize', {});
    const result = c.request('tools/call', { name: 'wait', arguments: {} });
    c.input.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: 2 } }) + '\n');
    assert.equal((await result).result.isError, true);
    assert(signalSeen);
});
test('actual CLI emits parseable JSON with runtime warnings kept on stderr', t => { const f = fixture(t, { 'a.ts': 'export function actual(){return 1}' }); const out = spawnSync(process.execPath, [cli, 'find', 'actual', '--root', f.root], { encoding: 'utf8' }); assert.equal(out.status, 0, out.stderr); const data = JSON.parse(out.stdout); assert(data.data.items.some(x => x.name === 'actual')); assert(!out.stdout.includes('ExperimentalWarning')); });
test('CLI help and invalid tool exit status work without external packages', t => { const f = fixture(t, {}); const help = spawnSync(process.execPath, [cli, '--help'], { encoding: 'utf8' }); assert.equal(help.status, 0); assert(help.stdout.includes('Code Intelligence')); const bad = spawnSync(process.execPath, [cli, 'tool', 'ci_fake', '--root', f.root], { encoding: 'utf8' }); assert.equal(bad.status, 1); assert.equal(JSON.parse(bad.stdout).error.code, 'TOOL_NOT_FOUND'); });
test('CLI canonical graph export is a real, readable graph file', t => { const f = fixture(t, { 'a.ts': 'export const a=1' }); const path = join(f.ws.store.directory, 'export.json'); const run = spawnSync(process.execPath, [cli, 'export', '--root', f.root, '--output', path], { encoding: 'utf8' }); assert.equal(run.status, 0, run.stderr); const graph = JSON.parse(readFileSync(path, 'utf8')); assert(graph.entities.length >= 2); assert(graph.evidence.length >= 1); });
test('deleting a tracked Git file produces a new snapshot rather than perpetual WORKSPACE_BUSY', t => { const f = fixture(t, { 'a.ts': 'export const a=1', 'b.ts': 'export const b=2' }); execFileSync('git', ['-C', f.root, 'add', '.']); const a = f.snapshot(); unlinkSync(join(f.root, 'a.ts')); const b = f.snapshot(); assert.notEqual(a.id, b.id); assert.deepEqual(b.files.map(f => f.path), ['b.ts']); });
test('loopback explorer requires a bearer token and refuses write tools', async (t) => {
    const f = fixture(t, { 'a.ts': 'export const a=1' });
    const viewer = await serveViewer(f.engine);
    t.after(() => viewer.close());
    const url = new URL(viewer.url), token = url.hash.slice(1);
    url.hash = '';
    const base = url.href;
    const html = await fetch(base);
    assert.equal(html.status, 200);
    assert(html.headers.get('content-security-policy').includes("frame-ancestors 'none'"));
    assert((await html.text()).includes('Code Intelligence'));
    const unauthorized = await fetch(base + 'api', { method: 'POST', body: '{}' });
    assert.equal(unauthorized.status, 401);
    const denied = await fetch(base + 'api', { method: 'POST', headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' }, body: JSON.stringify({ tool: 'ci_change_apply', arguments: {} }) });
    assert.equal(denied.status, 403);
    const response = await fetch(base + 'api', { method: 'POST', headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json' }, body: JSON.stringify({ tool: 'ci_find', arguments: { query: 'a' } }) });
    assert.equal(response.status, 200);
    assert((await response.json()).data.items.length > 0);
});
test('loopback explorer rejects a cross-site Origin even with a valid token', async (t) => { const f = fixture(t, {}); const viewer = await serveViewer(f.engine); t.after(() => viewer.close()); const url = new URL(viewer.url), token = url.hash.slice(1); url.hash = ''; const r = await fetch(url.href + 'api', { method: 'POST', headers: { Authorization: 'Bearer ' + token, Origin: 'https://untrusted.example' }, body: '{}' }); assert.equal(r.status, 403); });
test('bounded subprocess capture never treats truncated logs as complete', async (t) => { const f = fixture(t, {}); const out = await runProcess(process.execPath, ['-e', 'process.stdout.write("x".repeat(100000))'], { cwd: f.root, deadline: new Deadline(10000), maxBytes: 1234 }); assert.equal(out.truncated, true); assert.equal(Buffer.byteLength(out.stdout), 1234); assert.equal(out.exitCode, 0); });
test('subprocess deadlines kill a stalled process instead of waiting indefinitely', async (t) => { const f = fixture(t, {}); await assert.rejects(() => runProcess(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { cwd: f.root, deadline: new Deadline(100), maxBytes: 1000 }), e => e.code === 'DEADLINE_EXCEEDED'); });
test('publisher dry run names the public owner and makes no remote changes', () => {
    const result = spawnSync(process.execPath, ['scripts/publish-github.mjs', '--dry-run'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const data = JSON.parse(result.stdout);
    assert.equal(data.repository, 'HECer/Code-Intelligence');
    assert.equal(data.visibility, 'public');
    assert.match(data.tag, /^v\d+\.\d+\.\d+/);
    assert.equal(data.tag, `v${data.version}`);
    assert.equal(data.changesMade, false);
});
