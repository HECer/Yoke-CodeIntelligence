import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { readFileSync, existsSync, lstatSync, rmSync } from 'node:fs';
import { CodeIntelligence } from '../engine.js';
import { Workspace } from '../core/workspace.js';
import { FileEdit } from '../core/types.js';
import { Deadline, assert, hash, object } from '../core/util.js';
import { safePath } from '../core/paths.js';
import { dependencyPath } from '../core/dependencies.js';
import { SerenaAdapter } from './serena.js';
export interface SerenaCommandOptions {
    trusted: boolean;
    command: string;
    args: string[];
    snapshotId?: string;
    timeoutMs?: number;
    allowArbitraryCode?: boolean;
}
/** Discover real capabilities; edits are diffed in an isolated tree and only prepared. */
export async function runSerena(engine: CodeIntelligence, mode: 'capabilities' | 'read' | 'prepare', tool: string | undefined, args: Record<string, unknown>, options: SerenaCommandOptions): Promise<Record<string, unknown>> {
    assert(options.trusted, 'PROVIDER_UNTRUSTED', 'Explicit host provider permission is required');
    const deadline = new Deadline(Math.min(options.timeoutMs ?? 120000, engine.workspace.config.timeoutMs)), s = engine.workspace.snapshot(options.snapshotId, deadline);
    const dir = join(engine.workspace.store.directory, 'changes', `serena-${randomUUID()}`), tree = join(dir, 'tree');
    engine.workspace.materialize(s, tree);
    const adapter = new SerenaAdapter();
    try {
        await adapter.start(options.command, options.args, tree, deadline);
        if (mode === 'capabilities')
            return { capabilities: adapter.capabilities(), installed: true, snapshotId: s.id };
        assert(tool, 'INVALID_ARGUMENT', 'A Serena tool is required');
        const result = await adapter.call(tool, args, deadline, { allowMutation: mode === 'prepare', allowArbitraryCode: options.allowArbitraryCode });
        adapter.close();
        const edits: FileEdit[] = [];
        for (const f of s.files) {
            deadline.check();
            const name = f.readOnly ? dependencyPath(tree, f.path) : safePath(tree, f.path);
            if (!existsSync(name)) {
                assert(mode === 'prepare' && !f.readOnly, 'PROVIDER_MUTATION', 'Read provider deleted source');
                edits.push({ kind: 'delete', path: f.path, expectedHash: f.hash });
                continue;
            }
            assert(lstatSync(name).isFile(), 'PATH_DENIED', 'Provider replaced source with a non-file');
            const content = readFileSync(name);
            if (hash(content) !== f.hash) {
                assert(mode === 'prepare' && !f.readOnly, 'PROVIDER_MUTATION', 'Read provider changed source');
                edits.push({ kind: 'replace', path: f.path, expectedHash: f.hash, startByte: 0, endByte: f.size, text: content.toString('utf8') });
            }
        }
        if (mode === 'prepare') {
            const check = new Workspace(tree, engine.workspace.config);
            try {
                const after = check.capture(deadline);
                for (const omission of after.omissions)
                    assert(omission.path === '.' || s.omissions.some(o => o.path === omission.path), 'PROVIDER_OUTPUT_UNCAPTURED', 'Provider produced uncaptured source');
                for (const f of after.files)
                    if (!s.files.some(old => old.path === f.path)) {
                        assert(!f.readOnly, 'READ_ONLY_DEPENDENCY', 'Provider added dependency declarations');
                        edits.push({ kind: 'create', path: f.path, text: check.read(after, f.path).toString('utf8') });
                    }
            }
            finally {
                check.close();
            }
            const plan = engine.changes.prepare(s, edits, deadline);
            return { snapshotId: s.id, planId: plan.id, resultSnapshotId: plan.resultSnapshotId, applied: false, upstreamResult: result, networkIsolation: 'not_enforced', providerContract: 'live_advertised_tools', arbitraryRuntime: !!options.allowArbitraryCode };
        }
        return { snapshotId: s.id, data: result, sourceFilesUnchanged: true, networkIsolation: 'not_enforced', arbitraryRuntime: !!options.allowArbitraryCode };
    }
    finally {
        adapter.close();
        rmSync(dir, { recursive: true, force: true, maxRetries: 3, retryDelay: 50 });
    }
}
