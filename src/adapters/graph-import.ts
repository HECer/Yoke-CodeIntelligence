import type { Snapshot, Extraction, LocalEntity, Resolution } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { assert, object, text, TextIndex, stable } from '../core/util.js';
import { normalizePath } from '../core/paths.js';
/** Import contracts intentionally reject unbound, unknown-schema, or stale graphs. */
export interface BoundGraph {
    snapshotId: string;
    sourceHashes: Record<string, string>;
    version: string;
    graph: unknown;
}
const confidence: Record<string, Resolution> = { lsp_resolved: 'exact_binding', lsp_dispatch: 'possible_dispatch', extracted: 'syntactic', inferred: 'heuristic', EXTRACTED: 'syntactic', INFERRED: 'heuristic', AMBIGUOUS: 'unresolved' };
export function importGraph(workspace: Workspace, snapshot: Snapshot, backend: 'graft' | 'graphify', input: unknown): Extraction {
    const bound = object(input);
    assert(bound.snapshotId === snapshot.id, 'SNAPSHOT_STALE', 'Imported graph snapshot differs from selected snapshot');
    const hashes = object(bound.sourceHashes, 'sourceHashes');
    const version = text(bound.version, 'version', 100);
    const graph = object(bound.graph, 'graph');
    const producer = `${backend}@${version}`;
    const nodes = graph.nodes;
    assert(Array.isArray(nodes) && nodes.length <= 300000, 'ADAPTER_CONTRACT', 'Graph nodes must be a bounded array');
    if (backend === 'graft')
        assert(object(graph.meta, 'graft.meta').version === 1, 'ADAPTER_CONTRACT', 'Only Graft GraphV1 is supported');
    if (backend === 'graphify') {
        const metadata = object(graph.graph ?? {}, 'graphify.graph');
        assert(metadata.schema_version !== undefined || bound.rawExtraction === true, 'ADAPTER_CONTRACT', 'Graphify persisted graph must advertise graph.schema_version; raw extraction must be explicitly declared');
        assert(metadata.schema_version === undefined || [1, '1', '1.0', '1.0.0'].includes(metadata.schema_version as string | number), 'ADAPTER_CONTRACT', 'Unknown Graphify schema version; pin and validate before import');
    }
    const result: Extraction = { entities: [], observations: [], diagnostics: [], limitations: [`${producer}: binding is checked against declared source hashes, not a cryptographic producer attestation.`, `${producer}: edge anchors are enclosing symbols unless the upstream supplies exact call-site ranges.`] };
    if (backend === 'graphify' && graph.multigraph !== true && bound.rawExtraction !== true)
        result.limitations.push('Graphify simple-graph import may already have collapsed parallel/opposite relationships; lost edges cannot be reconstructed.');
    const mapped = new Map<string, LocalEntity>();
    for (const raw of nodes) {
        const node = object(raw, 'node');
        const originalId = text(node.id, 'node.id', 4096);
        const rawPath = backend === 'graft' ? node.path : node.source_file;
        if (typeof rawPath !== 'string' || !rawPath) {
            result.limitations.push(`${producer}: unanchored external node retained only as unresolved target: ${originalId.slice(0, 160)}`);
            continue;
        }
        const path = normalizePath(rawPath), file = workspace.file(snapshot, path);
        assert(hashes[path] === file.hash, 'SNAPSHOT_STALE', `Source hash missing or stale for ${path}`);
        const ti = new TextIndex(workspace.read(snapshot, path).toString('utf8'));
        const location = String(backend === 'graft' ? node.span ?? '' : node.source_location ?? '');
        const range = /^L(\d+)(?:-L?(\d+))?$/.exec(location);
        const startLine = range ? Number(range[1]) : 1, endLine = range ? Number(range[2] ?? range[1]) : ti.lineStarts.length;
        assert(startLine >= 1 && endLine >= startLine && endLine <= ti.lineStarts.length, 'ADAPTER_CONTRACT', `Invalid upstream line range for ${path}`);
        const startByte = ti.byte(ti.lineStarts[startLine - 1]!), endByte = endLine < ti.lineStarts.length ? ti.byte(ti.lineStarts[endLine]!) : ti.bytes.length;
        const entity: LocalEntity = { key: `${backend}:${originalId}`, path, name: String(node.name ?? node.label ?? originalId).slice(0, 2000), qualifiedName: String(node.name ?? node.label ?? originalId), kind: String(node.kind ?? node.type ?? 'concept'), language: file.language, signature: String(node.signature ?? node.label ?? node.name ?? '').slice(0, 700), producer, startByte, endByte, startLine, endLine, metadata: { upstreamId: originalId, upstreamOrigin: node.origin ?? node._origin ?? 'unknown', anchor: range ? 'enclosing_symbol' : 'file' } };
        assert(!mapped.has(originalId), 'ADAPTER_CONTRACT', 'Duplicate upstream node ID');
        mapped.set(originalId, entity);
        result.entities.push(entity);
    }
    const edges = graph.edges ?? graph.links;
    assert(Array.isArray(edges) && edges.length <= 1000000, 'ADAPTER_CONTRACT', 'Graph edges/links must be a bounded array');
    for (const raw of edges) {
        const edge = object(raw, 'edge');
        // Historical Graphify direction markers take precedence over node-link order.
        const sourceId = String(edge._src ?? edge.source), targetId = String(edge._tgt ?? edge.target);
        const source = mapped.get(sourceId);
        if (!source) {
            result.limitations.push(`${producer}: omitted an edge whose source has no snapshot anchor.`);
            continue;
        }
        const tag = String(edge.confidence ?? 'AMBIGUOUS');
        const target = mapped.get(targetId);
        result.observations.push({ sourceKey: source.key, targetKey: target?.key, targetLabel: target ? undefined : targetId, relation: text(edge.relation ?? 'references', 'relation', 100), resolution: edge._origin === 'semantic' ? 'inferred_concept' : confidence[tag] ?? 'unresolved', path: source.path, producer, anchor: 'enclosing_symbol', startByte: source.startByte, endByte: source.endByte, startLine: source.startLine, endLine: source.endLine });
    }
    result.limitations = [...new Set(result.limitations)].slice(0, 100);
    return result;
}
export function persistImport(workspace: Workspace, snapshot: Snapshot, extraction: Extraction): void {
    const key = `imports:${snapshot.id}`;
    const previous = workspace.store.cache<Extraction[]>(key) ?? [];
    const producer = extraction.entities[0]?.producer;
    workspace.store.setCache(key, [...previous.filter(p => p.entities[0]?.producer !== producer), extraction]);
    workspace.store.db.prepare('DELETE FROM graphs WHERE snapshot=?').run(snapshot.id);
}
export interface SerenaSymbol {
    namePath: string;
    path: string;
    kind?: unknown;
    bodyLocation?: unknown;
    raw: Record<string, unknown>;
}
/** Traverse explicit groups/children; never turn ordered lists into call edges. */
export function normalizeSerenaSymbols(input: unknown): SerenaSymbol[] {
    let value = input;
    if (typeof value === 'string') {
        try {
            value = JSON.parse(value);
        }
        catch {
            throw new Error('Serena response is not structured JSON');
        }
    }
    const out: SerenaSymbol[] = [];
    const seen = new Set<string>();
    let count = 0;
    const walk = (node: unknown, inheritedPath?: string, depth = 0): void => {
        assert(depth < 64 && ++count < 100000, 'ADAPTER_CONTRACT', 'Serena response nesting/size limit exceeded');
        if (Array.isArray(node)) {
            for (const item of node)
                walk(item, inheritedPath, depth + 1);
            return;
        }
        if (!node || typeof node !== 'object')
            return;
        const record = node as Record<string, unknown>;
        const path = typeof record.relative_path === 'string' ? normalizePath(record.relative_path) : inheritedPath;
        if (typeof record.name_path === 'string' && path) {
            const key = stable([path, record.name_path, record.body_location]);
            if (!seen.has(key)) {
                out.push({ namePath: record.name_path, path, kind: record.kind, bodyLocation: record.body_location, raw: record });
                seen.add(key);
            }
        }
        for (const [key, item] of Object.entries(record))
            if (item && typeof item === 'object') {
                let next = path;
                if (!path && /\.[A-Za-z0-9]+$/.test(key)) {
                    try {
                        next = normalizePath(key);
                    }
                    catch { /* not a path-keyed group */ }
                }
                walk(item, next, depth + 1);
            }
    };
    walk(value);
    assert(out.length > 0 || Array.isArray(value) && value.length === 0, 'ADAPTER_CONTRACT', 'Unrecognized Serena symbol response; not silently treated as no matches');
    return out;
}
