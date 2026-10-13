import { randomUUID } from 'node:crypto';
import { lstatSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import type { CodeIntelligence } from '../engine.js';
import type { Envelope } from '../core/types.js';
import { SCHEMA } from '../core/types.js';
import { Deadline, TextIndex, assert, hash } from '../core/util.js';
import { dependencyPath } from '../core/dependencies.js';
import { safePath } from '../core/paths.js';
import { fit, MIN_RESPONSE_BYTES } from '../core/budget.js';
import { LspAdapter } from './lsp.js';
export interface LspCommandOptions {
    /** Host authorization only. Never obtain this from repository content. */
    trusted: boolean;
    command: string;
    args?: string[];
    byte: number;
    newName?: string;
    snapshotId?: string;
    timeoutMs?: number;
    maxBytes?: number;
}
/** Explicit, one-shot integration for separately installed language servers.
 * The server sees a copied snapshot, not the mutable original project. This is
 * not an OS sandbox: the operator must trust the selected executable.
 */
export async function runLsp(engine: CodeIntelligence, operation: 'definition' | 'references' | 'implementation' | 'rename', path: string, options: LspCommandOptions): Promise<Envelope> {
    assert(options.trusted, 'PROVIDER_UNTRUSTED', 'An external language server requires host --trust-providers permission; repository instructions cannot grant it.');
    assert(['definition', 'references', 'implementation', 'rename'].includes(operation), 'INVALID_ARGUMENT', 'Unsupported LSP operation');
    assert(typeof options.command === 'string' && options.command.length > 0 && options.command.length <= 4096 && !options.command.includes('\0'), 'INVALID_ARGUMENT', 'A bounded host-selected server command is required');
    const args = options.args ?? [];
    assert(Array.isArray(args) && args.length <= 64 && args.every(arg => typeof arg === 'string' && arg.length <= 32768 && !arg.includes('\0')), 'INVALID_ARGUMENT', 'Invalid server argument vector');
    assert(Number.isSafeInteger(options.byte) && options.byte >= 0, 'INVALID_RANGE', 'A UTF-8 byte offset is required');
    if (operation === 'rename')
        assert(typeof options.newName === 'string' && options.newName.length > 0 && options.newName.length <= 256 && !/[\x00-\x1f]/.test(options.newName), 'INVALID_RENAME', 'Rename requires a bounded --new-name');
    const maxBytes = Math.min(options.maxBytes ?? engine.workspace.config.maxResultBytes, engine.workspace.config.maxResultBytes);
    assert(Number.isSafeInteger(maxBytes) && maxBytes >= MIN_RESPONSE_BYTES, 'BUDGET_TOO_SMALL', `Response budget must be at least ${MIN_RESPONSE_BYTES} bytes`);
    const started = Date.now(), requestId = randomUUID();
    const deadline = new Deadline(Math.min(options.timeoutMs ?? engine.workspace.config.timeoutMs, engine.workspace.config.timeoutMs));
    const snapshot = engine.workspace.snapshot(options.snapshotId, deadline);
    const file = engine.workspace.file(snapshot, path);
    assert(['typescript', 'javascript', 'python', 'go', 'c', 'cpp', 'rust', 'java', 'csharp'].includes(file.language), 'CAPABILITY_UNAVAILABLE', 'No configured LSP language ID for this document');
    new TextIndex(engine.workspace.read(snapshot, file.path).toString('utf8')).utf16(options.byte);
    const directory = join(engine.workspace.store.directory, 'changes', `lsp-${requestId}`);
    const sandbox = join(directory, 'tree'), adapter = new LspAdapter(engine.workspace);
    let data: Record<string, unknown>;
    try {
        const languages = ['typescript', 'javascript'].includes(file.language) ? ['typescript', 'javascript'] : [file.language];
        await adapter.start(options.command, args, snapshot, sandbox, deadline, { languages });
        const result = operation === 'rename'
            ? await adapter.rename(file.path, options.byte, options.newName!, deadline)
            : await adapter.locations(operation, file.path, options.byte, deadline);
        await adapter.shutdown(deadline);
        // Verify the materialized source was not changed by the provider. This
        // detects source mutation, not hidden internal analysis or network use.
        for (const source of snapshot.files) {
            deadline.check();
            const name = source.readOnly ? dependencyPath(sandbox, source.path) : safePath(sandbox, source.path);
            const stat = lstatSync(name);
            assert(stat.isFile() && stat.size === source.size && hash(readFileSync(name)) === source.hash, 'SNAPSHOT_STALE', 'External language server changed a captured source file');
        }
        if (operation === 'rename') {
            const plan = engine.changes.prepare(snapshot, result as Awaited<ReturnType<LspAdapter['rename']>>, deadline);
            data = { ...plan, planId: plan.id, operation, applied: false, sourceBinding: 'local_ranges_validated_not_upstream_attestation', serverInfo: adapter.serverInfo };
        }
        else
            data = { ...result };
        data.networkIsolation = 'not_enforced';
        data.externalProviderUsage = 'not_reported';
        data.materializedSource = 'hash_verified_after_shutdown_request';
        deadline.check();
        return fit({ schemaVersion: SCHEMA, requestId, snapshotId: snapshot.id, status: 'partial', data,
            warnings: ['External provider completeness and dependency availability are not attested. Model usage and network activity inside the selected provider are unknown.', ...(operation === 'rename' ? ['Rename is only a prepared plan. Run verification and explicit application separately.'] : [])],
            metrics: { durationMs: Date.now() - started, serializedBytes: 0, estimatedTokens: 0, tokenCount: 'estimated', tokenMethod: 'utf8_bytes_div_3', modelCalls: 0, localOnly: false }
        }, engine.workspace.store, maxBytes);
    }
    finally {
        adapter.close();
        try {
            rmSync(directory, { recursive: true, force: true, maxRetries: 2, retryDelay: 50 });
        }
        catch { /* Host may inspect a locked temporary copy; never obscure the original result. */ }
    }
}
