import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { Deadline, CIError, assert } from '../core/util.js';
import { killTree } from '../core/process.js';
/** Minimal bounded JSON-RPC transport, shared by optional MCP and LSP peers. */
export class RpcPeer extends EventEmitter {
    private child?: ChildProcessWithoutNullStreams;
    private buffer = Buffer.alloc(0);
    private next = 1;
    private dead = false;
    private pending = new Map<number, {
        resolve: (value: unknown) => void;
        reject: (error: Error) => void;
        cleanup: () => void;
    }>();
    constructor(readonly framing: 'lsp' | 'ndjson', readonly maxMessageBytes = 8000000) { super(); }
    start(command: string, args: string[], cwd: string): void {
        assert(!this.child, 'PEER_STARTED', 'Peer already started');
        this.child = spawn(command, args, { cwd, shell: false, detached: process.platform !== 'win32', stdio: ['pipe', 'pipe', 'pipe'], env: { PATH: process.env.PATH ?? '', SystemRoot: process.env.SystemRoot ?? '', PYTHONNOUSERSITE: '1', PYTHONDONTWRITEBYTECODE: '1', NO_COLOR: '1' } });
        this.child.stdout.on('data', (data: Buffer) => {
            try {
                this.ingest(data);
            }
            catch (e) {
                this.fail(e as Error);
            }
        });
        this.child.stderr.on('data', (data: Buffer) => this.emit('log', data.toString('utf8').slice(0, 4000)));
        this.child.stdin.on('error', () => { });
        this.child.on('error', error => this.fail(new CIError('PROCESS_UNAVAILABLE', error.message)));
        this.child.on('close', () => this.fail(new CIError('PEER_CLOSED', 'Provider process closed')));
    }
    private ingest(data: Buffer): void {
        this.buffer = Buffer.concat([this.buffer, data]);
        assert(this.buffer.length <= this.maxMessageBytes + 16384, 'PROTOCOL_LIMIT', 'Provider output buffer exceeded limit');
        while (this.buffer.length) {
            let body: Buffer;
            if (this.framing === 'ndjson') {
                const newline = this.buffer.indexOf(10);
                if (newline < 0)
                    return;
                body = this.buffer.subarray(0, newline);
                this.buffer = this.buffer.subarray(newline + 1);
                if (!body.length)
                    continue;
            }
            else {
                const end = this.buffer.indexOf('\r\n\r\n');
                if (end < 0) {
                    assert(this.buffer.length < 8192, 'PROTOCOL_LIMIT', 'LSP header too large');
                    return;
                }
                const header = this.buffer.subarray(0, end).toString('ascii');
                const lengths = [...header.matchAll(/^Content-Length:\s*(\d+)\s*$/gim)];
                assert(lengths.length === 1, 'PROTOCOL_ERROR', 'LSP requires exactly one Content-Length');
                const length = Number(lengths[0]![1]);
                assert(length <= this.maxMessageBytes, 'PROTOCOL_LIMIT', 'LSP body too large');
                if (this.buffer.length < end + 4 + length)
                    return;
                body = this.buffer.subarray(end + 4, end + 4 + length);
                this.buffer = this.buffer.subarray(end + 4 + length);
            }
            const message = JSON.parse(body.toString('utf8')) as Record<string, unknown>;
            assert(message.jsonrpc === '2.0', 'PROTOCOL_ERROR', 'Expected JSON-RPC 2.0');
            if (typeof message.method === 'string') {
                if (message.id !== undefined) {
                    // No peer is allowed to edit the workspace or acquire credentials via callbacks.
                    const result = message.method === 'workspace/configuration' ? (Array.isArray((message.params as any)?.items) ? (message.params as any).items.map(() => null) : []) : message.method === 'workspace/applyEdit' ? { applied: false, failureReason: 'Writes must be prepared through Code Intelligence' } : null;
                    this.send({ jsonrpc: '2.0', id: message.id, result });
                }
                else
                    this.emit('notification', message.method, message.params);
            }
            else if (typeof message.id === 'number') {
                const pending = this.pending.get(message.id);
                if (!pending)
                    continue;
                this.pending.delete(message.id);
                pending.cleanup();
                if (message.error)
                    pending.reject(new CIError('PROVIDER_ERROR', String((message.error as any).message ?? 'Provider error')));
                else
                    pending.resolve(message.result);
            }
        }
    }
    private send(value: unknown): void {
        assert(this.child && !this.dead, 'PEER_CLOSED', 'Provider is not running');
        const json = JSON.stringify(value);
        assert(Buffer.byteLength(json) <= this.maxMessageBytes, 'PROTOCOL_LIMIT', 'Outbound provider message too large');
        this.child.stdin.write(this.framing === 'ndjson' ? json + '\n' : `Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);
    }
    notify(method: string, params?: unknown): void { this.send({ jsonrpc: '2.0', method, params }); }
    request(method: string, params: unknown, deadline: Deadline): Promise<unknown> {
        deadline.check();
        const id = this.next++;
        return new Promise((resolve, reject) => {
            const abort = () => {
                try {
                    this.notify('$/cancelRequest', { id });
                }
                catch { }
                this.fail(new CIError('CANCELLED', 'Provider request cancelled'));
            };
            const timer = setTimeout(() => this.fail(new CIError('DEADLINE_EXCEEDED', 'Provider exceeded shared deadline')), deadline.remaining);
            const cleanup = () => { clearTimeout(timer); deadline.signal?.removeEventListener('abort', abort); };
            this.pending.set(id, { resolve, reject, cleanup });
            deadline.signal?.addEventListener('abort', abort, { once: true });
            try {
                this.send({ jsonrpc: '2.0', id, method, params });
            }
            catch (e) {
                this.pending.delete(id);
                cleanup();
                reject(e);
            }
        });
    }
    private fail(error: Error): void {
        if (this.dead)
            return;
        this.dead = true;
        for (const p of this.pending.values()) {
            p.cleanup();
            p.reject(error);
        }
        this.pending.clear();
        if (this.child)
            killTree(this.child);
        this.emit('closed', error);
    }
    close(): void { this.fail(new CIError('PEER_CLOSED', 'Provider closed by host')); }
}
