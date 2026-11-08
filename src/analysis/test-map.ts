import { randomUUID } from 'node:crypto';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { safePath } from '../core/paths.js';
import { dependencyPath } from '../core/dependencies.js';
import { CodeIntelligence } from '../engine.js';
import { Snapshot } from '../core/types.js';
import { Deadline, assert, object, hash, TextIndex, stable } from '../core/util.js';
import { runProcess } from '../core/process.js';
export interface TestObservation {
    testId: string;
    snapshotId: string;
    files: {
        path: string;
        contentHash: string;
        lines: number[];
    }[];
    executed: boolean;
    passed: boolean;
}
export function importTestCoverage(engine: CodeIntelligence, s: Snapshot, input: unknown): Record<string, unknown> {
    const value = object(input);
    assert(value.snapshotId === s.id && Array.isArray(value.tests) && value.tests.length <= 10000, 'INVALID_COVERAGE', 'Snapshot-bound test records required');
    const tests: TestObservation[] = [];
    const ids = new Set<string>();
    for (const raw of value.tests) {
        const t = object(raw);
        assert(typeof t.testId === 'string' && t.testId.length > 0 && t.testId.length <= 500 && !ids.has(t.testId) && t.executed === true && typeof t.passed === 'boolean' && Array.isArray(t.files) && t.files.length <= s.files.length, 'INVALID_COVERAGE', 'Invalid test observation');
        ids.add(t.testId);
        const seen = new Set<string>();
        const files = t.files.map(raw => { const f = object(raw); assert(typeof f.path === 'string' && !seen.has(f.path), 'INVALID_COVERAGE', 'Duplicate/invalid coverage file'); seen.add(f.path); const source = engine.workspace.file(s, f.path); assert(f.contentHash === source.hash && Array.isArray(f.lines) && f.lines.length <= 1000000, 'SNAPSHOT_STALE', 'Coverage hash or shape mismatch'); const lines = new TextIndex(engine.workspace.read(s, f.path).toString('utf8')).lineStarts.length; assert(f.lines.every(l => Number.isInteger(l) && l >= 1 && l <= lines), 'INVALID_COVERAGE', 'Invalid line hit'); return { path: f.path, contentHash: source.hash, lines: [...new Set(f.lines as number[])].sort((a, b) => a - b) }; });
        tests.push({ testId: t.testId, snapshotId: s.id, files, executed: true, passed: t.passed });
    }
    const artifactId = engine.workspace.store.artifact(s.id, { kind: 'per_test_coverage', snapshotId: s.id, tests });
    engine.workspace.store.setCache(`test-map:${s.id}`, { artifactId, tests });
    return { snapshotId: s.id, artifactId, testCount: tests.length, sourceBinding: 'hash_verified', exhaustive: false };
}
export function selectTests(engine: CodeIntelligence, base: Snapshot, target: Snapshot, knownTests: string[]): Record<string, unknown> {
    const map = engine.workspace.store.cache<{
        tests: TestObservation[];
    }>(`test-map:${base.id}`), before = new Map(base.files.map(f => [f.path, f.hash])), after = new Map(target.files.map(f => [f.path, f.hash]));
    const changed = [...new Set([...before.keys(), ...after.keys()])].filter(path => before.get(path) !== after.get(path));
    if (!changed.length)
        return { tests: [], changedPaths: [], reason: 'no_captured_source_change', exhaustive: false };
    const covered = new Set(map?.tests.flatMap(t => t.files.map(f => f.path)) ?? []), unknown = changed.filter(p => !covered.has(p));
    const selected = new Set(map?.tests.filter(t => !t.passed || t.files.some(f => changed.includes(f.path))).map(t => t.testId) ?? []);
    // New/unknown tests, execution metadata and missing mapping always widen the selection.
    const configChanged = changed.some(p => /(^|\/)(package(-lock)?\.json|tsconfig.*\.json|go\.(mod|sum)|pyproject\.toml)|(^|\/)(test|tests|__tests__)\//.test(p));
    const incomplete = !map || unknown.length > 0 || configChanged || knownTests.some(id => !map.tests.some(t => t.testId === id));
    if (incomplete)
        for (const id of knownTests)
            selected.add(id);
    return { tests: [...selected].sort(), changedPaths: changed, unknownPaths: unknown, broadened: incomplete, reason: incomplete ? 'unknown_coverage_or_execution_context' : 'recorded_per_test_dependencies', exhaustive: false, warning: 'Dynamic coverage only describes observed runs. Use the full suite as the final release gate.' };
}
/** Actually executes each Node test file separately, producing observed per-test-file coverage. */
export async function collectNodeTestCoverage(engine: CodeIntelligence, s: Snapshot, testPaths: (string | {
    testId: string;
    path: string;
    namePattern: string;
})[], trusted: boolean, deadline: Deadline): Promise<Record<string, unknown>> {
    assert(trusted, 'PROJECT_UNTRUSTED', 'Executing tests requires explicit host permission');
    assert(testPaths.length > 0 && testPaths.length <= 100, 'INVALID_ARGUMENT', 'Select 1–100 test files');
    const directory = join(engine.workspace.store.directory, 'changes', 'coverage-' + randomUUID()), tests: TestObservation[] = [], jobs: unknown[] = [];
    try {
        for (let n = 0; n < testPaths.length; n++) {
            deadline.check();
            const spec = testPaths[n]!, path = engine.workspace.file(s, typeof spec === 'string' ? spec : spec.path).path, testId = typeof spec === 'string' ? spec : spec.testId;
            assert(typeof testId === 'string' && testId.length > 0 && testId.length <= 500, 'INVALID_ARGUMENT', 'Invalid test ID');
            if (typeof spec !== 'string')
                assert(typeof spec.namePattern === 'string' && spec.namePattern.length > 0 && spec.namePattern.length <= 1000, 'INVALID_ARGUMENT', 'A bounded Node name pattern is required');
            assert(/\.[cm]?js$/.test(path), 'CAPABILITY_UNAVAILABLE', 'Native collector runs JavaScript test files; import maps from other runners');
            const root = join(directory, String(n), 'tree'), coverage = join(directory, String(n), 'coverage');
            engine.workspace.materialize(s, root);
            mkdirSync(coverage, { recursive: true });
            const out = await runProcess(process.execPath, ['--test', '--test-reporter', new URL('../../workers/test_reporter.mjs', import.meta.url).href, ...(typeof spec === 'string' ? [] : ['--test-name-pattern', spec.namePattern]), path], { cwd: root, deadline, maxBytes: 1000000, env: { NODE_V8_COVERAGE: coverage } });
            const hits = new Map<string, Set<number>>();
            for (const name of readdirSync(coverage).filter(p => p.endsWith('.json'))) {
                const parsed = JSON.parse(readFileSync(join(coverage, name), 'utf8'));
                for (const entry of parsed.result ?? []) {
                    if (!String(entry.url).startsWith('file:'))
                        continue;
                    let relativePath: string;
                    try {
                        relativePath = relative(root, fileURLToPath(entry.url)).replaceAll('\\', '/');
                    }
                    catch {
                        continue;
                    }
                    const file = s.files.find(f => f.path === relativePath);
                    if (!file)
                        continue;
                    const bytes = readFileSync(file.readOnly ? dependencyPath(root, relativePath) : safePath(root, relativePath));
                    assert(hash(bytes) === file.hash, 'SNAPSHOT_STALE', 'Executed source was modified before coverage could be bound');
                    const ti = new TextIndex(bytes.toString('utf8')), ranges = (entry.functions ?? []).flatMap((f: any) => f.ranges ?? []);
                    const lines = hits.get(file.path) ?? new Set<number>();
                    for (let i = 0; i < ti.lineStarts.length; i++) {
                        const pos = ti.lineStarts[i]!;
                        const containing = ranges.filter((r: any) => Number.isInteger(r.startOffset) && Number.isInteger(r.endOffset) && r.startOffset <= pos && r.endOffset > pos).sort((a: any, b: any) => (a.endOffset - a.startOffset) - (b.endOffset - b.startOffset));
                        if (containing[0]?.count > 0)
                            lines.add(i + 1);
                    }
                    hits.set(file.path, lines);
                }
            }
            const events = out.stdout.split('\n').filter(Boolean).map(line => { try {
                return JSON.parse(line);
            }
            catch {
                return null;
            } });
            const fileSummary = events.find(e => e?.type === 'test:summary' && typeof e.data?.file === 'string' && relative(root, e.data.file).replaceAll('\\', '/') === path)?.data;
            const count = typeof fileSummary?.counts?.tests === 'number' ? fileSummary.counts.tests : 0;
            let passed = out.exitCode === 0 && !out.truncated && fileSummary?.success === true && count > 0 && fileSummary?.counts?.passed > 0 && fileSummary?.counts?.failed === 0;
            for (const f of s.files) {
                try {
                    if (hash(readFileSync(f.readOnly ? dependencyPath(root, f.path) : safePath(root, f.path))) !== f.hash)
                        passed = false;
                }
                catch {
                    passed = false;
                }
            }
            tests.push({ testId, snapshotId: s.id, executed: true, passed, files: [...hits].map(([path, lines]) => ({ path, contentHash: engine.workspace.file(s, path).hash, lines: [...lines] })) });
            jobs.push({ testId, selection: typeof spec === 'string' ? 'test_file' : spec.namePattern, exitCode: out.exitCode, passed, testCount: count, outputTruncated: out.truncated, logArtifactId: engine.workspace.store.artifact(s.id, { kind: 'per_test_log', path, ...out }) });
        }
    }
    finally {
        rmSync(directory, { recursive: true, force: true, maxRetries: 2, retryDelay: 20 });
    }
    return { ...importTestCoverage(engine, s, { snapshotId: s.id, tests }), jobs, granularity: testPaths.some(p => typeof p !== 'string') ? 'isolated_name_pattern_selection' : 'test_file', networkIsolation: 'not_enforced_trusted_process' };
}
