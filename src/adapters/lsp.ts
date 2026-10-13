import { pathToFileURL, fileURLToPath } from 'node:url';
import { relative, resolve } from 'node:path';
import type { Snapshot, FileEdit } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { Deadline, assert, TextIndex, object } from '../core/util.js';
import { normalizePath, inside } from '../core/paths.js';
import { RpcPeer } from './rpc.js';
/** Generic installed LSP provider. No Serena application code is linked. */
export class LspAdapter {
    readonly peer = new RpcPeer('lsp');
    private root = '';
    private snapshot?: Snapshot;
    capabilities: Record<string, unknown> = {};
    readonly diagnostics = new Map<string, unknown[]>();
    constructor(readonly workspace: Workspace) {
        this.peer.on('notification', (method: string, params: any) => {
            if (method === 'textDocument/publishDiagnostics' && typeof params?.uri === 'string')
                this.diagnostics.set(params.uri, params.diagnostics ?? []);
        });
    }
    async start(command: string, args: string[], snapshot: Snapshot, sandbox: string, deadline: Deadline): Promise<void> {
        this.root = resolve(sandbox);
        this.snapshot = snapshot;
        this.workspace.materialize(snapshot, this.root);
        this.peer.start(command, args.map(a => a.replaceAll('{workspace}', this.root)), this.root);
        const initialized = object(await this.peer.request('initialize', { processId: process.pid, rootUri: pathToFileURL(this.root).href, workspaceFolders: [{ uri: pathToFileURL(this.root).href, name: 'immutable-snapshot' }], capabilities: { general: { positionEncodings: ['utf-16'] }, textDocument: { synchronization: { didSave: false }, rename: { prepareSupport: true } }, workspace: { applyEdit: false, workspaceEdit: { documentChanges: true } } } }, deadline));
        this.capabilities = object(initialized.capabilities ?? {});
        assert(!this.capabilities.positionEncoding || this.capabilities.positionEncoding === 'utf-16', 'UNSUPPORTED_ENCODING', 'Only negotiated UTF-16 LSP positions are supported');
        this.peer.notify('initialized', {});
        for (const f of snapshot.files)
            if (['typescript', 'javascript', 'python', 'go'].includes(f.language))
                this.peer.notify('textDocument/didOpen', { textDocument: { uri: pathToFileURL(resolve(this.root, f.path)).href, languageId: f.language, version: 1, text: this.workspace.read(snapshot, f.path).toString('utf8') } });
    }
    private target(path: string, byte: number): {
        uri: string;
        position: {
            line: number;
            character: number;
        };
    } {
        assert(this.snapshot, 'PROVIDER_UNAVAILABLE', 'LSP has not been initialized');
        const f = this.workspace.file(this.snapshot, path);
        const ti = new TextIndex(this.workspace.read(this.snapshot, path).toString('utf8'));
        const offset = ti.utf16(byte), line = ti.line(offset) - 1;
        return { uri: pathToFileURL(resolve(this.root, f.path)).href, position: { line, character: offset - ti.lineStarts[line]! } };
    }
    async query(operation: 'definition' | 'references' | 'implementation', path: string, byte: number, deadline: Deadline): Promise<unknown> {
        const t = this.target(path, byte);
        return this.peer.request(`textDocument/${operation}`, { textDocument: { uri: t.uri }, position: t.position, ...(operation === 'references' ? { context: { includeDeclaration: true } } : {}) }, deadline);
    }
    async rename(path: string, byte: number, newName: string, deadline: Deadline): Promise<FileEdit[]> {
        assert(this.capabilities.renameProvider, 'CAPABILITY_UNAVAILABLE', 'Language server does not advertise rename');
        const t = this.target(path, byte);
        const result = object(await this.peer.request('textDocument/rename', { textDocument: { uri: t.uri }, position: t.position, newName }, deadline), 'WorkspaceEdit');
        const edits: FileEdit[] = [];
        const append = (uri: string, changes: unknown) => {
            assert(uri.startsWith('file:'), 'SCOPE_VIOLATION', 'Only local file URI edits are allowed');
            const abs = fileURLToPath(uri);
            assert(inside(this.root, abs), 'SCOPE_VIOLATION', 'LSP edit escapes snapshot');
            const rel = normalizePath(relative(this.root, abs).replaceAll('\\', '/'));
            const f = this.workspace.file(this.snapshot!, rel);
            const ti = new TextIndex(this.workspace.read(this.snapshot!, rel).toString('utf8'));
            assert(Array.isArray(changes), 'ADAPTER_CONTRACT', 'Workspace edits must be an array');
            const offset = (p: any) => { assert(Number.isInteger(p?.line) && Number.isInteger(p?.character) && p.line >= 0 && p.line < ti.lineStarts.length && p.character >= 0, 'INVALID_RANGE', 'Invalid LSP position'); const at = ti.lineStarts[p.line]! + p.character; const end = p.line + 1 < ti.lineStarts.length ? ti.lineStarts[p.line + 1]! : ti.value.length; assert(at <= end, 'INVALID_RANGE', 'LSP character exceeds line'); const byte = ti.byte(at); assert(ti.utf16(byte) === at, 'INVALID_RANGE', 'LSP splits a surrogate pair'); return byte; };
            for (const raw of changes) {
                const e = object(raw);
                const range = object(e.range);
                assert(typeof e.newText === 'string', 'ADAPTER_CONTRACT', 'LSP newText must be a string');
                edits.push({ kind: 'replace', path: rel, expectedHash: f.hash, startByte: offset(range.start), endByte: offset(range.end), text: e.newText });
            }
        };
        if (result.changes)
            for (const [uri, changes] of Object.entries(object(result.changes)))
                append(uri, changes);
        if (result.documentChanges) {
            assert(Array.isArray(result.documentChanges), 'ADAPTER_CONTRACT', 'documentChanges must be an array');
            for (const raw of result.documentChanges) {
                const change = object(raw);
                assert(!change.kind, 'CAPABILITY_UNAVAILABLE', 'LSP resource create/delete/rename operations require a separate explicit file plan');
                const doc = object(change.textDocument);
                assert(doc.version === null || doc.version === undefined || doc.version === 1, 'SNAPSHOT_STALE', 'LSP document version differs from snapshot');
                append(String(doc.uri), change.edits);
            }
        }
        assert(edits.length, 'RENAME_UNAVAILABLE', 'LSP returned no edits');
        return edits;
    }
    close(): void { this.peer.close(); }
}
