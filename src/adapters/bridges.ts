import { readFileSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CodeIntelligence } from '../engine.js';
import { Deadline, assert, hash } from '../core/util.js';
import { runProcess } from '../core/process.js';
import { dependencyPath } from '../core/dependencies.js';
export async function runBridge(engine: CodeIntelligence, backend: 'graft' | 'graphify', snapshotId: string, options: {
    trusted: boolean;
    python?: string;
    timeoutMs?: number;
}): Promise<Record<string, unknown>> {
    assert(options.trusted, 'PROVIDER_UNTRUSTED', 'External providers require explicit --trust-providers. They are installed separately; network isolation is not guaranteed.');
    const deadline = new Deadline(options.timeoutMs ?? 120000);
    const snapshot = engine.workspace.store.snapshot(snapshotId);
    const sandbox = join(engine.workspace.store.directory, 'changes', `bridge-${randomUUID()}`, 'tree');
    engine.workspace.materialize(snapshot, sandbox);
    const input = JSON.stringify({ root: sandbox, snapshotId, sourceHashes: Object.fromEntries(snapshot.files.map(f => [f.path, f.hash])) });
    const script = fileURLToPath(new URL(backend === 'graft' ? '../../workers/graft_bridge.mjs' : '../../workers/graphify_bridge.py', import.meta.url));
    try {
        const output = await runProcess(backend === 'graft' ? process.execPath : options.python ?? engine.workspace.config.python, backend === 'graft' ? [script] : ['-I', script], { cwd: sandbox, deadline, input, maxBytes: 48000000, env: { DO_NOT_TRACK: '1', GRAFT_TELEMETRY_DISABLED: '1', GRAFT_API_KEY: '', OPENAI_API_KEY: '', ANTHROPIC_API_KEY: '' } });
        assert(output.exitCode === 0 && !output.truncated, 'PROVIDER_FAILED', output.stderr || 'Provider failed or output exceeded bound');
        const parsed = JSON.parse(output.stdout);
        const nodes = parsed.graph?.nodes;
        assert(Array.isArray(nodes), 'ADAPTER_CONTRACT', 'Bridge must return graph nodes');
        // Check every captured source, including dependencies, not only returned nodes.
        for (const file of snapshot.files) {
            deadline.check();
            assert(hash(readFileSync(dependencyPath(sandbox, file.path))) === file.hash, 'SNAPSHOT_STALE', 'External provider changed a captured source file');
        }
        deadline.check();
        await engine.import(backend, snapshotId, parsed);
        return { backend, snapshotId, imported: true, networkIsolation: 'not_enforced', source: 'isolated_snapshot', modelEnrichmentRequested: false };
    }
    finally {
        rmSync(join(sandbox, '..'), { recursive: true, force: true });
    }
}
