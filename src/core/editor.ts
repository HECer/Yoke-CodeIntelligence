import { randomUUID } from 'node:crypto';
import { Workspace } from './workspace.js';
import { assert, Deadline, hash, stable, TextIndex, integer } from './util.js';
import { safePath, normalizePath, secretLike, glob } from './paths.js';
import type { Snapshot } from './types.js';
interface BufferEntry {
    path: string;
    version: number;
    hash: string;
    baseHash: string | null;
}
interface Session {
    id: string;
    baseSnapshotId: string;
    revision: number;
    buffers: Record<string, BufferEntry>;
    updatedAt: string;
    lastSnapshotId?: string;
}
/** Full-text, monotonically versioned editor overlays. Never writes working-tree files. */
export class EditorBuffers {
    constructor(readonly workspace: Workspace) {
        workspace.store.db.exec('CREATE TABLE IF NOT EXISTS editor_sessions(id TEXT PRIMARY KEY, json TEXT NOT NULL)');
    }
    create(base = this.workspace.capture(new Deadline(120000))): Session {
        assert(base.workspaceId === this.workspace.store.workspaceId && base.kind === 'disk', 'SCOPE_VIOLATION', 'Editor sessions need a disk snapshot in this workspace');
        const s: Session = { id: randomUUID(), baseSnapshotId: base.id, revision: 0, buffers: {}, updatedAt: new Date().toISOString() };
        this.save(s);
        return s;
    }
    session(id: string): Session {
        assert(typeof id === 'string' && id.length <= 100, 'INVALID_ARGUMENT', 'Invalid editor session');
        const row = this.workspace.store.db.prepare('SELECT json FROM editor_sessions WHERE id=?').get(id) as {
            json: string;
        } | undefined;
        assert(row, 'SESSION_NOT_FOUND', 'Editor session is missing or expired');
        return JSON.parse(row.json);
    }
    private save(s: Session): void {
        this.workspace.store.db.prepare('INSERT INTO editor_sessions VALUES(?,?) ON CONFLICT(id) DO UPDATE SET json=excluded.json').run(s.id, stable(s));
    }
    update(id: string, value: {
        path: string;
        version: number;
        text: string;
        expectedVersion?: number;
    }): Record<string, unknown> {
        const path = normalizePath(value.path);
        safePath(this.workspace.root, path);
        assert(!this.workspace.config.exclude.some(p => glob(p).test(path)), 'PATH_DENIED', 'Excluded file cannot be added through an editor overlay');
        integer(value.version, 'version', 0, 2147483647);
        assert(typeof value.text === 'string' && !value.text.includes('\0') && !secretLike(value.text), 'INVALID_BUFFER', 'Binary or potential secret text refused');
        new TextIndex(value.text); // validates UTF-16 surrogate boundaries
        const bytes = Buffer.from(value.text, 'utf8');
        assert(bytes.toString('utf8') === value.text, 'INVALID_BUFFER', 'Unpaired surrogate in buffer');
        assert(bytes.length <= this.workspace.config.maxFileBytes, 'CAPACITY_EXCEEDED', 'Editor buffer too large');
        const digest = hash(bytes);
        return this.workspace.store.transaction(() => {
            const s = this.session(id), prior = s.buffers[path];
            if (prior && prior.version === value.version && prior.hash === digest)
                return { sessionId: id, revision: s.revision, idempotent: true };
            assert(!prior || value.version > prior.version, 'BUFFER_VERSION_CONFLICT', 'Editor buffer version is not newer');
            assert(value.expectedVersion === undefined || prior?.version === value.expectedVersion, 'BUFFER_VERSION_CONFLICT', 'Editor expected version does not match');
            assert(prior || Object.keys(s.buffers).length < Math.min(1000, this.workspace.config.maxFiles), 'CAPACITY_EXCEEDED', 'Too many open editor buffers');
            const base = this.workspace.store.snapshot(s.baseSnapshotId), file = base.files.find(f => f.path === path);
            assert(!file?.readOnly, 'READ_ONLY_DEPENDENCY', 'Dependencies cannot be edited');
            assert(file || !base.omissions.some(o => o.path === path), 'PATH_DENIED', 'Omitted source cannot be restored through an editor buffer');
            s.buffers[path] = { path, version: value.version, hash: this.workspace.store.putBlob(bytes), baseHash: prior ? prior.baseHash : file?.hash ?? null };
            s.revision++;
            s.updatedAt = new Date().toISOString();
            this.save(s);
            return { sessionId: id, revision: s.revision, version: value.version, contentHash: digest };
        });
    }
    closeFile(id: string, path: string, expectedVersion?: number): void {
        path = normalizePath(path);
        this.workspace.store.transaction(() => {
            const s = this.session(id), prior = s.buffers[path];
            assert(expectedVersion === undefined || prior?.version === expectedVersion, 'BUFFER_VERSION_CONFLICT', 'Cannot close a newer editor buffer');
            delete s.buffers[path];
            s.revision++;
            s.updatedAt = new Date().toISOString();
            this.save(s);
        });
    }
    capture(id: string, deadline = new Deadline(120000)): Snapshot {
        const before = this.session(id), disk = this.workspace.capture(deadline);
        const updates = Object.values(before.buffers).map(b => {
            const current = disk.files.find(f => f.path === b.path)?.hash ?? null;
            assert(current === b.baseHash || current === b.hash, 'BUFFER_BASE_CONFLICT', `Disk changed independently of editor buffer: ${b.path}`);
            return { path: b.path, content: this.workspace.store.blob(b.hash) };
        });
        const derived = this.workspace.derive(disk, updates);
        const { id: _id, createdAt: _time, ...body } = derived;
        const identity = { ...body, kind: 'overlay' as const, overlay: { sessionId: id, revision: before.revision, versions: Object.fromEntries(Object.values(before.buffers).map(b => [b.path, b.version])) } };
        const result: Snapshot = { ...identity, id: hash(stable(identity)), createdAt: new Date().toISOString() };
        this.workspace.store.transaction(() => {
            const after = this.session(id);
            assert(after.revision === before.revision, 'BUFFER_VERSION_CONFLICT', 'Editor changed while capturing overlay');
            this.workspace.store.putSnapshot(result);
            after.lastSnapshotId = result.id;
            // A saved buffer can become the new baseline for a later unsaved update.
            for (const b of Object.values(after.buffers))
                if (disk.files.find(f => f.path === b.path)?.hash === b.hash)
                    b.baseHash = b.hash;
            after.baseSnapshotId = disk.id;
            this.save(after);
        });
        return result;
    }
    close(id: string): void { this.workspace.store.db.prepare('DELETE FROM editor_sessions WHERE id=?').run(id); }
}
