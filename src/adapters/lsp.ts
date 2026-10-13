import { pathToFileURL, fileURLToPath } from 'node:url';
import { relative, resolve } from 'node:path';
import type { Snapshot, FileEdit, SourceRange } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { Deadline, assert, TextIndex, object, stable, CIError } from '../core/util.js';
import { normalizePath, inside } from '../core/paths.js';
import { RpcPeer } from './rpc.js';
export interface LspLocation extends SourceRange {
    path: string;
    contentHash: string;
    selection?: SourceRange;
    positionClamped: boolean;
}
export interface LspLocationResult {
    snapshotId: string;
    operation: 'definition' | 'references' | 'implementation';
    items: LspLocation[];
    external: {
        uri: string;
        reason: string;
    }[];
    serverInfo: Record<string, unknown>;
    exhaustive: false;
    sourceBinding: 'local_ranges_validated_not_upstream_attestation';
}
/** Generic, explicitly host-started LSP process. Does not link Serena application code. */
export class LspAdapter {
    readonly peer = new RpcPeer('lsp');
    private root = '';
    private snapshot?: Snapshot;
    private opened = new Set<string>();
    capabilities: Record<string, unknown> = {};
    serverInfo: Record<string, unknown> = {};
    readonly diagnostics = new Map<string, unknown[]>();
    constructor(readonly workspace: Workspace) {
        this.peer.on('notification', (method: string, params: any) => {
            if (method !== 'textDocument/publishDiagnostics' || !this.opened.has(params?.uri))
                return;
            if (params.version !== undefined && params.version !== null && params.version !== 1)
                return;
            if (Array.isArray(params.diagnostics))
                this.diagnostics.set(params.uri, params.diagnostics.slice(0, 3000));
        });
    }
    async start(command: string, args: string[], snapshot: Snapshot, sandbox: string, deadline: Deadline, options: {
        languages?: string[];
    } = {}): Promise<void> {
        deadline.check();
        this.root = resolve(sandbox);
        this.snapshot = snapshot;
        this.workspace.materialize(snapshot, this.root);
        try {
            this.peer.start(command, args.map(a => a.replaceAll('{workspace}', this.root)), this.root);
            const initialized = object(await this.peer.request('initialize', {
                processId: process.pid, rootUri: pathToFileURL(this.root).href,
                workspaceFolders: [{ uri: pathToFileURL(this.root).href, name: 'immutable-snapshot' }],
                capabilities: { general: { positionEncodings: ['utf-16'] }, textDocument: {
                        synchronization: { didSave: false }, rename: { prepareSupport: true },
                        definition: { linkSupport: true }, implementation: { linkSupport: true }
                    }, workspace: { applyEdit: false, workspaceEdit: { documentChanges: true } } }
            }, deadline));
            this.capabilities = object(initialized.capabilities ?? {});
            this.serverInfo = object(initialized.serverInfo ?? {});
            assert(!this.capabilities.positionEncoding || this.capabilities.positionEncoding === 'utf-16', 'UNSUPPORTED_ENCODING', 'Only negotiated UTF-16 LSP positions are supported');
            this.peer.notify('initialized', {});
            const languages = options.languages ?? ['typescript', 'javascript', 'python', 'go'];
            for (const f of snapshot.files) {
                deadline.check();
                if (!languages.includes(f.language))
                    continue;
                const uri = pathToFileURL(resolve(this.root, f.path)).href;
                this.opened.add(uri);
                const languageId = f.path.endsWith('.tsx') ? 'typescriptreact' : f.path.endsWith('.jsx') ? 'javascriptreact' : f.language;
                this.peer.notify('textDocument/didOpen', { textDocument: { uri, languageId, version: 1, text: this.workspace.read(snapshot, f.path).toString('utf8') } });
            }
        }
        catch (e) {
            this.close();
            throw e;
        }
    }
    private capability(name: string): void {
        const value = this.capabilities[name];
        assert(value === true || !!value && typeof value === 'object' && !Array.isArray(value), 'CAPABILITY_UNAVAILABLE', `Language server does not advertise ${name}`);
    }
    private target(path: string, byte: number): {
        uri: string;
        position: {
            line: number;
            character: number;
        };
    } {
        assert(this.snapshot, 'PROVIDER_UNAVAILABLE', 'LSP has not been initialized');
        const f = this.workspace.file(this.snapshot, path), ti = new TextIndex(this.workspace.read(this.snapshot, path).toString('utf8'));
        const offset = ti.utf16(byte), line = ti.line(offset) - 1;
        const position = { line, character: offset - ti.lineStarts[line]! };
        this.offset(ti, position, true); // Reject positions inside CRLF terminators.
        return { uri: pathToFileURL(resolve(this.root, f.path)).href, position };
    }
    private local(uri: unknown): string | undefined {
        assert(typeof uri === 'string' && uri.length <= 8192, 'ADAPTER_CONTRACT', 'Location URI must be a bounded string');
        let parsed: URL;
        try {
            parsed = new URL(uri);
        }
        catch {
            throw new CIError('ADAPTER_CONTRACT', 'Invalid location URI');
        }
        if (parsed.protocol !== 'file:' || parsed.search || parsed.hash || parsed.username || parsed.password)
            return undefined;
        let abs: string;
        try {
            abs = fileURLToPath(parsed);
        }
        catch {
            return undefined;
        }
        if (!inside(this.root, abs))
            return undefined;
        const relativePath = relative(this.root, abs).replaceAll('\\', '/');
        if (!relativePath)
            return undefined;
        const rel = normalizePath(relativePath);
        return this.snapshot?.files.some(f => f.path === rel) ? rel : undefined;
    }
    /** Location reads may clamp an oversized character as LSP specifies. Edits
     * require canonical exact positions instead of silently changing a patch. */
    private offset(ti: TextIndex, raw: unknown, strict: boolean): {
        byte: number;
        clamped: boolean;
    } {
        const p = object(raw, 'LSP position');
        assert(Number.isSafeInteger(p.line) && Number(p.line) >= 0 && Number(p.line) < ti.lineStarts.length && Number.isSafeInteger(p.character) && Number(p.character) >= 0, 'INVALID_RANGE', 'LSP position outside source');
        const line = Number(p.line), start = ti.lineStarts[line]!;
        let end = line + 1 < ti.lineStarts.length ? ti.lineStarts[line + 1]! : ti.value.length;
        if (end > start && ti.value[end - 1] === '\n')
            end--;
        if (end > start && ti.value[end - 1] === '\r')
            end--;
        const requested = Number(p.character);
        assert(!strict || requested <= end - start, 'INVALID_RANGE', 'LSP edit character exceeds line content');
        const at = start + Math.min(requested, end - start), byte = ti.byte(at);
        assert(ti.utf16(byte) === at, 'INVALID_RANGE', 'LSP position splits a surrogate pair');
        return { byte, clamped: requested > end - start };
    }
    private range(ti: TextIndex, value: unknown, strict: boolean): {
        range: SourceRange;
        clamped: boolean;
    } {
        const r = object(value, 'LSP range'), start = this.offset(ti, r.start, strict), end = this.offset(ti, r.end, strict);
        assert(end.byte >= start.byte, 'INVALID_RANGE', 'Reversed LSP range');
        return { range: { startByte: start.byte, endByte: end.byte, startLine: ti.line(ti.utf16(start.byte)), endLine: ti.line(ti.utf16(end.byte)) }, clamped: start.clamped || end.clamped };
    }
    /** Raw result for protocol-aware consumers. Prefer locations() for source-bound output. */
    async query(operation: 'definition' | 'references' | 'implementation', path: string, byte: number, deadline: Deadline): Promise<unknown> {
        this.capability(`${operation}Provider`);
        const t = this.target(path, byte);
        return this.peer.request(`textDocument/${operation}`, { textDocument: { uri: t.uri }, position: t.position, ...(operation === 'references' ? { context: { includeDeclaration: true } } : {}) }, deadline);
    }
    async locations(operation: 'definition' | 'references' | 'implementation', path: string, byte: number, deadline: Deadline): Promise<LspLocationResult> {
        const raw = await this.query(operation, path, byte, deadline);
        const list = raw === null ? [] : Array.isArray(raw) ? raw : [raw];
        assert(list.length <= 20000, 'PROTOCOL_LIMIT', 'Too many provider locations');
        const result: LspLocationResult = { snapshotId: this.snapshot!.id, operation, items: [], external: [], serverInfo: this.serverInfo, exhaustive: false, sourceBinding: 'local_ranges_validated_not_upstream_attestation' };
        const seen = new Set<string>(), textIndexes = new Map<string, TextIndex>();
        for (const value of list) {
            deadline.check();
            const entry = object(value, 'LSP location');
            const link = Object.hasOwn(entry, 'targetUri');
            const uri = link ? entry.targetUri : entry.uri, local = this.local(uri);
            if (!local) {
                result.external.push({ uri: String(uri), reason: 'outside_captured_source_not_read' });
                continue;
            }
            let ti = textIndexes.get(local);
            if (!ti) {
                ti = new TextIndex(this.workspace.read(this.snapshot!, local).toString('utf8'));
                textIndexes.set(local, ti);
            }
            const r = this.range(ti, link ? entry.targetRange : entry.range, false);
            const selection = link ? this.range(ti, entry.targetSelectionRange, false) : undefined;
            assert(!selection || selection.range.startByte >= r.range.startByte && selection.range.endByte <= r.range.endByte, 'ADAPTER_CONTRACT', 'LocationLink selection is outside target range');
            const item: LspLocation = { path: local, contentHash: this.workspace.file(this.snapshot!, local).hash, ...r.range, ...(selection ? { selection: selection.range } : {}), positionClamped: r.clamped || (selection?.clamped ?? false) };
            const key = stable(item);
            if (seen.has(key))
                continue;
            seen.add(key);
            result.items.push(item);
        }
        return result;
    }
    async rename(path: string, byte: number, newName: string, deadline: Deadline): Promise<FileEdit[]> {
        this.capability('renameProvider');
        assert(typeof newName === 'string' && newName.length > 0 && newName.length <= 256 && !/[\x00-\x1f]/.test(newName), 'INVALID_RENAME', 'Invalid requested name');
        const t = this.target(path, byte);
        if ((this.capabilities.renameProvider as any)?.prepareProvider) {
            const prepared = await this.peer.request('textDocument/prepareRename', { textDocument: { uri: t.uri }, position: t.position }, deadline);
            assert(prepared, 'RENAME_UNAVAILABLE', 'Language server refused prepareRename');
            const p = object(prepared);
            if (p.defaultBehavior !== true) {
                const ti = new TextIndex(this.workspace.read(this.snapshot!, path).toString('utf8'));
                const preparedRange = this.range(ti, p.range ?? p, true).range;
                assert(byte >= preparedRange.startByte && byte <= preparedRange.endByte, 'ADAPTER_CONTRACT', 'prepareRename range does not contain the requested target');
            }
        }
        const value = await this.peer.request('textDocument/rename', { textDocument: { uri: t.uri }, position: t.position, newName }, deadline);
        assert(value !== null, 'RENAME_UNAVAILABLE', 'Language server returned no rename');
        const result = object(value, 'WorkspaceEdit');
        assert(!(result.changes && result.documentChanges), 'ADAPTER_CONTRACT', 'Ambiguous WorkspaceEdit contains both changes and documentChanges');
        const edits: Extract<FileEdit, {
            kind: 'replace';
        }>[] = [];
        let textBytes = 0;
        const append = (uri: unknown, changes: unknown) => {
            const rel = this.local(uri);
            assert(rel, 'SCOPE_VIOLATION', 'LSP edit targets a file outside its immutable snapshot');
            const f = this.workspace.file(this.snapshot!, rel), ti = new TextIndex(this.workspace.read(this.snapshot!, rel).toString('utf8'));
            assert(Array.isArray(changes) && edits.length + changes.length <= 2000, 'ADAPTER_CONTRACT', 'Workspace edits must be a bounded array');
            for (const raw of changes) {
                deadline.check();
                const e = object(raw);
                assert(e.annotationId === undefined, 'CAPABILITY_UNAVAILABLE', 'Annotated edits require explicit annotation review; no change was applied');
                assert(typeof e.newText === 'string', 'ADAPTER_CONTRACT', 'LSP newText must be a string');
                assert(Buffer.from(e.newText, 'utf8').toString('utf8') === e.newText, 'INVALID_EDIT', 'LSP newText contains unpaired surrogates');
                textBytes += Buffer.byteLength(e.newText);
                assert(textBytes <= this.workspace.config.maxSnapshotBytes, 'CAPACITY_EXCEEDED', 'Provider replacement text exceeds workspace budget');
                const r = this.range(ti, e.range, true).range;
                edits.push({ kind: 'replace', path: rel, expectedHash: f.hash, startByte: r.startByte, endByte: r.endByte, text: e.newText });
            }
        };
        if (result.changes)
            for (const [uri, changes] of Object.entries(object(result.changes)))
                append(uri, changes);
        if (result.documentChanges) {
            assert(Array.isArray(result.documentChanges) && result.documentChanges.length <= 2000, 'ADAPTER_CONTRACT', 'documentChanges must be a bounded array');
            for (const raw of result.documentChanges) {
                const change = object(raw);
                assert(!change.kind, 'CAPABILITY_UNAVAILABLE', 'LSP resource operations require a separate explicit file plan');
                const doc = object(change.textDocument);
                assert(doc.version === null || doc.version === undefined || doc.version === 1 && this.opened.has(String(doc.uri)), 'SNAPSHOT_STALE', 'LSP document version differs from the opened immutable source');
                append(doc.uri, change.edits);
            }
        }
        assert(edits.length, 'RENAME_UNAVAILABLE', 'LSP returned no edits');
        const unique = [...new Map(edits.map(e => [stable(e), e])).values()].sort((a, b) => a.path.localeCompare(b.path) || a.startByte - b.startByte || a.endByte - b.endByte);
        for (let i = 1; i < unique.length; i++) {
            const a = unique[i - 1]!, b = unique[i]!;
            assert(a.path !== b.path || a.endByte <= b.startByte && (a.startByte !== b.startByte || a.endByte !== a.startByte), 'OVERLAPPING_EDITS', 'Conflicting or ambiguous provider edits');
        }
        return unique;
    }
    async shutdown(deadline: Deadline): Promise<void> {
        try {
            await this.peer.request('shutdown', null, deadline);
            this.peer.notify('exit');
        }
        finally {
            this.close();
        }
    }
    close(): void { this.peer.close(); }
}
