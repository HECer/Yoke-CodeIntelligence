import { readFileSync, readdirSync, statSync, lstatSync, openSync, fstatSync, closeSync, constants, existsSync, realpathSync, mkdirSync, chmodSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, relative, dirname } from 'node:path';
import { Store } from '../store/store.js';
import { assert, CIError, hash, stable, Deadline, atomicWrite, integer, object } from './util.js';
import { safePath, normalizePath, blockedPath, glob, language, secretLike, STATE } from './paths.js';
import { VERSION, type Config, type Snapshot, type ManifestFile } from './types.js';
export const DEFAULT_CONFIG: Config = {
    schemaVersion: 1, exclude: ['dist/**', 'build/**', 'coverage/**', 'vendor/**', '*.min.js', '*.map'],
    maxFileBytes: 2000000, maxFiles: 20000, maxSnapshotBytes: 256000000,
    python: process.platform === 'win32' ? 'python' : 'python3', pythonEnabled: true,
    captureAttempts: 3, maxResultBytes: 32768, timeoutMs: 15000, serviceScopes: {}, verify: {},
};
export class Workspace {
    readonly root: string;
    readonly store: Store;
    readonly config: Config;
    readonly configHash: string;
    private dirty = true;
    constructor(root: string, config: Partial<Config> = {}) {
        this.root = realpathSync(root);
        assert(statSync(this.root).isDirectory(), 'INVALID_WORKSPACE', 'Workspace root is not a directory');
        // Repository configuration never implicitly authorizes a process or a write.
        const path = join(this.root, 'code-intelligence.json');
        let disk: Partial<Config> = {};
        if (existsSync(path)) {
            assert(!lstatSync(path).isSymbolicLink(), 'PATH_DENIED', 'Configuration may not be a symlink');
            assert(statSync(path).size < 100000, 'INVALID_CONFIG', 'Configuration is too large');
            disk = object(JSON.parse(readFileSync(path, 'utf8')), 'configuration') as Partial<Config>;
        }
        assert(disk.python === undefined || disk.python === DEFAULT_CONFIG.python || config.python !== undefined, 'UNTRUSTED_EXECUTABLE', 'Repository configuration cannot choose a Python executable. Supply --python explicitly in the trusted host configuration.');
        this.config = { ...DEFAULT_CONFIG, ...disk, ...config };
        assert(this.config.schemaVersion === 1, 'INVALID_CONFIG', 'Unsupported configuration schema');
        assert(Array.isArray(this.config.exclude) && this.config.exclude.every(e => typeof e === 'string' && e.length < 2048), 'INVALID_CONFIG', 'exclude must be a bounded string list');
        for (const [key, min, max] of [['maxFileBytes', 1, 32000000], ['maxFiles', 1, 100000], ['maxSnapshotBytes', 1, 1000000000], ['captureAttempts', 1, 10], ['maxResultBytes', 1024, 4000000], ['timeoutMs', 10, 600000]] as const)
            integer(this.config[key], key, min, max);
        assert(typeof this.config.python === 'string' && typeof this.config.pythonEnabled === 'boolean', 'INVALID_CONFIG', 'Invalid Python settings');
        object(this.config.serviceScopes, 'serviceScopes');
        object(this.config.verify, 'verify');
        const state = join(this.root, STATE);
        if (existsSync(state))
            assert(!lstatSync(state).isSymbolicLink(), 'PATH_DENIED', 'State directory cannot be a symlink');
        this.store = new Store(state);
        const previousRoot = this.store.meta('root');
        assert(!previousRoot || previousRoot === this.root, 'WORKSPACE_MOVED', 'State directory belongs to another root. Export/import immutable artifacts or rebuild the local state.');
        this.store.setMeta('root', this.root);
        const { timeoutMs: _timeout, maxResultBytes: _bytes, ...semanticConfig } = this.config;
        this.configHash = hash(stable({ version: VERSION, config: semanticConfig, compiler: 'typescript@5.8.3', python: 'native-ast-v1' }));
    }
    markDirty(): void { this.dirty = true; }
    private excluded(path: string): boolean { return blockedPath(path) || this.config.exclude.some(p => glob(p).test(path)); }
    private list(): {
        paths: string[];
        omissions: Snapshot['omissions'];
    } {
        const omissions: Snapshot['omissions'] = [];
        let paths: string[];
        try {
            const value = execFileSync('git', ['-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false', '-C', this.root, 'ls-files', '-co', '--exclude-standard', '-z'], { encoding: 'utf8', timeout: 5000, maxBuffer: 32000000, stdio: ['ignore', 'pipe', 'ignore'], env: { PATH: process.env.PATH, SystemRoot: process.env.SystemRoot, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null' } });
            paths = [...new Set(value.split('\0').filter(Boolean))];
        }
        catch {
            assert(!existsSync(join(this.root, '.git')), 'GIT_LISTING_FAILED', 'Git repository listing failed; refusing an unfiltered fallback');
            paths = [];
            const ignores: {
                base: string;
                patterns: RegExp[];
            }[] = [];
            const ignored = (path: string) => ignores.some(entry => {
                if (entry.base && !path.startsWith(entry.base + '/'))
                    return false;
                const local = entry.base ? path.slice(entry.base.length + 1) : path;
                return entry.patterns.some(pattern => pattern.test(local) || local.split('/').some((_p, i, parts) => pattern.test(parts.slice(0, i + 1).join('/'))));
            });
            const visit = (dir: string): void => {
                const ignoreFile = join(dir, '.gitignore');
                if (existsSync(ignoreFile) && !lstatSync(ignoreFile).isSymbolicLink() && statSync(ignoreFile).size < 100000) {
                    const patterns = readFileSync(ignoreFile, 'utf8').split(/\r?\n/).map(s => s.trim()).filter(s => s && !s.startsWith('#') && !s.startsWith('!')).map(s => {
                        const anchored = s.startsWith('/');
                        s = s.replace(/^\//, '').replace(/\/$/, '');
                        return glob(!anchored && !s.includes('/') ? '**/' + s : s);
                    });
                    ignores.push({ base: relative(this.root, dir).replaceAll('\\', '/'), patterns });
                }
                for (const item of readdirSync(dir, { withFileTypes: true })) {
                    const abs = join(dir, item.name), rel = relative(this.root, abs).replaceAll('\\', '/');
                    if (ignored(rel) || this.excluded(rel) || this.config.exclude.some(p => glob(p).test(rel + '/')))
                        continue;
                    if (item.isSymbolicLink()) {
                        omissions.push({ path: rel, reason: 'symlink' });
                        continue;
                    }
                    if (item.isDirectory())
                        visit(abs);
                    else if (item.isFile())
                        paths.push(rel);
                    assert(paths.length <= this.config.maxFiles, 'CAPACITY_EXCEEDED', 'File count limit exceeded; narrow the workspace or excludes');
                }
            };
            visit(this.root);
            omissions.push({ path: '.', reason: 'Non-Git fallback: positive .gitignore patterns are conservative exclusions; negation is intentionally not re-included' });
        }
        paths = paths.filter(p => {
            if (this.excluded(p))
                return false;
            // Git's cached index also lists deleted tracked files. A stable deletion
            // is part of the new snapshot, not an endlessly busy working tree.
            try {
                lstatSync(safePath(this.root, p));
                return true;
            }
            catch (e) {
                if ((e as NodeJS.ErrnoException).code === 'ENOENT')
                    return false;
                return true;
            }
        }).sort();
        assert(paths.length <= this.config.maxFiles, 'CAPACITY_EXCEEDED', 'Workspace exceeds maxFiles');
        return { paths, omissions };
    }
    private stableRead(path: string): {
        bytes: Buffer;
        stamp: string;
        mode: number;
    } {
        const abs = safePath(this.root, path);
        const fd = openSync(abs, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
        try {
            const before = fstatSync(fd);
            assert(before.isFile(), 'UNSUPPORTED_FILE', 'Not a regular file');
            assert(before.size <= this.config.maxFileBytes, 'FILE_TOO_LARGE', 'File exceeds maxFileBytes');
            const bytes = readFileSync(fd);
            const after = fstatSync(fd);
            const stamp = (s: typeof before) => `${s.ino}:${s.size}:${s.mtimeMs}:${s.ctimeMs}:${s.mode}`;
            assert(stamp(before) === stamp(after) && bytes.length === after.size, 'WORKSPACE_BUSY', `File changed while reading: ${path}`);
            return { bytes, stamp: stamp(after), mode: after.mode & 0o777 };
        }
        finally {
            closeSync(fd);
        }
    }
    capture(deadline = new Deadline(60000)): Snapshot {
        for (let attempt = 0; attempt < this.config.captureAttempts; attempt++) {
            deadline.check();
            const before = this.list();
            const files: ManifestFile[] = [];
            const stamps = new Map<string, string>();
            let total = 0;
            try {
                for (const path of before.paths) {
                    deadline.check();
                    let read: ReturnType<Workspace['stableRead']>;
                    try {
                        read = this.stableRead(path);
                    }
                    catch (e) {
                        if (e instanceof CIError && ['PATH_DENIED', 'FILE_TOO_LARGE', 'UNSUPPORTED_FILE'].includes(e.code)) {
                            before.omissions.push({ path, reason: e.code });
                            continue;
                        }
                        throw e;
                    }
                    if (read.bytes.includes(0)) {
                        before.omissions.push({ path, reason: 'binary' });
                        continue;
                    }
                    let content: string;
                    try {
                        content = new TextDecoder('utf-8', { fatal: true }).decode(read.bytes);
                    }
                    catch {
                        before.omissions.push({ path, reason: 'invalid_utf8' });
                        continue;
                    }
                    if (secretLike(content)) {
                        before.omissions.push({ path, reason: 'potential_secret' });
                        continue;
                    }
                    total += read.bytes.length;
                    assert(total <= this.config.maxSnapshotBytes, 'CAPACITY_EXCEEDED', 'Snapshot exceeds maxSnapshotBytes');
                    stamps.set(path, read.stamp);
                    const digest = this.store.putBlob(read.bytes);
                    files.push({ path: normalizePath(path), hash: digest, size: read.bytes.length, mode: read.mode, language: language(path) });
                }
                const after = this.list();
                assert(stable(before.paths) === stable(after.paths), 'WORKSPACE_BUSY', 'File set changed during capture');
                for (const [path, expected] of stamps) {
                    const s = statSync(safePath(this.root, path));
                    assert(`${s.ino}:${s.size}:${s.mtimeMs}:${s.ctimeMs}:${s.mode}` === expected, 'WORKSPACE_BUSY', `File changed during capture: ${path}`);
                }
                const identity = { workspaceId: this.store.workspaceId, projectId: this.store.projectId, configHash: this.configHash, files, omissions: before.omissions, kind: 'disk' as const };
                const result: Snapshot = { ...identity, id: hash(stable(identity)), createdAt: new Date().toISOString(), captureConsistency: 'stable-double-scan' };
                this.store.putSnapshot(result);
                this.store.setMeta('current', result.id);
                this.dirty = false;
                return result;
            }
            catch (e) {
                if ((e instanceof CIError && e.code === 'WORKSPACE_BUSY') || (e as NodeJS.ErrnoException).code === 'ENOENT') {
                    if (attempt + 1 < this.config.captureAttempts)
                        continue;
                    throw new CIError('WORKSPACE_BUSY', 'Workspace changed repeatedly during capture; no new snapshot was published');
                }
                throw e;
            }
        }
        throw new CIError('WORKSPACE_BUSY', 'Unable to capture workspace');
    }
    snapshot(id?: string, deadline?: Deadline): Snapshot { return id ? this.store.snapshot(id) : this.capture(deadline); }
    contents(snapshot: Snapshot): Map<string, string> { return new Map(snapshot.files.map(f => [f.path, this.store.blob(f.hash).toString('utf8')])); }
    file(snapshot: Snapshot, path: string): ManifestFile { const normalized = normalizePath(path); const f = snapshot.files.find(f => f.path === normalized); assert(f, 'FILE_NOT_FOUND', 'File is absent from the snapshot or excluded by policy'); return f; }
    read(snapshot: Snapshot, path: string): Buffer { return this.store.blob(this.file(snapshot, path).hash); }
    derive(base: Snapshot, updates: {
        path: string;
        content: Buffer | null;
        mode?: number;
    }[]): Snapshot {
        const files = new Map(base.files.map(f => [f.path, f]));
        for (const u of updates) {
            safePath(this.root, u.path);
            const path = normalizePath(u.path);
            if (u.content === null)
                files.delete(path);
            else {
                assert(!u.content.includes(0), 'INVALID_EDIT', 'Binary edits are not supported');
                assert(!secretLike(u.content.toString('utf8')), 'SECRET_DENIED', 'Potential secret detected in edited source');
                files.set(path, { path, hash: this.store.putBlob(u.content), size: u.content.length, mode: u.mode ?? files.get(path)?.mode ?? 0o644, language: language(path) });
            }
        }
        assert(files.size <= this.config.maxFiles && [...files.values()].reduce((n, f) => n + f.size, 0) <= this.config.maxSnapshotBytes, 'CAPACITY_EXCEEDED', 'Derived snapshot exceeds capacity');
        const identity = { workspaceId: base.workspaceId, projectId: base.projectId, configHash: base.configHash, kind: 'derived' as const, parentId: base.id, files: [...files.values()].sort((a, b) => a.path < b.path ? -1 : 1), omissions: base.omissions };
        const result: Snapshot = { ...identity, id: hash(stable(identity)), createdAt: new Date().toISOString(), captureConsistency: 'derived-from-immutable' };
        this.store.putSnapshot(result);
        return result;
    }
    materialize(snapshot: Snapshot, destination: string): void {
        assert(!existsSync(destination) || readdirSync(destination).length === 0, 'DESTINATION_NOT_EMPTY', 'Sandbox destination must be empty');
        mkdirSync(destination, { recursive: true, mode: 0o700 });
        for (const f of snapshot.files) {
            const abs = safePath(destination, f.path);
            mkdirSync(dirname(abs), { recursive: true });
            atomicWrite(abs, this.store.blob(f.hash), f.mode);
            if (process.platform !== 'win32')
                chmodSync(abs, f.mode);
        }
    }
    assertCurrent(snapshot: Snapshot, deadline?: Deadline): void {
        assert(snapshot.kind === 'disk', 'SNAPSHOT_STALE', 'A derived snapshot cannot be a workspace edit base');
        const current = this.capture(deadline);
        assert(current.id === snapshot.id, 'SNAPSHOT_STALE', 'Workspace differs from the prepared snapshot; prepare a new plan');
    }
    close(): void { this.store.close(); }
}
