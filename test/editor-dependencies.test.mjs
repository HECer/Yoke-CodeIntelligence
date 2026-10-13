import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, named } from './helpers.mjs';
import { EditorBuffers } from '../dist/core/editor.js';
import { Deadline, hash } from '../dist/core/util.js';
import { serveEditor } from '../dist/protocol/editor-server.js';
import { buildTypeScript } from '../dist/analysis/build.js';
test('installed declarations are immutable, indexed and resolve package calls', async (t) => {
    const f = fixture(t, { 'package.json': '{"dependencies":{"money":"1.0.0"}}', 'index.ts': 'import {cash} from "money"; export const total = cash(12);', 'tsconfig.json': '{"compilerOptions":{"strict":true,"module":"ESNext","moduleResolution":"Bundler"}}', 'node_modules/money/package.json': '{"name":"money","version":"1.0.0","types":"index.d.ts"}', 'node_modules/money/index.d.ts': 'export declare function cash(n:number):number;', 'node_modules/money/run.js': 'throw new Error("must not execute")' });
    const { snapshot: s, graph: g } = await f.indexed();
    assert(s.files.find(x => x.path === 'node_modules/money/index.d.ts')?.readOnly);
    assert(!s.files.some(x => x.path.endsWith('/run.js')));
    const dep = named(g, 'cash', 'node_modules/money/index.d.ts');
    assert(g.observations.some(o => o.target === dep.id && o.relation === 'calls' && o.resolution === 'exact_binding'));
    assert.throws(() => f.engine.changes.prepare(s, [{ kind: 'delete', path: dep.path, expectedHash: f.ws.file(s, dep.path).hash }], new Deadline(1000)), /Protected/);
    f.write('node_modules/money/index.d.ts', 'export declare function cash(n:string):string;');
    const next = await f.indexed();
    assert.notEqual(s.id, next.snapshot.id);
    assert(next.graph.diagnostics.some(d => d.code === 'TS2345'));
    assert.equal(f.ws.read(s, dep.path).toString(), 'export declare function cash(n:number):number;');
});
test('ambient @types are discovered without reading live compiler filesystem', async (t) => {
    const f = fixture(t, { 'a.ts': 'export const ok: number = injected;', 'node_modules/@types/custom/package.json': '{"name":"@types/custom","version":"1.0.0","types":"index.d.ts"}', 'node_modules/@types/custom/index.d.ts': 'declare const injected: number;' });
    const { graph } = await f.indexed();
    assert(!graph.diagnostics.some(d => d.message.includes("Cannot find name 'injected'")));
});
test('dependency capture can be disabled explicitly', t => {
    const f = fixture(t, { 'a.ts': 'let x=1', 'node_modules/a/index.d.ts': 'export const x:number;' }, { config: { dependencyMode: 'none' } });
    assert(!f.snapshot().files.some(x => x.readOnly));
});
test('metadata capture reuses unchanged files but cannot authorize source writes', t => {
    const f = fixture(t, { 'a.ts': 'export const x=1' });
    f.snapshot();
    const s = f.ws.capture(new Deadline(5000), { mode: 'metadata' });
    assert.equal(s.captureConsistency, 'metadata-reconciled');
    const event = f.ws.store.db.prepare("SELECT json FROM events WHERE type='capture' ORDER BY id DESC LIMIT 1").get();
    assert.equal(JSON.parse(event.json).reusedFiles, 1);
    assert.throws(() => f.ws.assertCurrent(s), /derived snapshot/);
    f.write('a.ts', 'export const x=2');
    assert.notEqual(f.ws.capture(new Deadline(5000), { mode: 'metadata' }).id, s.id);
});
test('editor overlay analyzes unsaved source without changing disk', async (t) => {
    const f = fixture(t, { 'a.ts': 'export const saved=1;' }), editor = new EditorBuffers(f.ws), session = editor.create();
    editor.update(session.id, { path: 'a.ts', version: 1, text: 'export const unsaved=2;' });
    const s = editor.capture(session.id);
    assert.equal(s.kind, 'overlay');
    assert.equal(f.read('a.ts'), 'export const saved=1;');
    await f.engine.indexer.ensure(s, new Deadline(120000));
    assert(named(f.ws.store.graph(s.id), 'unsaved'));
    assert.equal(s.overlay.versions['a.ts'], 1);
    assert.throws(() => f.ws.assertCurrent(s), /derived snapshot/);
});
test('editor versions reject stale writes and allow exact retransmission', t => {
    const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
    e.update(s.id, { path: 'a.ts', version: 5, text: 'b' });
    assert.equal(e.update(s.id, { path: 'a.ts', version: 5, text: 'b' }).idempotent, true);
    for (const v of [0, 4, 5])
        assert.throws(() => e.update(s.id, { path: 'a.ts', version: v, text: 'wrong' }), /version/);
    assert.throws(() => e.update(s.id, { path: 'a.ts', version: 6, text: 'c', expectedVersion: 4 }), /version/);
});
test('editor refuses independent disk mutation and preserves both versions', t => {
    const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
    e.update(s.id, { path: 'a.ts', version: 1, text: 'b' });
    f.write('a.ts', 'third');
    assert.throws(() => e.capture(s.id), /independently/);
    assert.equal(f.read('a.ts'), 'third');
    assert.equal(f.ws.store.blob(e.session(s.id).buffers['a.ts'].hash).toString(), 'b');
});
test('editor saved-buffer baseline advances for subsequent unsaved changes', t => {
    const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
    e.update(s.id, { path: 'a.ts', version: 1, text: 'b' });
    f.write('a.ts', 'b');
    e.capture(s.id);
    e.update(s.id, { path: 'a.ts', version: 2, text: 'c' });
    assert.equal(f.ws.read(e.capture(s.id), 'a.ts').toString(), 'c');
});
test('new and closed buffers are separate from saved disk source', t => {
    const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
    e.update(s.id, { path: 'new.ts', version: 1, text: 'new' });
    assert(e.capture(s.id).files.some(x => x.path === 'new.ts'));
    e.closeFile(s.id, 'new.ts', 1);
    assert(!e.capture(s.id).files.some(x => x.path === 'new.ts'));
    e.close(s.id);
    assert.throws(() => e.session(s.id), /missing/);
});
for (const path of ['../escape.ts', '.env', 'node_modules/p/a.d.ts', '.code-intelligence/config', '/absolute.ts'])
    test('editor rejects protected path ' + path, t => {
        const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
        assert.throws(() => e.update(s.id, { path, version: 1, text: 'x' }));
    });
