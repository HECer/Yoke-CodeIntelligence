import { Workspace } from '../core/workspace.js';
import { Deadline, TextIndex, hash, stable, unique, assert } from '../core/util.js';
import { TypeScriptProject } from './typescript.js';
import { extractPython } from './python.js';
import { extractGo } from './go.js';
import { extractContracts } from './contracts.js';
import { extractDocuments } from './documents.js';
import type { Snapshot, Graph, Entity, Evidence, Observation, Extraction, LocalEntity } from '../core/types.js';
const INDEX_REVISION = 'ci-index@0.3.0-contracts-2';
export class Indexer {
    private project?: TypeScriptProject;
    private running = new Map<string, Promise<void>>();
    constructor(readonly workspace: Workspace) { }
    typescript(snapshot: Snapshot, deadline = new Deadline(120000)): TypeScriptProject {
        if (this.project?.snapshot.id !== snapshot.id) {
            this.project?.close();
            this.project = new TypeScriptProject(snapshot, this.workspace.contents(snapshot), this.workspace.root, deadline);
        }
        return this.project;
    }
    async ensure(snapshot: Snapshot, deadline: Deadline): Promise<void> {
        if (this.workspace.store.graphInfo(snapshot.id) && this.workspace.store.meta(`index-revision:${snapshot.id}`) === INDEX_REVISION)
            return;
        // Shared builds use the first caller's bounded deadline; no caller can silently extend it.
        const pending = this.running.get(snapshot.id);
        if (pending) {
            await pending;
            deadline.check();
            return;
        }
        const task = this.build(snapshot, deadline);
        this.running.set(snapshot.id, task);
        try {
            await task;
        }
        finally {
            this.running.delete(snapshot.id);
        }
    }
    async build(snapshot: Snapshot, deadline: Deadline): Promise<void> {
        deadline.check();
        const contents = this.workspace.contents(snapshot);
        const pieces: Extraction[] = [];
        const files: LocalEntity[] = snapshot.files.map(f => {
            const source = contents.get(f.path)!;
            const ti = new TextIndex(source);
            return { key: `file:${f.path}`, path: f.path, name: f.path.split('/').at(-1)!, qualifiedName: f.path, kind: 'file', language: f.language, signature: source.split('\n').find(s => s.trim())?.slice(0, 300) ?? '', producer: 'ci-snapshot@0.1.0', ...ti.range(0, source.length) };
        });
        pieces.push({ entities: files, observations: [], diagnostics: [], limitations: [] });
        if (snapshot.files.some(f => ['typescript', 'javascript'].includes(f.language))) {
            const project = this.typescript(snapshot, deadline);
            pieces.push(project.extract(deadline, { get: key => this.workspace.store.cache<Extraction>(key), put: (key, value) => this.workspace.store.setCache(key, value) }));
            this.workspace.store.event('semantic-cache', { snapshotId: snapshot.id, ...project.cacheStats });
        }
        try {
            pieces.push(await extractPython(this.workspace, snapshot, deadline));
        }
        catch (e) {
            deadline.check();
            pieces.push({ entities: [], observations: [], diagnostics: [], limitations: [`Python AST unavailable: ${(e as Error).message}. Python files remain searchable; no Python call graph claimed.`] });
        }
        try {
            pieces.push(await extractGo(this.workspace, snapshot, deadline));
        }
        catch (e) {
            deadline.check();
            pieces.push({ entities: [], observations: [], diagnostics: [], limitations: ['Go AST unavailable: ' + (e as Error).message] });
        }
        pieces.push(extractDocuments(snapshot, contents, this.workspace.config.serviceScopes));
        pieces.push(extractContracts(snapshot, contents, this.workspace.config.serviceScopes, deadline));
        const imports = this.workspace.store.cache<Extraction[]>(`imports:${snapshot.id}`) ?? [];
        pieces.push(...imports);
        const graph = canonicalize(snapshot, pieces);
        for (const f of snapshot.files)
            if (!['typescript', 'javascript', 'python', 'go', 'document', 'config', 'sql', 'text'].includes(f.language))
                graph.coverage.limitations.push(`${f.language}: built-in semantic/AST provider unavailable; file and text search available, external graph import supported.`);
        deadline.check();
        this.workspace.store.putGraph(graph);
        this.workspace.store.setMeta(`index-revision:${snapshot.id}`, INDEX_REVISION);
        this.workspace.store.event('index', { snapshotId: snapshot.id, files: snapshot.files.length, entities: graph.entities.length, observations: graph.observations.length, durationMs: deadline.elapsed, modelCalls: 0 });
    }
    close(): void { this.project?.close(); }
}
export function canonicalize(snapshot: Snapshot, extractions: Extraction[]): Graph {
    const nodes = new Map<string, Entity>(), keys = new Map<string, string>();
    const manifest = new Map(snapshot.files.map(f => [f.path, f]));
    const observations = new Map<string, Observation>(), evidence = new Map<string, Evidence>();
    const makeId = (key: string) => hash(stable({ snapshot: snapshot.id, key }));
    for (const x of extractions)
        for (const e of x.entities) {
            const id = makeId(e.key);
            keys.set(e.key, id);
            const previous = nodes.get(id);
            assert(!previous || stable({ ...previous, id: undefined, snapshotId: undefined }) === stable(e), 'IDENTITY_COLLISION', 'Different entities share a canonical key');
            nodes.set(id, { ...e, id, snapshotId: snapshot.id });
        }
    for (const x of extractions)
        for (const edge of x.observations) {
            const source = keys.get(edge.sourceKey);
            assert(source, 'INVALID_GRAPH', 'Unknown source entity');
            const file = manifest.get(edge.path);
            assert(file, 'INVALID_GRAPH', 'Observation must be anchored in an indexed source file');
            assert(edge.startByte >= 0 && edge.endByte >= edge.startByte && edge.endByte <= file.size, 'INVALID_GRAPH', 'Observation range lies outside its immutable source');
            const e: Evidence = { id: '', snapshotId: snapshot.id, path: edge.path, contentHash: file.hash, producer: edge.producer, anchor: edge.anchor ?? 'exact', trust: 'source-data', startByte: edge.startByte, endByte: edge.endByte, startLine: edge.startLine, endLine: edge.endLine };
            e.id = hash(stable({ ...e, id: undefined }));
            evidence.set(e.id, e);
            const target = edge.targetKey ? keys.get(edge.targetKey) ?? null : null;
            const o: Observation = { id: '', snapshotId: snapshot.id, source, target, targetLabel: target ? undefined : edge.targetLabel ?? edge.targetKey, relation: edge.relation, resolution: target ? edge.resolution : 'unresolved', evidenceIds: [e.id], producer: edge.producer, scope: edge.path.split('/').slice(0, -1).join('/'), ...(edge.analysisContext ? { analysisContext: edge.analysisContext } : {}) };
            o.id = hash(stable({ ...o, id: undefined }));
            observations.set(o.id, o);
        }
    const languages: Graph['coverage']['languages'] = {};
    const semanticPaths = new Set(extractions.flatMap(x => x.analysisContexts ?? []).flatMap(c => c.analyzedFiles));
    for (const file of snapshot.files) {
        languages[file.language] ??= { files: 0, provider: ['typescript', 'javascript'].includes(file.language) ? 'typescript-compiler@5.8.3' : file.language === 'python' && extractions.some(x => x.entities.some(e => e.language === 'python' && e.kind !== 'file')) ? 'python-ast' : file.language === 'go' && extractions.some(x => x.entities.some(e => e.language === 'go' && e.kind !== 'file')) ? 'go-parser' : 'text/contracts', semantic: ['typescript', 'javascript'].includes(file.language) };
        languages[file.language]!.files++;
        if (['typescript', 'javascript'].includes(file.language)) {
            languages[file.language]!.semanticFiles = (languages[file.language]!.semanticFiles ?? 0) + (semanticPaths.has(file.path) ? 1 : 0);
            languages[file.language]!.semantic = languages[file.language]!.semanticFiles! > 0;
        }
    }
    const diagnostics = extractions.flatMap(x => x.diagnostics);
    return { snapshotId: snapshot.id, entities: [...nodes.values()], evidence: [...evidence.values()], observations: [...observations.values()], diagnostics,
        coverage: { analysisContexts: extractions.flatMap(x => x.analysisContexts ?? []), indexedFiles: snapshot.files.length, sourceFiles: snapshot.files.filter(f => !['document', 'config', 'text'].includes(f.language)).length, languages, unresolved: [...observations.values()].filter(e => e.target === null).length, omissions: snapshot.omissions.length, diagnostics: diagnostics.length, exhaustive: false, limitations: unique([snapshot.kind === 'overlay' ? 'Includes explicitly versioned editor buffers; other editor sessions are not implied.' : 'Only saved, captured files are known. Use an editor overlay snapshot for unsaved buffers.', 'A stable double scan detects ordinary concurrent edits; it is not an atomic filesystem-wide snapshot against a hostile concurrent writer.', ...extractions.flatMap(x => x.limitations)]) } };
}
