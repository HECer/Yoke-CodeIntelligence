import { randomUUID } from 'node:crypto';
import { Workspace } from './core/workspace.js';
import { Deadline, assert, object, text, CIError, stable, hash } from './core/util.js';
import { type Config, type Envelope, type Snapshot, type FileEdit, VERSION, SCHEMA } from './core/types.js';
import { Indexer } from './analysis/indexer.js';
import { QueryEngine } from './analysis/query.js';
import { Knowledge } from './analysis/knowledge.js';
import { analyzeGraph } from './analysis/analytics.js';
import { ChangeManager } from './changes/manager.js';
import { Verifier } from './changes/verify.js';
import { Ledger } from './core/ledger.js';
import { fit, measure, artifactPage } from './core/budget.js';
import { TOOLS } from './protocol/tools.js';
import { validate } from './protocol/validate.js';
import { importGraph, persistImport } from './adapters/graph-import.js';
export interface EngineOptions {
    config?: Partial<Config>;
    allowWrites?: boolean;
    trustProject?: boolean;
}
export class CodeIntelligence {
    readonly workspace: Workspace;
    readonly indexer: Indexer;
    readonly query: QueryEngine;
    readonly knowledge: Knowledge;
    readonly changes: ChangeManager;
    readonly verifier: Verifier;
    readonly ledger: Ledger;
    constructor(root: string, options: EngineOptions = {}) {
        this.workspace = new Workspace(root, options.config);
        this.indexer = new Indexer(this.workspace);
        this.query = new QueryEngine(this.workspace);
        this.knowledge = new Knowledge(this.workspace);
        this.changes = new ChangeManager(this.workspace, { allowWrites: options.allowWrites });
        this.verifier = new Verifier(this.workspace, options.trustProject);
        this.ledger = new Ledger(this.workspace.store);
    }
    async invoke(tool: string, argumentsValue: unknown = {}, signal?: AbortSignal): Promise<Envelope> {
        const started = Date.now(), requestId = randomUUID();
        let snapshotId: string | null = null;
        let maxBytes = this.workspace.config.maxResultBytes, tokenBudget: number | undefined;
        const envelope = (data: Record<string, unknown>, status: Envelope['status'] = 'ok', warnings: string[] = []): Envelope => ({ schemaVersion: SCHEMA, requestId, snapshotId, status, data, warnings, metrics: { durationMs: Date.now() - started, serializedBytes: 0, estimatedTokens: 0, tokenCount: 'estimated', tokenMethod: 'utf8_bytes_div_3', modelCalls: 0, localOnly: true } });
        try {
            const definition = TOOLS.find(t => t.name === tool);
            assert(definition, 'TOOL_NOT_FOUND', 'Unknown Code Intelligence tool');
            validate(definition.inputSchema, argumentsValue);
            const args = object(argumentsValue);
            maxBytes = Math.min(this.workspace.config.maxResultBytes, Number(args.maxBytes ?? this.workspace.config.maxResultBytes));
            tokenBudget = args.tokenBudget as number | undefined;
            assert(tokenBudget === undefined || tokenBudget >= 342, 'BUDGET_TOO_SMALL', 'Estimated token guidance cannot represent the minimum 1024-byte response envelope');
            const deadline = new Deadline(Math.min(this.workspace.config.timeoutMs, Number(args.timeoutMs ?? this.workspace.config.timeoutMs)), signal);
            if (tool === 'ci_read' && (args.cursor || args.artifactId)) {
                const cursor = typeof args.cursor === 'string' ? args.cursor : this.workspace.store.cursor.encode({ kind: 'artifact', artifactId: args.artifactId, snapshotId: this.workspace.store.readArtifact(String(args.artifactId)).snapshot, offset: 0 });
                const page = artifactPage(this.workspace.store, cursor, Math.max(64, Math.floor((maxBytes - 1100) / 2)));
                snapshotId = page.snapshotId;
                const out = envelope(page.data, page.nextCursor ? 'partial' : 'ok');
                out.nextCursor = page.nextCursor;
                return fit(out, this.workspace.store, maxBytes, tokenBudget);
            }
            if (tool === 'ci_status' && (!args.mode || args.mode === 'status')) {
                const current = this.workspace.store.meta('current');
                snapshotId = typeof args.snapshotId === 'string' ? args.snapshotId : current ?? null;
                const info = snapshotId ? this.workspace.store.graphInfo(snapshotId) : undefined;
                return fit(envelope({ version: VERSION, workspaceId: this.workspace.store.workspaceId, projectId: this.workspace.store.projectId, indexed: !!info, coverage: info?.coverage, statistics: this.workspace.store.stats(), snapshots: this.workspace.store.snapshots(5).map(s => ({ id: s.id, kind: s.kind, files: s.files.length, createdAt: s.createdAt })), capabilities: { typescript: { provider: 'typescript@5.8.3', symbols: true, references: true, rename: true }, python: { provider: 'stdlib_ast', configured: this.workspace.config.pythonEnabled, semantic: false, rename: false }, go: { provider: 'stdlib_go_parser', requires: 'installed_go_toolchain', semantic: false, rename: false }, graft: { mode: 'pinned_graph_import_or_opt_in_bridge', bundled: false }, graphify: { mode: 'pinned_raw_graph_import_or_opt_in_bridge', bundled: false }, lsp: { mode: 'opt_in_installed_provider', bundledServer: false }, serena: { mode: 'optional_external_mcp', bundled: false }, embeddings: { mode: 'precomputed_vector_import_and_cosine_search', modelBundled: false }, scope: 'disk_snapshots_unsaved_buffers_not_connected' }, cost: args.taskId ? this.ledger.total(String(args.taskId)) : { modelCalls: 0, harnessUsage: 'not_reported' }, minimumResponseBytes: 1024, defaultIndexingRunsProjectCode: false }), this.workspace.store, maxBytes, tokenBudget);
            }
            const snapshot = this.workspace.snapshot(args.snapshotId as string | undefined, deadline);
            snapshotId = snapshot.id;
            if (!['ci_read', 'ci_change_apply', 'ci_verify'].includes(tool))
                await this.indexer.ensure(snapshot, deadline);
            let data: Record<string, unknown>;
            switch (tool) {
                case 'ci_status':
                    data = args.mode === 'analyze' ? analyzeGraph(this.workspace, snapshot, deadline) : { snapshotId: snapshot.id, files: snapshot.files.length, ...this.workspace.store.graphInfo(snapshot.id), omissions: snapshot.omissions };
                    break;
                case 'ci_find':
                    if (args.kind === 'memory')
                        data = { items: this.knowledge.recall(snapshot, String(args.query ?? ''), Number(args.limit ?? 20)) };
                    else if (args.kind === 'vector') {
                        assert(typeof args.model === 'string' && Array.isArray(args.vector), 'INVALID_ARGUMENT', 'Vector search requires model and vector');
                        data = { items: this.knowledge.searchVector(snapshot, args.model, args.vector as number[], Number(args.limit ?? 20), deadline) };
                    }
                    else
                        data = this.query.find(snapshot, text(args.query, 'query', 4000), { kind: args.kind as string | undefined, path: args.path as string | undefined, limit: Number(args.limit ?? 20), offset: Number(args.offset ?? 0) }, deadline);
                    break;
                case 'ci_read':
                    data = this.query.read(snapshot, args);
                    break;
                case 'ci_trace':
                    data = this.query.trace(snapshot, args.entityIds as string[], args, deadline);
                    break;
                case 'ci_context':
                    assert((typeof args.query === 'string') !== Array.isArray(args.plan), 'INVALID_ARGUMENT', 'Provide either query or read-only plan');
                    data = Array.isArray(args.plan) ? this.query.plan(snapshot, args.plan as Record<string, unknown>[], deadline) : this.query.context(snapshot, String(args.query), deadline, Number(args.limit ?? 8));
                    if (args.includeMemory && typeof args.query === 'string')
                        data.memories = this.knowledge.recall(snapshot, args.query, 5);
                    break;
                case 'ci_change_prepare': {
                    assert([args.edits, args.rename, args.replaceSymbol].filter(v => v !== undefined).length === 1, 'INVALID_ARGUMENT', 'Choose edits, rename or replaceSymbol');
                    let edits: FileEdit[];
                    if (args.rename) {
                        const rename = object(args.rename);
                        const entity = this.workspace.store.entity(String(rename.entityId), snapshot.id);
                        assert(['typescript', 'javascript'].includes(entity.language) && entity.nameStartByte !== undefined, 'CAPABILITY_UNAVAILABLE', 'Native rename is available only for TypeScript/JavaScript symbols');
                        edits = this.indexer.typescript(snapshot).rename(entity.path, entity.nameStartByte, String(rename.newName));
                    }
                    else if (args.replaceSymbol) {
                        const replace = object(args.replaceSymbol);
                        const entity = this.workspace.store.entity(String(replace.entityId), snapshot.id);
                        assert(entity.kind !== 'file', 'INVALID_EDIT', 'Use explicit file edits for whole files');
                        assert(entity.producer === 'typescript-compiler@5.8.3', 'CAPABILITY_UNAVAILABLE', 'Whole-symbol replacement requires a native exact TypeScript source span; use explicit hash-bound edits otherwise');
                        edits = [{ kind: 'replace', path: entity.path, expectedHash: this.workspace.file(snapshot, entity.path).hash, startByte: entity.startByte, endByte: entity.endByte, text: String(replace.text) }];
                    }
                    else
                        edits = args.edits as FileEdit[];
                    data = this.changes.prepare(snapshot, edits, deadline) as unknown as Record<string, unknown>;
                    break;
                }
                case 'ci_change_apply':
                    assert(maxBytes >= 4096, 'BUDGET_TOO_SMALL', 'Change receipts require at least 4096 bytes');
                    data = await this.changes.apply(String(args.planId), String(args.idempotencyKey), (args.mode ?? 'isolated') as 'isolated' | 'workspace', deadline) as unknown as Record<string, unknown>;
                    break;
                case 'ci_verify':
                    data = await this.verifier.verify(snapshot, { planId: args.planId as string | undefined, profiles: args.profiles as string[] | undefined }, deadline) as unknown as Record<string, unknown>;
                    break;
                default: throw new CIError('TOOL_NOT_FOUND', 'Unknown tool');
            }
            const warnings: string[] = [];
            if (snapshot.omissions.length)
                warnings.push(`${snapshot.omissions.length} omission/coverage notices; inspect ci_status mode=index for details.`);
            if (['ci_trace', 'ci_find', 'ci_context'].includes(tool))
                data.coverage ??= this.workspace.store.graphInfo(snapshot.id)?.coverage;
            const semanticFailure = data.status === 'failed' || data.status === 'conflict' || data.status === 'recovery_required' || data.status === 'rolled_back';
            const out = fit(envelope(data, semanticFailure ? 'partial' : 'ok', warnings), this.workspace.store, maxBytes, tokenBudget);
            this.workspace.store.event('request', { requestId, tool, snapshotId, status: out.status, bytes: out.metrics.serializedBytes, durationMs: out.metrics.durationMs, modelCalls: 0, queryHash: args.query ? hash(String(args.query)) : undefined });
            return out;
        }
        catch (error) {
            const out = envelope({}, 'error');
            out.error = { code: error instanceof CIError ? error.code : 'INTERNAL_ERROR', message: (error as Error).message.slice(0, 300) };
            return measure(out);
        }
    }
    async import(backend: 'graft' | 'graphify', snapshotId: string, payload: unknown): Promise<void> { const snapshot = this.workspace.store.snapshot(snapshotId); persistImport(this.workspace, snapshot, importGraph(this.workspace, snapshot, backend, payload)); await this.indexer.build(snapshot, new Deadline(120000)); }
    close(): void { this.indexer.close(); this.workspace.close(); }
}
