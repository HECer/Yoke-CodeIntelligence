import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture, named, okInvoke } from './helpers.mjs';
import { Deadline } from '../dist/core/util.js';
import { TypeScriptProject } from '../dist/analysis/typescript.js';
import { diagnosticDelta } from '../dist/changes/verify.js';
const json = JSON.stringify;
const options = { target: 'ES2022', module: 'ESNext', moduleResolution: 'Bundler', strict: true };
const monorepo = {
    'tsconfig.json': json({ files: [], references: [{ path: './packages/core' }, { path: './packages/app' }] }),
    'packages/core/tsconfig.json': json({ compilerOptions: { ...options, composite: true }, include: ['src/**/*.ts'] }),
    'packages/core/src/math.ts': 'export function invoiceTotal(n:number){return n*2}\n',
    'packages/app/tsconfig.json': json({ compilerOptions: { ...options, composite: true, baseUrl: '.', paths: { '@core/*': ['../core/src/*'] } }, references: [{ path: '../core' }], include: ['src/**/*.ts'] }),
    'packages/app/src/main.ts': "import {invoiceTotal} from '@core/math';\nexport const total=invoiceTotal(4);\n"
};
test('tsconfig include/exclude controls roots while excluded source remains text-searchable', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ compilerOptions: options, include: ['src/**/*.ts'], exclude: ['src/skip/**'] }),
        'src/main.ts': 'export const included=1;', 'src/skip/no.ts': 'export const excluded=2;', 'other.ts': 'export const outside=3;' });
    const { snapshot, graph } = await f.indexed();
    assert(named(graph, 'included'));
    assert(!graph.entities.some(e => ['excluded', 'outside'].includes(e.name)));
    assert.deepEqual(graph.coverage.analysisContexts[0].rootFiles, ['src/main.ts']);
    assert.equal((await okInvoke(f.engine, 'ci_find', { snapshotId: snapshot.id, query: 'excluded', kind: 'text' })).data.total, 1);
    assert(graph.coverage.limitations.some(s => s.includes('2 captured files')));
});
test('files list is authoritative rather than every TypeScript file in the snapshot', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ compilerOptions: options, files: ['one.ts'] }), 'one.ts': 'export const one=1', 'two.ts': 'export const two=2' });
    const { graph } = await f.indexed();
    assert(named(graph, 'one'));
    assert(!graph.entities.some(e => e.name === 'two'));
});
test('exclude does not remove a file actually imported by a configured root', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ compilerOptions: options, files: ['main.ts'], exclude: ['helper.ts'] }),
        'helper.ts': 'export function helper(){return 1}', 'main.ts': "import {helper} from './helper'; export const value=helper();" });
    const { graph } = await f.indexed();
    assert(named(graph, 'helper'));
    assert(graph.observations.some(e => e.relation === 'calls' && e.resolution === 'exact_binding'));
});
test('extends retains the base configuration origin for relative baseUrl and paths', async (t) => {
    const f = fixture(t, { 'config/base.json': json({ compilerOptions: { ...options, baseUrl: '..', paths: { '@shared/*': ['shared/*'] } } }),
        'app/tsconfig.json': json({ extends: '../config/base.json', include: ['*.ts'] }), 'shared/value.ts': 'export function shared(){return 7}',
        'app/main.ts': "import {shared} from '@shared/value'; export const output=shared();" });
    const { graph } = await f.indexed();
    const target = named(graph, 'shared');
    assert(graph.observations.some(e => e.target === target.id && e.relation === 'calls' && e.analysisContext === 'app/tsconfig.json'));
    assert(!graph.diagnostics.some(d => d.code === 'TS2307'));
    assert.equal(graph.coverage.analysisContexts.length, 1);
});
test('separate default configs keep unrelated global scripts from contaminating each other', async (t) => {
    const f = fixture(t, { 'a/tsconfig.json': json({ files: ['main.ts'] }), 'b/tsconfig.json': json({ files: ['main.ts'] }),
        'a/main.ts': 'const local = 1;', 'b/main.ts': 'const local = 2;' });
    const { graph } = await f.indexed();
    assert.equal(graph.entities.filter(e => e.name === 'local').length, 2);
    assert(!graph.diagnostics.some(d => d.code === 'TS2451'));
});
test('each package resolves the same alias against its own tsconfig', async (t) => {
    const files = {};
    for (const pkg of ['one', 'two']) {
        files[`${pkg}/tsconfig.json`] = json({ compilerOptions: { ...options, baseUrl: '.', paths: { '@value': ['value.ts'] } }, files: ['main.ts'] });
        files[`${pkg}/value.ts`] = `export function value(){return '${pkg}'}`;
        files[`${pkg}/main.ts`] = "import {value} from '@value'; export const use=value();";
    }
    const f = fixture(t, files);
    const { graph } = await f.indexed();
    for (const pkg of ['one', 'two']) {
        const value = named(graph, 'value', `${pkg}/value.ts`);
        const edges = graph.observations.filter(e => e.target === value.id && e.relation === 'calls');
        assert.equal(edges.length, 1);
        assert.equal(edges[0].analysisContext, `${pkg}/tsconfig.json`);
    }
});
test('project references discover source and resolve calls without generated outputs', async (t) => {
    const f = fixture(t, monorepo);
    const { graph } = await f.indexed();
    const total = named(graph, 'invoiceTotal');
    assert(graph.observations.some(e => e.target === total.id && e.relation === 'calls' && e.analysisContext === 'packages/app/tsconfig.json'));
    assert.equal(graph.coverage.analysisContexts.length, 3);
    assert(!graph.diagnostics.some(d => d.code === 'TS6305' || d.code === 'TS2307'), json(graph.diagnostics));
});
test('cross-project compiler rename deduplicates edits and updates the importing project', async (t) => {
    const f = fixture(t, monorepo);
    const { snapshot, graph } = await f.indexed();
    const total = named(graph, 'invoiceTotal');
    const out = await okInvoke(f.engine, 'ci_change_prepare', { snapshotId: snapshot.id, rename: { entityId: total.id, newName: 'calculateInvoice' } });
    const plan = out.data.plan ?? out.data;
    const result = f.ws.store.snapshot(plan.resultSnapshotId);
    assert(f.ws.read(result, 'packages/core/src/math.ts').toString().includes('function calculateInvoice'));
    assert(f.ws.read(result, 'packages/app/src/main.ts').toString().includes('calculateInvoice(4)'));
    assert.equal(plan.files.length, 2);
    assert(f.read('packages/core/src/math.ts').includes('function invoiceTotal'));
});
test('a shared source file retains conflicting bindings as separate analysis contexts', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ files: [], references: [{ path: './dev.json' }, { path: './prod.json' }] }),
        'dev.json': json({ compilerOptions: { ...options, baseUrl: '.', paths: { '@env': ['dev.ts'] } }, files: ['main.ts'] }),
        'prod.json': json({ compilerOptions: { ...options, baseUrl: '.', paths: { '@env': ['prod.ts'] } }, files: ['main.ts'] }),
        'dev.ts': 'export function mode(){return 1}', 'prod.ts': 'export function mode(){return 2}',
        'main.ts': "import {mode} from '@env';export const value=mode();" });
    const { graph, snapshot } = await f.indexed();
    const use = named(graph, 'value', 'main.ts');
    assert.deepEqual(use.metadata.analysisContexts, ['dev.json', 'prod.json']);
    const calls = graph.observations.filter(e => e.source === use.id && e.relation === 'calls');
    assert.equal(calls.length, 2);
    assert.equal(new Set(calls.map(e => e.target)).size, 2);
    const traced = await okInvoke(f.engine, 'ci_trace', { snapshotId: snapshot.id, entityIds: [use.id], relations: ['calls'], analysisContexts: ['dev.json'] });
    assert.equal(traced.data.edges.length, 1);
    assert.equal(traced.data.edges[0].analysisContext, 'dev.json');
});
test('changing only a config invalidates unchanged consumer bindings', async (t) => {
    const config = path => json({ compilerOptions: { ...options, paths: { '@env': [path] } }, files: ['main.ts'] });
    const f = fixture(t, { 'tsconfig.json': config('./one.ts'), 'one.ts': 'export function value(){return 1}', 'two.ts': 'export function value(){return 2}',
        'main.ts': "import {value} from '@env'; export const use=value();" });
    const first = await f.indexed();
    f.write('tsconfig.json', config('./two.ts'));
    const second = await f.indexed();
    assert.notEqual(first.snapshot.id, second.snapshot.id);
    assert(second.graph.observations.some(e => e.relation === 'calls' && e.target === named(second.graph, 'value', 'two.ts').id));
});
test('missing/outside project references are diagnostics, not silently complete analysis', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ files: ['main.ts'], references: [{ path: '../../outside' }] }), 'main.ts': 'export const v=1;' });
    const { graph } = await f.indexed();
    assert(graph.diagnostics.some(d => d.code === 'CI_TS_REFERENCE_UNAVAILABLE'));
});
test('reference cycles terminate and produce an explicit cycle diagnostic', async (t) => {
    const f = fixture(t, { 'a/tsconfig.json': json({ files: ['a.ts'], references: [{ path: '../b' }] }), 'a/a.ts': 'export const a=1;',
        'b/tsconfig.json': json({ files: ['b.ts'], references: [{ path: '../a' }] }), 'b/b.ts': 'export const b=1;' });
    const { graph } = await f.indexed();
    assert(graph.diagnostics.some(d => d.code === 'CI_TS_REFERENCE_CYCLE'));
});
test('malformed and circular extended config errors preserve their own paths', async (t) => {
    const f = fixture(t, { 'app/tsconfig.json': '{"extends":"./base.json", "files":["main.ts"]}', 'app/base.json': '{"extends":"./tsconfig.json"}', 'app/main.ts': 'export const v=1;' });
    const { graph } = await f.indexed();
    assert(graph.diagnostics.some(d => d.code === 'TS18000' && d.analysisContext === 'app/tsconfig.json'));
});
test('jsconfig is discovered and JavaScript is not forced into TypeScript strict mode', async (t) => {
    const f = fixture(t, { 'app/jsconfig.json': json({ compilerOptions: { checkJs: true }, include: ['*.js'] }), 'app/main.js': 'function identity(value){return value;}\n' });
    const { graph } = await f.indexed();
    assert(named(graph, 'identity'));
    assert(!graph.diagnostics.some(d => d.code === 'TS7006'));
});
test('nonexistent files from explicit config lists are reported by the compiler', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ files: ['exists.ts', 'missing.ts'] }), 'exists.ts': 'export const value=1;' });
    const { graph } = await f.indexed();
    assert(graph.diagnostics.some(d => d.code === 'TS6053'), json(graph.diagnostics));
});
test('diagnostic baseline comparison includes configuration identity', () => {
    const item = { path: 'shared.ts', code: 'TS123', severity: 'error', message: 'x' };
    const delta = diagnosticDelta([{ ...item, analysisContext: 'dev.json' }], [{ ...item, analysisContext: 'prod.json' }]);
    assert.equal(delta.added.length, 1);
    assert.equal(delta.removed.length, 1);
});
test('too many configs fail explicitly before constructing unbounded compiler programs', t => {
    const files = {};
    for (let i = 0; i < 129; i++)
        files[`p${i}/tsconfig.json`] = json({ files: [] });
    const f = fixture(t, files);
    const s = f.snapshot();
    assert.throws(() => new TypeScriptProject(s, f.ws.contents(s), f.root, new Deadline(10000)), e => e.code === 'CAPACITY_EXCEEDED');
});
test('compiler reads remain immutable after captured config/source changes on disk', async (t) => {
    const f = fixture(t, monorepo);
    const s = f.snapshot();
    f.write('packages/app/tsconfig.json', '{}');
    f.write('packages/core/src/math.ts', 'throw new Error("not captured")');
    await f.engine.indexer.ensure(s, new Deadline(120000));
    const graph = f.ws.store.graph(s.id);
    assert(named(graph, 'invoiceTotal'));
    assert(graph.observations.some(e => e.relation === 'calls' && e.resolution === 'exact_binding'));
});
test('rename refuses to modify a shared consumer that binds differently in another variant', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ files: [], references: [{ path: './dev.json' }, { path: './prod.json' }] }),
        'dev.json': json({ compilerOptions: { ...options, paths: { '@env': ['./dev.ts'] } }, files: ['main.ts'] }),
        'prod.json': json({ compilerOptions: { ...options, paths: { '@env': ['./prod.ts'] } }, files: ['main.ts'] }),
        'dev.ts': 'export function mode(){return 1}', 'prod.ts': 'export function mode(){return 2}', 'main.ts': "import {mode} from '@env';export const value=mode();" });
    const { snapshot, graph } = await f.indexed();
    const target = named(graph, 'mode', 'dev.ts');
    const r = await f.engine.invoke('ci_change_prepare', { snapshotId: snapshot.id, rename: { entityId: target.id, newName: 'developmentMode' } });
    assert.equal(r.status, 'error');
    assert.equal(r.error.code, 'RENAME_CONTEXT_CONFLICT');
    assert(f.read('main.ts').includes('{mode}'));
});
test('zero configured source files do not claim semantic coverage', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ files: [] }), 'one.ts': 'export const one=1;' });
    const { graph } = await f.indexed();
    assert.equal(graph.coverage.languages.typescript.semanticFiles, 0);
    assert.equal(graph.coverage.languages.typescript.semantic, false);
});
test('configured strict false is not silently overridden by inferred defaults', async (t) => {
    const f = fixture(t, { 'tsconfig.json': json({ compilerOptions: { strict: false }, files: ['main.ts'] }), 'main.ts': 'export function value(x){return x;}' });
    const { graph } = await f.indexed();
    assert(!graph.diagnostics.some(d => d.code === 'TS7006'));
});
