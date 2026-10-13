import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, writeFileSync, renameSync } from 'node:fs';
import { join } from 'node:path';
import { fixture } from './helpers.mjs';
import { Deadline } from '../dist/core/util.js';
import { collectGarbage, pinSnapshot, unpinSnapshot } from '../dist/store/maintenance.js';
import { Workspace } from '../dist/core/workspace.js';
test('unchanged independently configured TypeScript unit reuses semantic extraction', async (t) => {
    const f = fixture(t, { 'a/tsconfig.json': '{"include":["*.ts"]}', 'a/a.ts': 'export const a=1;', 'b/tsconfig.json': '{"include":["*.ts"]}', 'b/b.ts': 'export const b=1;' });
    await f.indexed();
    f.write('a/a.ts', 'export const a=2;');
    const { snapshot: s } = await f.indexed();
    const row = f.ws.store.db.prepare("SELECT json FROM events WHERE type='semantic-cache' ORDER BY id DESC LIMIT 1").get();
    const stats = JSON.parse(row.json);
    assert.equal(stats.snapshotId, s.id);
    assert.equal(stats.hits, 1);
    assert.equal(stats.misses, 1);
});
test('new resolution candidate invalidates negative and existing semantic caches', async (t) => {
    const f = fixture(t, { 'tsconfig.json': '{"compilerOptions":{"moduleResolution":"Bundler","module":"ESNext"}}', 'a.ts': 'import {value} from "./later"; export const x=value;' });
    let g = (await f.indexed()).graph;
    assert(g.diagnostics.some(d => d.code === 'TS2307'));
    f.write('later.ts', 'export const value=1;');
    g = (await f.indexed()).graph;
    assert(!g.diagnostics.some(d => d.code === 'TS2307'));
    assert.equal(f.engine.indexer.typescript(f.ws.store.snapshots(1)[0]).cacheStats.hits, 0);
});
test('GC defaults to dry-run and preserves explicitly pinned snapshots', t => {
    const f = fixture(t, { 'a.ts': 'a' });
    const a = f.snapshot();
    pinSnapshot(f.ws.store, a.id, 'keep', 'user checkpoint');
    f.write('a.ts', 'b');
    const b = f.snapshot();
    f.write('a.ts', 'c');
    const c = f.snapshot();
    const report = collectGarbage(f.ws.store, { keepSnapshots: 1, minimumAgeMs: 0, cacheAgeMs: 0 });
    assert.equal(report.applied, false);
    assert(f.ws.store.snapshot(b.id));
    collectGarbage(f.ws.store, { keepSnapshots: 1, minimumAgeMs: 0, cacheAgeMs: 0, apply: true });
    assert(f.ws.store.snapshot(a.id));
    assert(f.ws.store.snapshot(c.id));
    unpinSnapshot(f.ws.store, 'keep');
    const next = collectGarbage(f.ws.store, { keepSnapshots: 1, minimumAgeMs: 0, cacheAgeMs: 0, apply: true });
    assert(next.retiredSnapshots.includes(a.id));
});
test('GC refuses a second live connection and succeeds after close', t => {
    const f = fixture(t, { 'a.ts': 'a' });
    f.snapshot();
    const other = new Workspace(f.root);
    try {
        assert.throws(() => collectGarbage(f.ws.store, { apply: true }), /other workspace/);
    }
    finally {
        other.close();
    }
    assert(collectGarbage(f.ws.store, { apply: true }).applied);
});
test('quarantined source is recoverable after interrupted maintenance', t => {
    const f = fixture(t, { 'a.ts': 'a' }), s = f.snapshot(), digest = s.files[0].hash;
    const path = f.ws.store.blobPath(digest);
    renameSync(path, join(f.ws.store.directory, 'trash', digest));
    assert(!existsSync(path));
    assert.equal(f.ws.read(s, 'a.ts').toString(), 'a');
    assert(existsSync(path));
});
test('GC retains source blobs referenced by memories and prepared changes', t => {
    const f = fixture(t, { 'a.ts': 'a' }), s = f.snapshot();
    f.engine.knowledge.remember(s, { kind: 'confirmed', text: 'A fact', paths: ['a.ts'], actor: 'test' });
    const plan = f.engine.changes.prepare(s, [{ kind: 'replace', path: 'a.ts', expectedHash: s.files[0].hash, startByte: 0, endByte: 1, text: 'b' }], new Deadline(5000));
    f.write('a.ts', 'c');
    f.snapshot();
    collectGarbage(f.ws.store, { apply: true, keepSnapshots: 1, minimumAgeMs: 0, cacheAgeMs: 0 });
    assert(f.ws.store.snapshot(plan.resultSnapshotId));
    assert.equal(f.ws.store.blob(s.files[0].hash).toString(), 'a');
});
