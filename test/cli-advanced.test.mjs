import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { fixture } from './helpers.mjs';
const cli = fileURLToPath(new URL('../dist/cli.js', import.meta.url));
function run(f, args) { const r = spawnSync(process.execPath, [cli, ...args, '--root', f.root, '--timeout', '120000'], { encoding: 'utf8', timeout: 130000 }); assert(!r.error, r.error?.message); return { ...r, data: JSON.parse(r.stdout || r.stderr.split('\n').find(l => l.startsWith('{')) || '{}') }; }
test('advanced CLI embeds source and retrieves the same snapshot through the local vectorizer', t => { const f = fixture(t, { 'a.ts': 'export function calculateInvoice(amount:number){return amount*2}' }); const out = run(f, ['embed']); assert.equal(out.status, 0, out.stderr); assert.equal(out.data.kind, 'statistical_lexical_embedding'); const found = run(f, ['semantic-search', 'calculate invoice', '--model', out.data.model, '--snapshot', out.data.snapshotId]); assert.equal(found.status, 0, found.stderr); assert(found.data.items.length > 0); });
test('advanced CLI solution build produces no source-tree output', t => { const f = fixture(t, { 'tsconfig.json': '{"compilerOptions":{"composite":true,"outDir":"build","declaration":true},"files":["a.ts"]}', 'a.ts': 'export const value:number=4;' }); const out = run(f, ['build']); assert.equal(out.status, 0, JSON.stringify(out.data)); assert.equal(out.data.sourceTreeWritten, false); assert(out.data.outputs.length > 0); });
test('advanced CLI model commands reject absent host permission', t => { const f = fixture(t, { 'a.ts': 'export const v=2' }); const out = run(f, ['embed', '--host-config', 'does-not-exist.json']); assert.notEqual(out.status, 0); assert.match(JSON.stringify(out.data), /PROVIDER_UNTRUSTED/); });
test('advanced CLI rejects misspelled GC application mode instead of deleting', t => { const f = fixture(t, { 'a.ts': 'x' }); const r = run(f, ['gc', '--mode', 'aply']); assert.notEqual(r.status, 0); assert.match(JSON.stringify(r.data), /INVALID_ARGUMENT/); });
test('advanced CLI taint labels untrusted input flow as a nonexhaustive candidate', t => { const f = fixture(t, { 'a.ts': 'declare function eval(x:any):void;function f(req:any){eval(req.query.value)}' }); const out = run(f, ['taint']); assert.equal(out.status, 0, JSON.stringify(out.data)); assert.equal(out.data.exhaustive, false); assert.equal(out.data.findings.length, 1); });
test('real editor CLI handshake, authentication, unsaved lookup and unchanged disk source', async (t) => {
    const f = fixture(t, { 'a.ts': 'export const saved=1;' });
    const child = spawn(process.execPath, [cli, 'editor', '--root', f.root, '--timeout', '120000'], { stdio: ['ignore', 'pipe', 'pipe'] });
    let raw = '', errors = '';
    child.stderr.on('data', b => errors += b);
    t.after(() => child.kill());
    const connection = await new Promise((resolve, reject) => { const timer = setTimeout(() => reject(Error('No editor handshake ' + errors)), 15000); child.once('error', reject); child.once('exit', () => { clearTimeout(timer); reject(Error('Early editor exit ' + errors)); }); child.stdout.on('data', b => { raw += b; try {
        const j = JSON.parse(raw);
        clearTimeout(timer);
        resolve(j);
    }
    catch { } }); });
    assert.equal(new URL(connection.endpoint).hostname, '127.0.0.1');
    const post = async (data) => { const r = await fetch(connection.endpoint, { method: 'POST', headers: { authorization: 'Bearer ' + connection.token }, body: JSON.stringify(data) }); assert.equal(r.status, 200); return r.json(); };
    const opened = await post({ operation: 'open' }), sessionId = opened.data.id;
    await post({ operation: 'buffer', sessionId, buffer: { path: 'a.ts', version: 1, text: 'export const unsaved=2;' } });
    const result = await post({ operation: 'tool', sessionId, tool: 'ci_find', arguments: { query: 'unsaved' } });
    assert.equal(result.data.status, 'ok');
    assert(result.data.data.items.length > 0);
    assert.equal(f.read('a.ts'), 'export const saved=1;');
    assert.equal((await fetch(connection.endpoint, { method: 'POST', headers: { authorization: 'Bearer ' + 'é'.repeat(64) }, body: '{}' })).status, 401);
    child.kill();
    await once(child, 'exit');
});
