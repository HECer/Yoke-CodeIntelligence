import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, readFileSync, existsSync, lstatSync, renameSync } from 'node:fs';
import { join } from 'node:path';
import { randomBytes, randomUUID } from 'node:crypto';
import { hostname } from 'node:os';
import { assert, atomicWrite, hash, stable, CursorCodec, tokens } from '../core/util.js';
import { SCHEMA, type Snapshot, type Graph, type Entity, type Observation, type Evidence } from '../core/types.js';
/** One local writer; immutable source blobs and snapshot-bound observations. */
export class Store {
    readonly db: DatabaseSync;
    readonly cursor: CursorCodec;
    readonly workspaceId: string;
    readonly projectId: string;
    private closed = false;
    readonly clientId = randomUUID();
    constructor(readonly directory: string) {
        if (existsSync(directory))
            assert(!lstatSync(directory).isSymbolicLink(), 'PATH_DENIED', 'State directory cannot be a symbolic link');
        mkdirSync(directory, { recursive: true, mode: 0o700 });
        for (const name of ['blobs', 'trash', 'changes', 'logs', 'workers', 'index.sqlite', 'index.sqlite-wal', 'index.sqlite-shm']) {
            try {
                assert(!lstatSync(join(directory, name)).isSymbolicLink(), 'PATH_DENIED', 'State files and directories cannot be symbolic links');
            }
            catch (e) {
                if ((e as NodeJS.ErrnoException).code !== 'ENOENT')
                    throw e;
            }
        }
        for (const sub of ['blobs', 'trash', 'changes', 'logs'])
            mkdirSync(join(directory, sub), { recursive: true, mode: 0o700 });
        this.db = new DatabaseSync(join(directory, 'index.sqlite'));
        this.db.exec('PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=FULL;');
        this.db.exec('CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)');
        const schema = this.meta('schema');
        assert(!schema || Number(schema) === SCHEMA, 'SCHEMA_MISMATCH', `Unsupported database schema ${schema}; back up and rebuild the index.`);
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS clients(id TEXT PRIMARY KEY, pid INTEGER NOT NULL, host TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS pins(id TEXT PRIMARY KEY, snapshot TEXT NOT NULL, reason TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS snapshots(id TEXT PRIMARY KEY, workspace TEXT NOT NULL, json TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS graphs(snapshot TEXT PRIMARY KEY REFERENCES snapshots(id), coverage TEXT NOT NULL, diagnostics TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS entities(id TEXT PRIMARY KEY, snapshot TEXT NOT NULL REFERENCES snapshots(id), path TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, json TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS entities_snapshot ON entities(snapshot, path);
      CREATE INDEX IF NOT EXISTS entities_name ON entities(snapshot, name);
      CREATE VIRTUAL TABLE IF NOT EXISTS entity_fts USING fts5(id UNINDEXED, snapshot UNINDEXED, name, qualified, signature, path, tokenize='unicode61');
      CREATE TABLE IF NOT EXISTS evidence(id TEXT PRIMARY KEY, snapshot TEXT NOT NULL REFERENCES snapshots(id), json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS observations(id TEXT PRIMARY KEY, snapshot TEXT NOT NULL REFERENCES snapshots(id), source TEXT NOT NULL REFERENCES entities(id), target TEXT REFERENCES entities(id), relation TEXT NOT NULL, resolution TEXT NOT NULL, json TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS observations_source ON observations(snapshot, source, relation);
      CREATE INDEX IF NOT EXISTS observations_target ON observations(snapshot, target, relation);
      CREATE TABLE IF NOT EXISTS parse_cache(key TEXT PRIMARY KEY, json TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS artifacts(id TEXT PRIMARY KEY, snapshot TEXT, json TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS plans(id TEXT PRIMARY KEY, json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS approvals(plan TEXT PRIMARY KEY REFERENCES plans(id), digest TEXT NOT NULL, allow_test INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS receipts(key TEXT PRIMARY KEY, plan TEXT NOT NULL, json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS journals(id TEXT PRIMARY KEY, json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS memories(id TEXT PRIMARY KEY, json TEXT NOT NULL, created TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS vectors(id TEXT PRIMARY KEY, snapshot TEXT NOT NULL, model TEXT NOT NULL, dimension INTEGER NOT NULL, json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS attempts(id TEXT PRIMARY KEY, task TEXT NOT NULL, state TEXT NOT NULL, reserved REAL NOT NULL, actual REAL, json TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS attempts_task ON attempts(task);
      CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT NOT NULL, json TEXT NOT NULL, created TEXT NOT NULL);
    `);
        const identity = this.transaction(() => {
            this.setMeta('schema', String(SCHEMA));
            const workspace = this.meta('workspace') ?? randomUUID();
            this.setMeta('workspace', workspace);
            const project = this.meta('project') ?? randomUUID();
            this.setMeta('project', project);
            const secret = this.meta('cursor_secret') ?? randomBytes(32).toString('hex');
            this.setMeta('cursor_secret', secret);
            return { workspace, project, secret };
        });
        this.db.prepare('INSERT INTO clients VALUES(?,?,?,?)').run(this.clientId, process.pid, hostname(), new Date().toISOString());
        this.workspaceId = identity.workspace;
        this.projectId = identity.project;
        this.cursor = new CursorCodec(Buffer.from(identity.secret, 'hex'));
    }
    meta(key: string): string | undefined {
        return (this.db.prepare('SELECT value FROM meta WHERE key=?').get(key) as {
            value: string;
        } | undefined)?.value;
    }
    setMeta(key: string, value: string): void { this.db.prepare('INSERT INTO meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value').run(key, value); }
    transaction<T>(fn: () => T): T {
        this.db.exec('BEGIN IMMEDIATE');
        try {
            const result = fn();
            this.db.exec('COMMIT');
            return result;
        }
        catch (e) {
            this.db.exec('ROLLBACK');
            throw e;
        }
    }
    putBlob(bytes: Buffer | string): string {
        const digest = hash(bytes);
        const path = this.blobPath(digest);
        if (!existsSync(path))
            atomicWrite(path, bytes);
        else
            assert(hash(readFileSync(path)) === digest, 'CORRUPT_BLOB', 'Existing source blob failed integrity validation');
        return digest;
    }
    blobPath(digest: string): string {
        assert(/^[a-f0-9]{64}$/.test(digest), 'INVALID_DIGEST', 'Expected SHA-256 digest');
        for (const part of [join(this.directory, 'blobs'), join(this.directory, 'blobs', digest.slice(0, 2)), join(this.directory, 'blobs', digest.slice(0, 2), digest.slice(2))]) {
            try {
                assert(!lstatSync(part).isSymbolicLink(), 'PATH_DENIED', 'Content-addressed storage may not follow symbolic links');
            }
            catch (e) {
                if ((e as NodeJS.ErrnoException).code !== 'ENOENT')
                    throw e;
            }
        }
        return join(this.directory, 'blobs', digest.slice(0, 2), digest.slice(2));
    }
    blob(digest: string): Buffer {
        const path = this.blobPath(digest), trash = join(this.directory, 'trash', digest);
        assert(!lstatSync(join(this.directory, 'trash')).isSymbolicLink(), 'PATH_DENIED', 'Trash root cannot be a symlink');
        if (!existsSync(path) && existsSync(trash)) {
            assert(!lstatSync(trash).isSymbolicLink() && hash(readFileSync(trash)) === digest, 'CORRUPT_BLOB', 'Quarantined blob failed integrity check');
            mkdirSync(join(this.directory, 'blobs', digest.slice(0, 2)), { recursive: true, mode: 0o700 });
            renameSync(trash, path);
        }
        assert(existsSync(path), 'MISSING_BLOB', 'Source blob is missing');
        const b = readFileSync(path);
        assert(hash(b) === digest, 'CORRUPT_BLOB', 'Source blob failed SHA-256 integrity check');
        return b;
    }
    putSnapshot(value: Snapshot): void { this.db.prepare('INSERT OR IGNORE INTO snapshots VALUES(?,?,?,?)').run(value.id, value.workspaceId, stable(value), value.createdAt); }
    snapshot(id: string): Snapshot {
        const row = this.db.prepare('SELECT json FROM snapshots WHERE id=? AND workspace=?').get(id, this.workspaceId) as {
            json: string;
        } | undefined;
        assert(row, 'SNAPSHOT_NOT_FOUND', 'Snapshot not found in this workspace');
        return JSON.parse(row.json) as Snapshot;
    }
    snapshots(limit = 20): Snapshot[] {
        return (this.db.prepare('SELECT json FROM snapshots ORDER BY created DESC LIMIT ?').all(limit) as {
            json: string;
        }[]).map(r => JSON.parse(r.json));
    }
    putGraph(graph: Graph): void {
        // Foreign keys and endpoint checks prevent silently materialized phantom nodes.
        assert(graph.entities.every(n => n.snapshotId === graph.snapshotId) && graph.observations.every(n => n.snapshotId === graph.snapshotId) && graph.evidence.every(n => n.snapshotId === graph.snapshotId), 'INVALID_GRAPH', 'Mixed snapshot identities are forbidden');
        assert(new Set(graph.entities.map(n => n.id)).size === graph.entities.length, 'INVALID_GRAPH', 'Duplicate canonical entity IDs');
        const ids = new Set(graph.entities.map(n => n.id));
        const evidence = new Set(graph.evidence.map(e => e.id));
        for (const o of graph.observations) {
            assert(ids.has(o.source) && (o.target === null || ids.has(o.target)), 'INVALID_GRAPH', 'Observation endpoint missing');
            assert(o.evidenceIds.length && o.evidenceIds.every(e => evidence.has(e)), 'INVALID_GRAPH', 'Observation has missing evidence');
        }
        this.transaction(() => {
            this.db.prepare('DELETE FROM observations WHERE snapshot=?').run(graph.snapshotId);
            this.db.prepare('DELETE FROM evidence WHERE snapshot=?').run(graph.snapshotId);
            this.db.prepare('DELETE FROM entity_fts WHERE snapshot=?').run(graph.snapshotId);
            this.db.prepare('DELETE FROM entities WHERE snapshot=?').run(graph.snapshotId);
            const node = this.db.prepare('INSERT INTO entities VALUES(?,?,?,?,?,?)');
            const fts = this.db.prepare('INSERT INTO entity_fts VALUES(?,?,?,?,?,?)');
            for (const n of graph.entities) {
                node.run(n.id, n.snapshotId, n.path, n.name, n.kind, stable(n));
                fts.run(n.id, n.snapshotId, n.name + ' ' + tokens(n.name).join(' '), n.qualifiedName + ' ' + tokens(n.qualifiedName).join(' '), n.signature, n.path);
            }
            const ev = this.db.prepare('INSERT OR REPLACE INTO evidence VALUES(?,?,?)');
            for (const e of graph.evidence)
                ev.run(e.id, e.snapshotId, stable(e));
            const edge = this.db.prepare('INSERT OR REPLACE INTO observations VALUES(?,?,?,?,?,?,?)');
            for (const e of graph.observations)
                edge.run(e.id, e.snapshotId, e.source, e.target, e.relation, e.resolution, stable(e));
            this.db.prepare('INSERT INTO graphs VALUES(?,?,?,?) ON CONFLICT(snapshot) DO UPDATE SET coverage=excluded.coverage,diagnostics=excluded.diagnostics,created=excluded.created').run(graph.snapshotId, stable(graph.coverage), stable(graph.diagnostics), new Date().toISOString());
        });
    }
    graph(id: string): Graph {
        const info = this.graphInfo(id);
        assert(info, 'INDEX_UNAVAILABLE', 'Snapshot has not been indexed');
        const evidence = (this.db.prepare('SELECT json FROM evidence WHERE snapshot=? ORDER BY id').all(id) as {
            json: string;
        }[]).map(r => JSON.parse(r.json) as Evidence);
        return { snapshotId: id, entities: this.entities(id), observations: this.observations(id), evidence, ...info };
    }
    graphInfo(id: string): Pick<Graph, 'coverage' | 'diagnostics'> | undefined {
        const row = this.db.prepare('SELECT coverage,diagnostics FROM graphs WHERE snapshot=?').get(id) as {
            coverage: string;
            diagnostics: string;
        } | undefined;
        return row ? { coverage: JSON.parse(row.coverage), diagnostics: JSON.parse(row.diagnostics) } : undefined;
    }
    entities(snapshot: string, path?: string): Entity[] {
        const rows = path === undefined ? this.db.prepare('SELECT json FROM entities WHERE snapshot=? ORDER BY path,id').all(snapshot) : this.db.prepare('SELECT json FROM entities WHERE snapshot=? AND path=? ORDER BY id').all(snapshot, path);
        return (rows as {
            json: string;
        }[]).map(r => JSON.parse(r.json));
    }
    entity(id: string, snapshot: string): Entity {
        const row = this.db.prepare('SELECT json FROM entities WHERE id=? AND snapshot=?').get(id, snapshot) as {
            json: string;
        } | undefined;
        assert(row, 'SYMBOL_NOT_FOUND', 'Symbol is missing or belongs to another snapshot');
        return JSON.parse(row.json);
    }
    observations(snapshot: string, entity?: string, direction: 'in' | 'out' | 'both' = 'both'): Observation[] {
        let query = 'SELECT json FROM observations WHERE snapshot=?';
        const params = [snapshot];
        if (entity) {
            query += direction === 'in' ? ' AND target=?' : direction === 'out' ? ' AND source=?' : ' AND (source=? OR target=?)';
            params.push(entity);
            if (direction === 'both')
                params.push(entity);
        }
        return (this.db.prepare(query + ' ORDER BY id').all(...params) as {
            json: string;
        }[]).map(r => JSON.parse(r.json));
    }
    evidence(ids: string[], snapshot: string): Evidence[] {
        const stmt = this.db.prepare('SELECT json FROM evidence WHERE id=? AND snapshot=?');
        return ids.map(id => {
            const row = stmt.get(id, snapshot) as {
                json: string;
            } | undefined;
            assert(row, 'EVIDENCE_NOT_FOUND', 'Evidence not found in snapshot');
            return JSON.parse(row.json) as Evidence;
        });
    }
    cache<T>(key: string): T | undefined {
        const row = this.db.prepare('SELECT json FROM parse_cache WHERE key=?').get(key) as {
            json: string;
        } | undefined;
        return row ? JSON.parse(row.json) : undefined;
    }
    setCache(key: string, value: unknown): void { this.db.prepare('INSERT OR REPLACE INTO parse_cache VALUES(?,?,?)').run(key, stable(value), new Date().toISOString()); }
    artifact(snapshotId: string | null, value: unknown): string { const json = stable(value), id = hash(json); this.db.prepare('INSERT OR IGNORE INTO artifacts VALUES(?,?,?,?)').run(id, snapshotId, json, new Date().toISOString()); return id; }
    readArtifact(id: string): {
        snapshot: string | null;
        value: unknown;
    } {
        const row = this.db.prepare('SELECT snapshot,json FROM artifacts WHERE id=?').get(id) as {
            snapshot: string | null;
            json: string;
        } | undefined;
        assert(row, 'ARTIFACT_NOT_FOUND', 'Artifact not found');
        return { snapshot: row.snapshot, value: JSON.parse(row.json) };
    }
    event(type: string, value: unknown): void { this.db.prepare('INSERT INTO events(type,json,created) VALUES(?,?,?)').run(type, stable(value), new Date().toISOString()); }
    stats(): Record<string, number> {
        const tables = ['snapshots', 'entities', 'observations', 'evidence', 'parse_cache', 'artifacts', 'plans', 'memories', 'vectors'];
        return Object.fromEntries(tables.map(t => [t, (this.db.prepare(`SELECT count(*) AS n FROM ${t}`).get() as {
                n: number;
            }).n]));
    }
    close(): void {
        if (!this.closed) {
            this.db.prepare('DELETE FROM clients WHERE id=?').run(this.clientId);
            this.db.close();
            this.closed = true;
        }
    }
}
