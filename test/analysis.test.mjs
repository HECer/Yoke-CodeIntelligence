import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fixture, named, okInvoke } from './helpers.mjs';
import { Deadline } from '../dist/core/util.js';
import { graphDiff, analyzeGraph } from '../dist/analysis/analytics.js';
const sample = {
    'math.ts': 'export function add(a:number,b:number){return a+b}\nexport class Box { value=4; get(){return this.value} }\n',
    'main.ts': "import {add,Box} from './math';\nexport const amount=add(1,2);\nexport const label='😀é';\nexport const box=new Box();\nexport const current=box.value;\n",
    'README.md': '# Billing\nThe invoice subsystem calculates totals.\n# WHY: deterministic arithmetic\nSee [implementation](math.ts).\n'
};
test('native TypeScript graph keeps imports, calls and references distinct', async (t) => {
    const f = fixture(t, sample);
    const { s: unused, snapshot: s, graph: g } = await f.indexed();
    const add = named(g, 'add', 'math.ts');
    const calls = g.observations.filter(e => e.relation === 'calls' && e.target === add.id);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].resolution, 'exact_binding');
    assert(g.observations.some(e => e.relation === 'references' && e.target === add.id));
    assert(g.observations.some(e => e.relation === 'imports'));
    for (const edge of g.observations) {
        assert(g.entities.some(n => n.id === edge.source));
        for (const id of edge.evidenceIds) {
            const evidence = g.evidence.find(x => x.id === id);
            assert(evidence);
            assert.equal(f.ws.file(s, evidence.path).hash, evidence.contentHash);
        }
    }
});
test('non-call property reads remain references', async (t) => { const f = fixture(t, sample); const { graph: g } = await f.indexed(); const value = named(g, 'value', 'math.ts'); assert(g.observations.some(e => e.relation === 'references' && e.target === value.id && g.entities.find(n => n.id === e.source)?.path === 'main.ts')); });
test('same-named functions in different files never collapse by name', async (t) => {
    const f = fixture(t, { 'a.ts': 'export function same(){return 1}', 'b.ts': 'export function same(){return 2}', 'main.ts': "import {same as a} from './a';import {same as b} from './b';export const result=a()+b();" });
    const { graph: g } = await f.indexed();
    const a = named(g, 'same', 'a.ts'), b = named(g, 'same', 'b.ts');
    assert.notEqual(a.id, b.id);
    assert(g.observations.some(e => e.relation === 'calls' && e.target === a.id));
    assert(g.observations.some(e => e.relation === 'calls' && e.target === b.id));
});
test('interface dispatch is a possible dispatch, not a runtime observation', async (t) => { const f = fixture(t, { 'a.ts': 'interface Worker { run():number }\nexport function go(w:Worker){return w.run()}' }); const { graph: g } = await f.indexed(); assert(g.observations.some(e => e.relation === 'calls' && e.resolution === 'possible_dispatch')); assert(!g.observations.some(e => e.resolution === 'observed_execution')); });
test('unresolved external calls stay explicit with null targets', async (t) => { const f = fixture(t, { 'a.ts': 'export function run(){ return unknownRuntime(1) }' }); const { graph: g } = await f.indexed(); assert(g.observations.some(e => e.relation === 'calls' && e.target === null && e.resolution === 'unresolved')); assert(g.coverage.unresolved > 0); });
test('compiler syntax errors appear in coverage and diagnostics', async (t) => { const f = fixture(t, { 'broken.ts': 'export function bad( { !' }); const { graph: g } = await f.indexed(); assert(g.diagnostics.length > 0); assert(g.coverage.diagnostics > 0); assert.equal(g.coverage.exhaustive, false); });
test('renaming an export invalidates unchanged consumer resolution', async (t) => {
    const f = fixture(t, { 'a.ts': 'export function old(){return 1}', 'b.ts': "import {old} from './a';export const v=old();" });
    const first = await f.indexed();
    f.write('a.ts', 'export function newer(){return 1}');
    const second = await f.indexed();
    assert.notEqual(first.snapshot.id, second.snapshot.id);
    assert(second.graph.diagnostics.some(d => d.path === 'b.ts'));
    assert(!second.graph.entities.some(e => e.name === 'old' && e.path === 'a.ts'));
});
test('source reads are immutable and UTF-8 byte-accurate', async (t) => {
    const f = fixture(t, sample);
    const { snapshot: s, graph: g } = await f.indexed();
    const n = named(g, 'label');
    const r = await okInvoke(f.engine, 'ci_read', { snapshotId: s.id, entityId: n.id });
    assert(r.data.text.includes('😀é'));
    assert.equal(Buffer.byteLength(r.data.text), r.data.endByte - r.data.startByte);
    f.write('main.ts', 'changed');
    assert.equal((await okInvoke(f.engine, 'ci_read', { snapshotId: s.id, entityId: n.id })).data.text, r.data.text);
});
test('mixed line/byte/entity read arguments are rejected', async (t) => { const f = fixture(t, sample); const { snapshot: s, graph: g } = await f.indexed(); assert.equal((await f.engine.invoke('ci_read', { snapshotId: s.id, path: 'main.ts', startByte: 0, startLine: 1 })).error.code, 'INVALID_RANGE'); assert.equal((await f.engine.invoke('ci_read', { snapshotId: s.id, entityId: named(g, 'add').id, path: 'main.ts' })).status, 'error'); });
test('literal text search preserves offsets after Unicode case-expansion characters', async (t) => { const f = fixture(t, { 'a.txt': 'İ😀 before target after\n' }); const { snapshot: s } = await f.indexed(); const out = await okInvoke(f.engine, 'ci_find', { snapshotId: s.id, kind: 'text', query: 'target' }); const hit = out.data.items[0]; assert.equal(f.ws.read(s, 'a.txt').subarray(hit.startByte, hit.endByte).toString(), 'target'); });
test('camelCase identifiers are discoverable by a constituent word', async (t) => { const f = fixture(t, { 'a.ts': 'export function invoiceTotal(){ return 0 }' }); const { snapshot: s } = await f.indexed(); const out = await okInvoke(f.engine, 'ci_find', { snapshotId: s.id, query: 'total' }); assert(out.data.items.some(x => x.name === 'invoiceTotal')); });
test('scoped search excludes similarly named neighbouring services', async (t) => { const f = fixture(t, { 'one/a.ts': 'export const shared=1', 'two/a.ts': 'export const shared=2' }); const { snapshot: s } = await f.indexed(); const out = await okInvoke(f.engine, 'ci_find', { snapshotId: s.id, query: 'shared', path: 'one' }); assert.equal(out.data.items.length, 1); assert.equal(out.data.items[0].path, 'one/a.ts'); });
test('trace is directed, bounded and does not invent an ordered path', async (t) => {
    const f = fixture(t, sample);
    const { snapshot: s, graph: g } = await f.indexed();
    const add = named(g, 'add');
    const out = await okInvoke(f.engine, 'ci_trace', { snapshotId: s.id, entityIds: [add.id], direction: 'in', relations: ['calls'], depth: 1 });
    assert.equal(out.data.edges.length, 1);
    assert.equal(out.data.edges[0].target, add.id);
    const zero = await okInvoke(f.engine, 'ci_trace', { snapshotId: s.id, entityIds: [add.id], direction: 'both', depth: 8, maxVisited: 1 });
    assert.equal(zero.data.items.length, 1);
    assert.equal(zero.data.traversalComplete, false);
});
test('read-only plans combine navigation without an extra model call', async (t) => { const f = fixture(t, sample); const { snapshot: s } = await f.indexed(); const out = await okInvoke(f.engine, 'ci_context', { snapshotId: s.id, plan: [{ op: 'find', query: 'add' }, { op: 'filter', field: 'kind', includes: 'function' }, { op: 'read' }, { op: 'select', fields: ['path', 'text'] }] }); assert(out.data.items[0].text.includes('return a+b')); assert.equal(out.metrics.modelCalls, 0); const bad = await f.engine.invoke('ci_context', { snapshotId: s.id, plan: [{ op: 'exec', query: 'rm' }] }); assert.equal(bad.status, 'error'); });
test('Markdown concepts and rationale nodes keep original provenance', async (t) => { const f = fixture(t, sample); const { graph: g } = await f.indexed(); assert(g.entities.some(e => e.path === 'README.md' && e.kind === 'concept')); assert(g.entities.some(e => e.kind === 'rationale')); assert(g.observations.some(e => e.relation === 'documents')); });
test('Python AST works without imports or execution of project code', async (t) => {
    const f = fixture(t, { 'a.py': 'raise RuntimeError("must never execute")\ndef café(x):\n    return x\n\ndef go():\n    return café("😀")\n' });
    const { graph: g, snapshot: s } = await f.indexed();
    assert(named(g, 'café'));
    assert(g.observations.some(e => e.relation === 'calls' && e.resolution === 'heuristic'));
    assert.equal(g.diagnostics.length, 0);
    const e = named(g, 'café');
    assert(f.ws.read(s, e.path).subarray(e.startByte, e.endByte).toString().includes('def café'));
});
test('Python syntax errors are reported without empty-success semantics', async (t) => { const f = fixture(t, { 'a.py': 'def bad(:\n  pass' }); const { graph: g } = await f.indexed(); assert(g.diagnostics.some(x => x.path === 'a.py')); });
test('disabled Python parsing is explicitly reported as unavailable semantics', async (t) => { const f = fixture(t, { 'a.py': 'def f(): pass' }, { config: { pythonEnabled: false } }); const { graph: g } = await f.indexed(); assert(g.coverage.limitations.some(x => x.includes('disabled'))); });
let hasGo = true;
try {
    execFileSync('go', ['version'], { stdio: 'ignore' });
}
catch {
    hasGo = false;
}
test('Go stdlib parser emits real methods, imports and bounded call candidates', { skip: !hasGo, timeout: 180000 }, async (t) => {
    const f = fixture(t, { 'main.go': 'package main\nimport "fmt"\ntype Item struct{Value int}\nfunc (x Item) Read() int { return x.Value }\nfunc add(a,b int) int { return a+b }\nfunc main(){ fmt.Println(add(1,2)) }\n' }, { config: { timeoutMs: 180000 } });
    const { graph: g } = await f.indexed();
    assert(named(g, 'add'));
    assert(named(g, 'Read'));
    assert(g.observations.some(e => e.relation === 'calls'));
    assert(!g.observations.some(e => e.producer.includes('go') && e.resolution === 'exact_binding'));
});
