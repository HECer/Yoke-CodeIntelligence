import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve, toNamespacedPath } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CodeIntelligence } from '../engine.js';
import { Deadline, assert, hash } from '../core/util.js';
import { runProcess } from '../core/process.js';
import { dependencyPath } from '../core/dependencies.js';
import { safePath } from '../core/paths.js';
export async function runBridge(engine: CodeIntelligence, backend: 'graft' | 'graphify', snapshotId: string, options: {
    trusted: boolean;
    python?: string;
    timeoutMs?: number;
}): Promise<Record<string, unknown>> {
    assert(options.trusted, 'PROVIDER_UNTRUSTED', 'External providers require explicit --trust-providers. They are installed separately; network isolation is not guaranteed.');
    const deadline = new Deadline(options.timeoutMs ?? 120000);
    const snapshot = engine.workspace.store.snapshot(snapshotId);
    const changes = resolve(engine.workspace.store.directory, 'changes');
    const owned = join(changes, `bridge-${randomUUID()}`);
    const sandbox = join(owned, 'tree');
    const cacheParent = resolve(tmpdir());
    const cachePrefix = `ci-gfy-${hash(engine.workspace.store.directory).slice(0, 16)}-`;
    let cacheRoot: string | undefined;
    const script = fileURLToPath(new URL(backend === 'graft' ? '../../workers/graft_bridge.mjs' : '../../workers/graphify_bridge.py', import.meta.url));
    try {
        engine.workspace.materialize(snapshot, sandbox);
        if (backend === 'graphify')
            cacheRoot = mkdtempSync(join(cacheParent, cachePrefix));
        const input = JSON.stringify({ root: sandbox, snapshotId, sourceHashes: Object.fromEntries(snapshot.files.map(f => [f.path, f.hash])), cacheRoot: cacheRoot && toNamespacedPath(cacheRoot) });
        const output = await runProcess(backend === 'graft' ? process.execPath : options.python ?? engine.workspace.config.python, backend === 'graft' ? [script] : ['-I', script], { cwd: sandbox, deadline, input, maxBytes: 48000000, env: { DO_NOT_TRACK: '1', GRAFT_TELEMETRY_DISABLED: '1', GRAFT_API_KEY: '', OPENAI_API_KEY: '', ANTHROPIC_API_KEY: '' } });
        assert(output.exitCode === 0 && !output.truncated, 'PROVIDER_FAILED', output.stderr || 'Provider failed or output exceeded bound');
        const parsed = JSON.parse(output.stdout);
        const nodes = parsed.graph?.nodes;
        assert(Array.isArray(nodes), 'ADAPTER_CONTRACT', 'Bridge must return graph nodes');
        // Check every captured source, including dependencies, not only returned nodes.
        for (const file of snapshot.files) {
            deadline.check();
            const path = file.readOnly && file.path.split('/').includes('node_modules') ? dependencyPath(sandbox, file.path) : safePath(sandbox, file.path);
            assert(hash(readFileSync(path)) === file.hash, 'SNAPSHOT_STALE', 'External provider changed a captured source file');
        }
        deadline.check();
        await engine.import(backend, snapshotId, parsed);
        return { backend, snapshotId, imported: true, networkIsolation: 'not_enforced', source: 'isolated_snapshot', modelEnrichmentRequested: false };
    }
    finally {
        const cleanup = [{ path: owned, parent: changes, prefix: 'bridge-' }];
        if (cacheRoot)
            cleanup.push({ path: cacheRoot, parent: cacheParent, prefix: cachePrefix });
        const errors: unknown[] = [];
        for (const entry of cleanup) {
            try {
                const path = resolve(entry.path);
                assert(dirname(path) === entry.parent && basename(path).startsWith(entry.prefix), 'PROVIDER_FAILED', 'Owned provider cleanup escaped its allocated parent');
                rmSync(path, { recursive: true, force: true });
            }
            catch (error) { errors.push(error); }
        }
        if (errors.length)
            throw new AggregateError(errors, 'Owned provider workspace cleanup failed');
    }
}
