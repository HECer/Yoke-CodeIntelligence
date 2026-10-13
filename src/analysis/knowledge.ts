import { randomUUID } from 'node:crypto';
import type { Snapshot } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { assert, stable, tokens, hash, Deadline, object, TextIndex } from '../core/util.js';
export interface Memory {
    id: string;
    kind: 'hypothesis' | 'confirmed' | 'dead_end' | 'correction' | 'rule';
    text: string;
    paths: Record<string, string>;
    createdAt: string;
    supersedes?: string;
    actor: string;
}
export class Knowledge {
    constructor(readonly workspace: Workspace) { }
    remember(snapshot: Snapshot, input: Omit<Memory, 'id' | 'paths' | 'createdAt'> & {
        paths: string[];
    }): Memory {
        assert(['hypothesis', 'confirmed', 'dead_end', 'correction', 'rule'].includes(input.kind), 'INVALID_ARGUMENT', 'Unknown memory kind');
        assert(input.text.length > 0 && input.text.length <= 20000, 'INVALID_ARGUMENT', 'Memory text size invalid');
        const paths = Object.fromEntries(input.paths.map(p => { const f = this.workspace.file(snapshot, p); return [f.path, f.hash]; }));
        if (input.supersedes)
            assert(this.workspace.store.db.prepare('SELECT id FROM memories WHERE id=?').get(input.supersedes), 'MEMORY_NOT_FOUND', 'Superseded memory does not exist');
        const result: Memory = { ...input, paths, id: randomUUID(), createdAt: new Date().toISOString() };
        this.workspace.store.db.prepare('INSERT INTO memories VALUES(?,?,?)').run(result.id, stable(result), result.createdAt);
        return result;
    }
    recall(snapshot: Snapshot, query: string, limit = 20): Record<string, unknown>[] {
        const rows = this.workspace.store.db.prepare('SELECT json FROM memories ORDER BY created DESC LIMIT 10000').all() as {
            json: string;
        }[];
        const terms = tokens(query);
        const memories = rows.map(r => JSON.parse(r.json) as Memory);
        const superseded = new Set(memories.map(m => m.supersedes).filter(Boolean));
        return memories.map(m => { const lower = m.text.toLocaleLowerCase('en'); const score = terms.filter(t => lower.includes(t)).length; const stale = Object.entries(m.paths).some(([path, digest]) => snapshot.files.find(f => f.path === path)?.hash !== digest); return { ...m, score, stale, superseded: superseded.has(m.id), trust: 'authored_data_not_execution_authority' }; }).filter(m => m.score > 0 || !query).sort((a, b) => b.score - a.score).slice(0, limit);
    }
    importVectors(snapshot: Snapshot, input: unknown): {
        count: number;
        model: string;
        dimension: number;
    } {
        const data = object(input);
        assert(data.snapshotId === snapshot.id, 'SNAPSHOT_STALE', 'Embedding snapshot mismatch');
        assert(typeof data.model === 'string' && data.model.length < 200, 'INVALID_ARGUMENT', 'Embedding model identifier required');
        assert(Array.isArray(data.items) && data.items.length <= 100000, 'INVALID_ARGUMENT', 'Embedding items must be bounded');
        let dimension: number | undefined;
        const normalized: {
            id: string;
            vector: number[];
        }[] = [];
        for (const raw of data.items) {
            const item = object(raw);
            assert(typeof item.entityId === 'string', 'INVALID_ARGUMENT', 'Embedding entityId required');
            const entity = this.workspace.store.entity(item.entityId, snapshot.id);
            assert(item.contentHash === this.workspace.file(snapshot, entity.path).hash, 'SNAPSHOT_STALE', 'Embedding source hash mismatch');
            const v = item.vector;
            assert(Array.isArray(v) && v.length > 0 && v.length <= 4096 && v.every(n => typeof n === 'number' && Number.isFinite(n)), 'INVALID_VECTOR', 'Embedding must contain finite numbers');
            dimension ??= v.length;
            assert(v.length === dimension, 'INVALID_VECTOR', 'Mixed embedding dimensions');
            const norm = Math.hypot(...v);
            assert(norm > 0 && Number.isFinite(norm), 'INVALID_VECTOR', 'Zero or overflowed vector');
            normalized.push({ id: entity.id, vector: v.map(n => n / norm) });
        }
        assert(dimension, 'INVALID_VECTOR', 'No embedding vectors');
        this.workspace.store.transaction(() => {
            for (const item of normalized) {
                const id = hash(stable([snapshot.id, data.model, item.id]));
                this.workspace.store.db.prepare('INSERT OR REPLACE INTO vectors VALUES(?,?,?,?,?)').run(id, snapshot.id, data.model as string, dimension!, stable({ entityId: item.id, vector: item.vector }));
            }
        });
        return { count: normalized.length, model: data.model as string, dimension };
    }
    searchVector(snapshot: Snapshot, model: string, vector: number[], limit: number, deadline: Deadline): Record<string, unknown>[] {
        assert(vector.length > 0 && vector.length <= 4096 && vector.every(Number.isFinite), 'INVALID_VECTOR', 'Query vector invalid');
        const norm = Math.hypot(...vector);
        assert(norm > 0 && Number.isFinite(norm), 'INVALID_VECTOR', 'Query vector has invalid norm');
        const query = vector.map(n => n / norm);
        const rows = this.workspace.store.db.prepare('SELECT json,dimension FROM vectors WHERE snapshot=? AND model=?').all(snapshot.id, model) as {
            json: string;
            dimension: number;
        }[];
        const results: Record<string, unknown>[] = [];
        for (const row of rows) {
            deadline.check();
            assert(row.dimension === vector.length, 'INVALID_VECTOR', 'Query and model dimension differ');
            const item = JSON.parse(row.json);
            const score = query.reduce((n, v, i) => n + v * item.vector[i], 0);
            results.push({ entity: this.workspace.store.entity(item.entityId, snapshot.id), score, model });
        }
        results.sort((a, b) => Number(b.score) - Number(a.score));
        return results.slice(0, limit);
    }
    importLcov(snapshot: Snapshot, lcov: string, hashes: Record<string, string>): Record<string, unknown> {
        assert(lcov.length < 20000000, 'CAPACITY_EXCEEDED', 'Coverage artifact too large');
        let path: string | undefined;
        const files: Record<string, unknown>[] = [];
        let lines: Record<string, number> = {};
        const flush = () => {
            if (!path)
                return;
            const file = this.workspace.file(snapshot, path);
            assert(hashes[path] === file.hash, 'SNAPSHOT_STALE', 'LCOV needs a matching source hash for every file');
            const max = new TextIndex(this.workspace.read(snapshot, path).toString('utf8')).lineStarts.length;
            assert(Object.keys(lines).every(n => Number(n) >= 1 && Number(n) <= max), 'INVALID_COVERAGE', 'Coverage line outside source');
            files.push({ path, contentHash: file.hash, lines: { ...lines } });
        };
        for (const line of lcov.split(/\r?\n/)) {
            if (line.startsWith('SF:')) {
                flush();
                path = line.slice(3).replaceAll('\\', '/');
                lines = {};
            }
            else if (line.startsWith('DA:')) {
                const match = /^DA:(\d+),(\d+)(?:,.*)?$/.exec(line);
                assert(match, 'INVALID_COVERAGE', 'Malformed DA coverage record');
                lines[match[1]!] = Number(match[2]);
            }
            else if (line === 'end_of_record') {
                flush();
                path = undefined;
                lines = {};
            }
        }
        flush();
        assert(files.length, 'INVALID_COVERAGE', 'No file coverage records');
        const data = { format: 'lcov', snapshotId: snapshot.id, files, provenance: 'declared_runtime_artifact', limitations: ['Aggregate LCOV does not identify individual tests or prove complete path coverage.'] };
        const artifactId = this.workspace.store.artifact(snapshot.id, data);
        this.workspace.store.setCache(`coverage:${snapshot.id}`, { ...data, artifactId });
        return { artifactId, files: files.length };
    }
    importRuntime(snapshot: Snapshot, input: unknown): Record<string, unknown> {
        const data = object(input);
        assert(data.snapshotId === snapshot.id, 'SNAPSHOT_STALE', 'Runtime snapshot mismatch');
        assert(Array.isArray(data.spans) && data.spans.length <= 100000, 'INVALID_RUNTIME', 'Runtime spans must be bounded');
        const ids = new Set<string>();
        const spans: Record<string, unknown>[] = data.spans.map(raw => { const span = object(raw); assert(typeof span.id === 'string' && !ids.has(span.id), 'INVALID_RUNTIME', 'Unique span IDs required'); ids.add(span.id); assert(typeof span.path === 'string', 'INVALID_RUNTIME', 'Span requires source path'); const f = this.workspace.file(snapshot, span.path); assert(span.contentHash === f.hash, 'SNAPSHOT_STALE', 'Runtime source hash mismatch'); assert(Number.isSafeInteger(span.line) && Number(span.line) > 0 && Number(span.line) <= new TextIndex(this.workspace.read(snapshot, f.path).toString('utf8')).lineStarts.length, 'INVALID_RUNTIME', 'Runtime line outside source'); return { ...span, path: f.path }; });
        for (const s of spans)
            if (s.parentId !== undefined)
                assert(typeof s.parentId === 'string' && ids.has(s.parentId) && s.parentId !== s.id, 'INVALID_RUNTIME', 'Missing or self-referential parent span');
        const parents = new Map(spans.map(s => [String(s.id), s.parentId as string | undefined]));
        const finished = new Set<string>();
        for (const id of ids) {
            let node: string | undefined = id;
            const chain = new Set<string>();
            while (node !== undefined && !finished.has(node)) {
                assert(!chain.has(node), 'INVALID_RUNTIME', 'Runtime parent graph contains a cycle');
                chain.add(node);
                node = parents.get(node);
            }
            for (const visited of chain)
                finished.add(visited);
        }
        const artifact = { format: 'ci-runtime-v1', snapshotId: snapshot.id, spans, relation: 'observed_execution', scope: 'this_recorded_execution_only' };
        return { artifactId: this.workspace.store.artifact(snapshot.id, artifact), spans: spans.length };
    }
    importSarif(snapshot: Snapshot, input: unknown, hashes: Record<string, string>): Record<string, unknown> {
        const data = object(input);
        assert(data.version === '2.1.0' && Array.isArray(data.runs), 'INVALID_SARIF', 'Expected SARIF 2.1.0 runs');
        const findings: unknown[] = [];
        for (const run of data.runs) {
            const r = object(run);
            for (const raw of (r.results ?? []) as unknown[]) {
                const finding = object(raw);
                for (const location of (finding.locations ?? []) as any[]) {
                    const uri = location?.physicalLocation?.artifactLocation?.uri;
                    if (typeof uri !== 'string')
                        continue;
                    const path = decodeURIComponent(uri);
                    const f = this.workspace.file(snapshot, path);
                    assert(hashes[f.path] === f.hash, 'SNAPSHOT_STALE', 'SARIF source hash mismatch');
                    findings.push({ ruleId: finding.ruleId, level: finding.level, message: finding.message, path: f.path, region: location.physicalLocation.region, producer: (r.tool as any)?.driver?.name, classification: 'static_analysis_finding_not_formal_proof' });
                }
            }
        }
        const artifactId = this.workspace.store.artifact(snapshot.id, { format: 'sarif', findings });
        return { artifactId, findings: findings.length };
    }
}