test('editor rejects secrets, unpaired surrogates and versioned close conflicts', t => {
    const f = fixture(t, { 'a.ts': 'a' }), e = new EditorBuffers(f.ws), s = e.create();
    for (const text of ['\ud800', '\0', '-----BEGIN PRIVATE KEY-----'])
        assert.throws(() => e.update(s.id, { path: 'a.ts', version: 1, text }));
    e.update(s.id, { path: 'a.ts', version: 3, text: 'x' });
    assert.throws(() => e.closeFile(s.id, 'a.ts', 2), /newer/);
});
test('real authenticated editor server opens, updates and captures buffers', async (t) => {
    const f = fixture(t, { 'a.ts': 'export const a=1;' }), server = await serveEditor(f.engine);
    t.after(server.close);
    assert.equal((await fetch(server.endpoint, { method: 'POST', body: '{}' })).status, 401);
    const req = async (value) => { const r = await fetch(server.endpoint, { method: 'POST', headers: { authorization: 'Bearer ' + server.token }, body: JSON.stringify(value) }); return { status: r.status, ...await r.json() }; };
    const open = await req({ operation: 'open' });
    const sessionId = open.data.id;
    assert(open.ok);
    assert((await req({ operation: 'buffer', sessionId, buffer: { path: 'a.ts', version: 1, text: 'export const live=2;' } })).ok);
    const capture = await req({ operation: 'capture', sessionId });
    assert.equal(capture.data.kind, 'overlay');
    const find = await req({ operation: 'tool', sessionId, tool: 'ci_find', arguments: { query: 'live' } });
    assert(find.data.data.items.some(x => x.name === 'live'));
    assert.equal((await req({ operation: 'tool', sessionId, tool: 'ci_change_apply', arguments: {} })).status, 400);
});
test('tsc solution builder generates referenced declarations entirely in memory', t => {
    const f = fixture(t, { 'tsconfig.json': '{"files":[],"references":[{"path":"./core"},{"path":"./app"}]}', 'core/tsconfig.json': '{"compilerOptions":{"composite":true,"outDir":"lib","strict":true},"include":["src"]}', 'core/src/index.ts': 'export const money = (n:number):number => n*2;', 'app/tsconfig.json': '{"compilerOptions":{"composite":true,"outDir":"lib","strict":true},"references":[{"path":"../core"}],"include":["src"]}', 'app/src/index.ts': 'import {money} from "../../core/src/index"; export const result = money(2);' });
    const s = f.snapshot(), r = buildTypeScript(f.ws, s, new Deadline(120000));
    assert.equal(r.status, 'passed', JSON.stringify(r.diagnostics));
    assert(r.outputs.some(x => x.path.endsWith('.d.ts')));
    assert.equal(r.sourceTreeWritten, false);
    assert.equal(s.id, f.snapshot().id);
});
test('tsc build reports type failure rather than marking generated output verified', t => {
    const f = fixture(t, { 'tsconfig.json': '{"compilerOptions":{"outDir":"lib","strict":true}}', 'a.ts': 'export const n:number="bad";' });
    const r = buildTypeScript(f.ws, f.snapshot(), new Deadline(120000));
    assert.equal(r.status, 'failed');
    assert(r.diagnostics.some(d => d.code === 'TS2322'));
});
test('tsc build rejects output escape and never writes outside virtual root', t => {
    const f = fixture(t, { 'tsconfig.json': '{"compilerOptions":{"outDir":"../../escaped-build"}}', 'a.ts': 'export const n=1;' });
    const r = buildTypeScript(f.ws, f.snapshot(), new Deadline(120000));
    assert.equal(r.status, 'failed');
    assert(r.diagnostics.some(d => d.message.includes('leave')));
    assert.equal(r.outputs.length, 0);
});
