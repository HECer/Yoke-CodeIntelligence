import test from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { mkdirSync, symlinkSync, writeFileSync, unlinkSync } from 'node:fs';
import { TextIndex, CursorCodec, Deadline, stable, hash, isTestPath } from '../dist/core/util.js';
import { normalizePath, safePath, glob, secretLike } from '../dist/core/paths.js';
import { Store } from '../dist/store/store.js';
import { Workspace } from '../dist/core/workspace.js';
import { measure, fit, artifactPage } from '../dist/core/budget.js';
import { validate } from '../dist/protocol/validate.js';
import { TOOLS } from '../dist/protocol/tools.js';
import { fixture } from './helpers.mjs';
test('UTF-8 offsets survive emoji, umlauts and CRLF', () => {
    const ti = new TextIndex('A😀é\r\nβ');
    assert.equal(ti.byte(1), 1);
    assert.equal(ti.byte(3), 5);
    assert.equal(ti.byte(4), 7);
    assert.equal(ti.utf16(5), 3);
    assert.equal(ti.line(6), 2);
    assert.equal(ti.bytes.length, 11);
    assert.throws(() => ti.utf16(2), /boundary/);
    assert.equal(ti.isBoundary(4), false);
    assert.equal(ti.isBoundary(11), true);
    assert.equal(ti.isBoundary(-1), false);
});
test('canonical hashes are independent of object insertion order', () => { assert.equal(stable({ b: 2, a: 1 }), stable({ a: 1, b: 2 })); assert.equal(hash('abc').length, 64); });
test('signed cursors reject tampering and a different workspace key', () => {
    const a = new CursorCodec(Buffer.alloc(32, 1)), b = new CursorCodec(Buffer.alloc(32, 2));
    const cursor = a.encode({ offset: 8, snapshotId: 'test' });
    assert.equal(a.decode(cursor).offset, 8);
    assert.throws(() => a.decode(cursor.replace(/^./, c => c === 'a' ? 'b' : 'a')));
    assert.throws(() => b.decode(cursor));
    assert.throws(() => a.decode('x.y.z'));
    assert.throws(() => a.decode('a'.repeat(5000)));
});
test('deadline cancellation is distinct from expiration', () => {
    const ac = new AbortController();
    const d = new Deadline(10000, ac.signal);
    ac.abort();
    assert.throws(() => d.check(), e => e.code === 'CANCELLED');
    assert.throws(() => new Deadline(-1).check(), e => e.code === 'DEADLINE_EXCEEDED');
});
for (const path of ['../x', 'a/../../x', '/tmp/x', 'C:\\secret', '\\\\server\\x', 'a\u0000b', 'a//b', 'a/CON.txt', 'a/nul', 'a/trailing.']) {
    test(`path policy refuses ${JSON.stringify(path)}`, () => assert.throws(() => normalizePath(path)));
}
test('safe Windows-style relative paths normalize consistently', () => assert.equal(normalizePath('src\\hello.ts'), 'src/hello.ts'));
test('glob recursion differs from a single path-segment star', () => { assert(glob('**/*.ts').test('a.ts')); assert(glob('src/**').test('src/a/b.ts')); assert(!glob('src/*.ts').test('src/a/b.ts')); });
test('test-path detector recognizes root Python tests and Go/TS conventions', () => {
    for (const p of ['test_api.py', 'tests/x.ts', 'a.test.ts', 'a_test.go', '__tests__/a.ts'])
        assert(isTestPath(p), p);
    assert(!isTestPath('src/testimonial.ts'));
});
test('protected files, binary data and obvious credentials do not enter a snapshot', t => {
    const f = fixture(t, { '.env': 'SECRET=x', 'key.pem': 'x', 'src/a.ts': 'export const safe = 1;', 'raw.bin': Buffer.from([0, 1]), 'token.txt': 'gh' + 'p_' + 'A'.repeat(36) });
    const s = f.snapshot();
    assert.deepEqual(s.files.map(x => x.path), ['src/a.ts']);
    assert(s.omissions.some(x => x.reason === 'potential_secret'));
    assert(s.omissions.some(x => x.reason === 'binary'));
});
test('Git ignored and generated state files never self-index', t => {
    const f = fixture(t, { '.gitignore': 'private/\n', 'a.ts': 'export const a=1', 'private/notes.txt': 'private', 'node_modules/x.ts': 'export const secret=1' });
    const a = f.snapshot(), b = f.snapshot();
    assert.equal(a.id, b.id);
    assert(!b.files.some(x => x.path.includes('.code-intelligence') || x.path.startsWith('private/')));
});
test('non-Git fallback conservatively respects positive root and nested ignores', t => {
    const f = fixture(t, { '.gitignore': 'private/\n*.secret\n!ok.secret\n', 'private/a.txt': 'x', 'a.secret': 'x', 'ok.secret': 'x', 'src/.gitignore': 'hidden.txt\n', 'src/hidden.txt': 'x', 'src/a.ts': 'export const a=1' }, { git: false });
    const paths = f.snapshot().files.map(x => x.path);
    assert(!paths.includes('private/a.txt'));
    assert(!paths.includes('src/hidden.txt'));
    assert(!paths.includes('ok.secret'));
    assert(paths.includes('src/a.ts'));
});
test('snapshots retain old bytes after edits, creation and deletion', t => {
    const f = fixture(t, { 'a.ts': 'export const a=1' });
    const old = f.snapshot();
    f.write('a.ts', 'export const a=2');
    f.write('b.ts', 'export const b=3');
    const next = f.snapshot();
    assert.notEqual(old.id, next.id);
    assert.equal(f.ws.read(old, 'a.ts').toString(), 'export const a=1');
    unlinkSync(join(f.root, 'a.ts'));
    assert.equal(f.snapshot().files.length, 1);
});
test('request budgets do not change semantic snapshot identities', t => {
    const f = fixture(t, { 'a.ts': 'export const a=1' });
    const id = f.snapshot().id;
    const other = new Workspace(f.root, { timeoutMs: 10000, maxResultBytes: 2048 });
    try {
        assert.equal(other.capture().id, id);
    }
    finally {
        other.close();
    }
});
test('snapshots are root/workspace isolated', t => { const a = fixture(t, { 'a.ts': 'x' }), b = fixture(t, { 'a.ts': 'x' }); const s = a.snapshot(); assert.notEqual(s.workspaceId, b.snapshot().workspaceId); assert.throws(() => b.ws.store.snapshot(s.id)); });
test('parent, leaf and dangling symbolic links are denied', { skip: process.platform === 'win32' ? 'Creating symlinks requires elevated Windows privilege' : false }, t => {
    const f = fixture(t, { 'a.ts': 'x' });
    symlinkSync(f.root, join(f.root, 'link'), 'dir');
    symlinkSync(join(f.root, 'missing'), join(f.root, 'dangling'));
    assert.throws(() => safePath(f.root, 'link/a.ts'));
    assert.throws(() => safePath(f.root, 'dangling'));
    assert(!f.snapshot().files.some(x => x.path === 'dangling'));
});
test('SQLite state and CAS path symbolic links are refused', { skip: process.platform === 'win32' }, t => {
    const f = fixture(t, { 'a.ts': 'x' });
    const digest = hash('payload');
    const prefix = join(f.ws.store.directory, 'blobs', digest.slice(0, 2));
    symlinkSync(f.root, prefix, 'dir');
    assert.throws(() => f.ws.store.putBlob('payload'));
    const bad = join(f.root, 'bad-state');
    mkdirSync(bad);
    symlinkSync(join(f.root, 'a.ts'), join(bad, 'index.sqlite'));
    assert.throws(() => new Store(bad));
});
test('CAS integrity failure is visible, not silently reused', t => { const f = fixture(t, { 'a.ts': 'x' }); const s = f.snapshot(); writeFileSync(f.ws.store.blobPath(s.files[0].hash), 'tampered'); assert.throws(() => f.ws.read(s, 'a.ts'), e => e.code === 'CORRUPT_BLOB'); });
test('schema validator rejects undeclared authority and invalid numbers', () => {
    const schema = TOOLS.find(t => t.name === 'ci_find').inputSchema;
    assert.throws(() => validate(schema, { query: 'x', allowWrites: true }));
    assert.throws(() => validate(schema, { query: 'x', limit: NaN }));
    assert.throws(() => validate(schema, { query: 'x', limit: 201 }));
});
function envelope(data, snapshotId = null) { return { schemaVersion: 1, requestId: 'test', snapshotId, status: 'ok', data, warnings: [], metrics: { durationMs: 1, serializedBytes: 0, estimatedTokens: 0, tokenCount: 'estimated', tokenMethod: 'utf8_bytes_div_3', modelCalls: 0, localOnly: true } }; }
test('full serialized envelope respects byte bound including its metrics', t => { const f = fixture(t, {}); const out = fit(envelope({ items: Array.from({ length: 100 }, (_, i) => ({ id: i, text: '😀'.repeat(100) })) }), f.ws.store, 1800); assert(Buffer.byteLength(JSON.stringify(out)) <= 1800); assert.equal(out.metrics.serializedBytes, Buffer.byteLength(JSON.stringify(out))); assert(out.artifactId); assert.equal(out.status, 'partial'); });
test('artifact pagination recovers original JSON without splitting Unicode', t => {
    const f = fixture(t, {});
    const data = { source: '😀é漢'.repeat(400) };
    const result = fit(envelope(data), f.ws.store, 1600);
    let cursor = result.nextCursor, full = '';
    while (cursor) {
        const page = artifactPage(f.ws.store, cursor, 53);
        full += page.data.text;
        cursor = page.nextCursor;
    }
    assert.deepEqual(JSON.parse(full), data);
});
test('tiny budgets and forged artifact cursors fail explicitly', t => { const f = fixture(t, {}); assert.throws(() => fit(envelope({}), f.ws.store, 100)); assert.throws(() => artifactPage(f.ws.store, 'bogus.cursor')); });
test('derived source cannot exceed the configured total capacity', t => { const f = fixture(t, { 'a.ts': 'x' }, { config: { maxSnapshotBytes: 10 } }); assert.throws(() => f.ws.derive(f.snapshot(), [{ path: 'b.ts', content: Buffer.from('x'.repeat(11)) }]), e => e.code === 'CAPACITY_EXCEEDED'); });
test('repository configuration cannot select an executable interpreter', t => {
    const f = fixture(t, { 'a.py': 'def safe(): return 1' });
    f.write('code-intelligence.json', JSON.stringify({ schemaVersion: 1, python: './untrusted-program' }));
    assert.throws(() => new Workspace(f.root), e => e.code === 'UNTRUSTED_EXECUTABLE');
});
test('explicit host interpreter selection overrides untrusted repository setting', t => {
    const f = fixture(t, {});
    f.write('code-intelligence.json', JSON.stringify({ schemaVersion: 1, python: './untrusted-program' }));
    const command = process.platform === 'win32' ? 'python' : 'python3';
    const trusted = new Workspace(f.root, { python: command });
    try {
        assert.equal(trusted.config.python, command);
    }
    finally {
        trusted.close();
    }
});
