import type { Snapshot, Entity, Observation } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { Deadline, assert, tokens, TextIndex, isTestPath, hash, stable } from '../core/util.js';
import { normalizePath } from '../core/paths.js';
export class QueryEngine {
    constructor(readonly workspace: Workspace) { }
    find(snapshot: Snapshot, query: string, options: {
        kind?: string;
        path?: string;
        limit?: number;
        offset?: number;
    }, deadline: Deadline): {
        items: unknown[];
        total: number;
        candidateLimitReached?: boolean;
    } {
        const terms = tokens(query), kind = options.kind ?? 'symbol';
        const limit = options.limit ?? 20, offset = options.offset ?? 0;
        const prefix = options.path ? normalizePath(options.path) : undefined;
        assert(query.length > 0 && terms.length > 0, 'INVALID_ARGUMENT', 'Search query must contain searchable characters');
        if (kind === 'text') {
            const hits: unknown[] = [];
            let total = 0;
            for (const f of snapshot.files) {
                deadline.check();
                if (prefix && f.path !== prefix && !f.path.startsWith(prefix + '/'))
                    continue;
                const value = this.workspace.store.blob(f.hash).toString('utf8');
                const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu');
                for (const match of value.matchAll(pattern)) {
                    deadline.check();
                    const at = match.index!;
                    if (total >= offset && hits.length < limit) {
                        const ti = new TextIndex(value);
                        const start = value.lastIndexOf('\n', at - 1) + 1;
                        const end = value.indexOf('\n', at);
                        hits.push({ path: f.path, contentHash: f.hash, excerpt: value.slice(start, end < 0 ? value.length : end).slice(0, 1500), ...ti.range(at, at + match[0].length) });
                    }
                    total++;
                    if (total >= 100000)
                        return { items: hits, total, candidateLimitReached: true };
                }
            }
            return { items: hits, total };
        }
        const fts = terms.map(t => '"' + t.replaceAll('"', '""') + '"*').join(' OR ');
        // SQL parameters separate user text from FTS syntax and workspace identity.
        let scopeSql = '';
        const params: (string | number)[] = [fts, snapshot.id];
        if (prefix) {
            scopeSql += ' AND (entities.path=? OR substr(entities.path,1,?)=?)';
            params.push(prefix, prefix.length + 1, prefix + '/');
        }
        if (kind === 'symbol')
            scopeSql += " AND entities.kind NOT IN ('file','concept','api_contract','configuration_key','rationale')";
        else if (kind !== 'all') {
            scopeSql += ' AND entities.kind=?';
            params.push(kind);
        }
        const rows = this.workspace.store.db.prepare(`SELECT entities.json AS json, bm25(entity_fts,0,0,8,5,1,2) AS rank FROM entity_fts JOIN entities ON entities.id=entity_fts.id WHERE entity_fts MATCH ? AND entity_fts.snapshot=?${scopeSql} ORDER BY rank LIMIT 5000`).all(...params) as {
            json: string;
            rank: number;
        }[];
        const ranked = rows.map(row => {
            const entity = JSON.parse(row.json) as Entity;
            const name = entity.name.toLocaleLowerCase('en');
            let score = -row.rank;
            if (name === query.toLocaleLowerCase('en'))
                score += 100;
            if (entity.qualifiedName.toLocaleLowerCase('en') === query.toLocaleLowerCase('en'))
                score += 120;
            if (isTestPath(entity.path) && !/test|spec/i.test(query))
                score *= 0.8;
            return { entity, score };
        }).filter(({ entity: e }) => (!prefix || e.path === prefix || e.path.startsWith(prefix + '/')) && (kind === 'all' || kind === 'file' ? kind === 'all' || e.kind === 'file' : kind === 'symbol' ? !['file', 'concept', 'api_contract', 'configuration_key', 'rationale'].includes(e.kind) : e.kind === kind));
        ranked.sort((a, b) => b.score - a.score || a.entity.id.localeCompare(b.entity.id));
        return { items: ranked.slice(offset, offset + limit).map(({ entity, score }) => ({ ...entity, score: Math.round(score * 1e6) / 1e6, contentHash: this.workspace.file(snapshot, entity.path).hash })), total: ranked.length, candidateLimitReached: rows.length === 5000 };
    }
    read(snapshot: Snapshot, input: {
        path?: string;
        entityId?: string;
        startLine?: number;
        endLine?: number;
        startByte?: number;
        endByte?: number;
    }): Record<string, unknown> {
        assert(!(input.entityId && (input.path !== undefined || input.startByte !== undefined || input.endByte !== undefined || input.startLine !== undefined || input.endLine !== undefined)), 'INVALID_RANGE', 'Entity reads cannot be combined with a second path or explicit range');
        assert(!((input.startByte !== undefined || input.endByte !== undefined) && (input.startLine !== undefined || input.endLine !== undefined)), 'INVALID_RANGE', 'Choose byte ranges or line ranges, not both');
        let path = input.path;
        let startByte = input.startByte, endByte = input.endByte;
        if (input.entityId) {
            const e = this.workspace.store.entity(input.entityId, snapshot.id);
            path = e.path;
            startByte = e.startByte;
            endByte = e.endByte;
        }
        assert(path, 'INVALID_ARGUMENT', 'Provide path or entityId');
        const file = this.workspace.file(snapshot, path);
        const bytes = this.workspace.read(snapshot, path);
        const ti = new TextIndex(bytes.toString('utf8'));
        if (input.startLine !== undefined || input.endLine !== undefined) {
            const start = input.startLine ?? 1, end = input.endLine ?? Math.min(start + 99, ti.lineStarts.length);
            assert(start >= 1 && end >= start && start <= ti.lineStarts.length && end <= ti.lineStarts.length, 'INVALID_RANGE', 'Line range is outside the file');
            startByte = ti.byte(ti.lineStarts[start - 1]!);
            endByte = end < ti.lineStarts.length ? ti.byte(ti.lineStarts[end]!) : bytes.length;
        }
        startByte ??= 0;
        endByte ??= bytes.length;
        assert(ti.isBoundary(startByte) && ti.isBoundary(endByte) && endByte >= startByte, 'INVALID_RANGE', 'Source range must use valid UTF-8 boundaries');
        return { path: file.path, contentHash: file.hash, startByte, endByte, startLine: ti.line(ti.utf16(startByte)), endLine: ti.line(ti.utf16(endByte)), text: bytes.subarray(startByte, endByte).toString('utf8'), trust: 'untrusted_source_data', snapshotId: snapshot.id };
    }
    trace(snapshot: Snapshot, startIds: string[], options: {
        direction?: 'in' | 'out' | 'both';
        relations?: string[];
        resolutions?: string[];
        analysisContexts?: string[];
        depth?: number;
        maxVisited?: number;
    }, deadline: Deadline): Record<string, unknown> {
        const max = options.maxVisited ?? 300, depth = options.depth ?? 2;
        const queue = startIds.map(id => ({ id, depth: 0 }));
        const seen = new Set<string>(), observations = new Map<string, Observation>();
        let truncated = false;
        while (queue.length) {
            deadline.check();
            const item = queue.shift()!;
            if (seen.has(item.id))
                continue;
            if (seen.size >= max) {
                truncated = true;
                break;
            }
            this.workspace.store.entity(item.id, snapshot.id);
            seen.add(item.id);
            if (item.depth >= depth)
                continue;
            for (const edge of this.workspace.store.observations(snapshot.id, item.id, options.direction ?? 'out')) {
                if (options.relations?.length && !options.relations.includes(edge.relation))
                    continue;
                if (options.analysisContexts?.length && (!edge.analysisContext || !options.analysisContexts.includes(edge.analysisContext)))
                    continue;
                if (options.resolutions?.length && !options.resolutions.includes(edge.resolution))
                    continue;
                if (observations.size >= max * 10) {
                    truncated = true;
                    break;
                }
                observations.set(edge.id, edge);
                const next = edge.source === item.id ? edge.target : edge.source;
                if (next && !seen.has(next))
                    queue.push({ id: next, depth: item.depth + 1 });
            }
        }
        const edges = [...observations.values()].filter(e => seen.has(e.source) && (e.target === null || seen.has(e.target)));
        if (edges.length !== observations.size)
            truncated = true;
        const evidenceIds = [...new Set(edges.flatMap(e => e.evidenceIds))];
        const analysisContexts = [...new Set(edges.map(e => e.analysisContext).filter(Boolean))].sort();
        return { items: [...seen].map(id => this.workspace.store.entity(id, snapshot.id)), edges, evidence: this.workspace.store.evidence(evidenceIds, snapshot.id), traversalComplete: !truncated, analysisContexts, crossContextUnion: analysisContexts.length > 1, depthLimit: depth, visited: seen.size, unresolved: edges.filter(e => e.target === null).length };
    }
    context(snapshot: Snapshot, query: string, deadline: Deadline, limit = 8): Record<string, unknown> {
        const found = this.find(snapshot, query, { kind: 'all', limit }, deadline);
        const entities = found.items as Entity[];
        const delivered = new Set<string>();
        const items: Record<string, unknown>[] = [];
        for (const e of entities) {
            deadline.check();
            const source = this.read(snapshot, { entityId: e.id });
            const identity = stable([source.contentHash, source.startByte, source.endByte]);
            if (delivered.has(identity))
                continue;
            delivered.add(identity);
            // Large bodies stay exact and paginatable via ci_read, never silently cut.
            if (Buffer.byteLength(String(source.text)) > 6000) {
                const { text: _text, ...anchor } = source;
                items.push({ entity: e, source: anchor, bodyOmitted: true, readWith: { tool: 'ci_read', entityId: e.id, snapshotId: snapshot.id } });
            }
            else
                items.push({ entity: e, source });
        }
        const paths = new Set(entities.map(e => e.path));
        const tests = snapshot.files.filter(f => isTestPath(f.path) && ([...paths].some(p => p.replace(/\.[^.]+$/, '').split('/').at(-1) && f.path.includes(p.replace(/\.[^.]+$/, '').split('/').at(-1)!)))).map(f => f.path);
        return { items, suggestedTests: tests.slice(0, 20), testSelection: tests.length ? 'filename_convention_candidates' : 'unknown_use_broader_tests', coverage: this.workspace.store.graphInfo(snapshot.id)?.coverage, totalCandidates: found.total };
    }
    plan(snapshot: Snapshot, steps: Record<string, unknown>[], deadline: Deadline): Record<string, unknown> {
        assert(steps.length > 0 && steps.length <= 16, 'INVALID_PLAN', 'Read plan needs 1–16 steps');
        let current: unknown[] = [];
        for (const step of steps) {
            deadline.check();
            const op = step.op;
            if (op === 'find')
                current = this.find(snapshot, String(step.query ?? ''), { kind: String(step.kind ?? 'symbol'), limit: Math.min(100, Number(step.limit ?? 20)) }, deadline).items;
            else if (op === 'limit') {
                assert(Number.isInteger(step.count) && Number(step.count) >= 0 && Number(step.count) <= 100, 'INVALID_PLAN', 'Invalid limit');
                current = current.slice(0, Number(step.count));
            }
            else if (op === 'filter') {
                assert(['path', 'kind', 'name', 'language'].includes(String(step.field)), 'INVALID_PLAN', 'Invalid filter field');
                current = current.filter(v => String((v as Record<string, unknown>)[String(step.field)] ?? '').includes(String(step.includes ?? '')));
            }
            else if (op === 'read')
                current = current.slice(0, 20).map(v => { const e = v as Record<string, unknown>; return this.read(snapshot, typeof e.id === 'string' ? { entityId: e.id } : { path: String(e.path) }); });
            else if (op === 'references') {
                const ids = current.slice(0, 20).map(v => String((v as Record<string, unknown>).id));
                current = this.trace(snapshot, ids, { direction: 'in', relations: ['references', 'calls', 'constructs'], depth: 1, maxVisited: 200 }, deadline).items as unknown[];
            }
            else if (op === 'select') {
                assert(Array.isArray(step.fields) && step.fields.every(f => typeof f === 'string' && !['__proto__', 'constructor', 'prototype'].includes(f)), 'INVALID_PLAN', 'Invalid select fields');
                current = current.map(v => Object.fromEntries((step.fields as string[]).filter(k => Object.hasOwn(v as object, k)).map(k => [k, (v as Record<string, unknown>)[k]])));
            }
            else if (op === 'group') {
                const field = String(step.field);
                assert(['path', 'kind', 'language'].includes(field), 'INVALID_PLAN', 'Invalid group field');
                const groups = new Map<string, unknown[]>();
                for (const item of current) {
                    const key = String((item as Record<string, unknown>)[field] ?? 'unknown');
                    const group = groups.get(key) ?? [];
                    group.push(item);
                    groups.set(key, group);
                }
                current = [...groups].map(([key, items]) => ({ key, items }));
            }
            else
                throw new Error(`Unsupported read-only operation: ${String(op)}`);
            assert(current.length <= 1000, 'PLAN_CAPACITY', 'Read plan output limit exceeded');
        }
        return { items: current, steps: steps.length, intermediatesReturned: false };
    }
}
