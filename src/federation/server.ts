import { createServer as httpServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { createServer as httpsServer } from 'node:https';
import { timingSafeEqual, createHash } from 'node:crypto';
import { CodeIntelligence } from '../engine.js';
import { assert, object, Deadline, integer, text } from '../core/util.js';
import { exportSnapshot, importSnapshot, type SyncIdentity } from './snapshot-sync.js';
export interface FederationProject {
    engine: CodeIntelligence;
    tenantId: string;
    projectId: string;
    sync?: {
        keyId: string;
        publicKey?: string;
        privateKey?: string;
    };
}
export interface FederationGrant {
    token: string;
    tenantId: string;
    projects: string[];
    permissions: ('read' | 'export' | 'import')[];
}
export interface FederationOptions {
    projects: FederationProject[];
    grants: FederationGrant[];
    host?: string;
    port?: number;
    tls?: {
        cert: string;
        key: string;
    };
    allowedHosts?: string[];
    timeoutMs?: number;
}
/** Host-supplied capabilities only. The server never opens a root named in a request. */
export async function serveFederation(options: FederationOptions): Promise<{
    endpoint: string;
    close: () => void;
}> {
    const host = options.host ?? '127.0.0.1', loopback = ['127.0.0.1', '::1'].includes(host), port = options.port ?? 0;
    assert(loopback || options.tls, 'TLS_REQUIRED', 'Non-loopback federation requires TLS');
    assert(options.grants.length > 0 && options.grants.every(g => g.token.length >= 32 && g.projects.length > 0 && g.permissions.length > 0), 'INVALID_GRANT', 'Bounded explicit grants and strong tokens required');
    assert(loopback || options.allowedHosts?.length, 'INVALID_HOST', 'Remote server requires an explicit Host allowlist');
    const roots = new Set<string>();
    for (const p of options.projects) {
        assert(!roots.has(p.engine.workspace.root), 'INVALID_SCOPE', 'Each federated project must use a distinct workspace store');
        roots.add(p.engine.workspace.root);
    }
    const keys = new Set<string>();
    for (const p of options.projects) {
        const k = p.tenantId + '/' + p.projectId;
        assert(!keys.has(k), 'INVALID_SCOPE', 'Duplicate federation project');
        keys.add(k);
    }
    const hashes = options.grants.map(g => ({ grant: g, digest: createHash('sha256').update(g.token).digest() }));
    assert(new Set(hashes.map(v => v.digest.toString('hex'))).size === hashes.length, 'INVALID_GRANT', 'Duplicate tokens are ambiguous');
    let address = '', active = 0;
    const queues = new Map<CodeIntelligence, Promise<unknown>>();
    const scheduled = <T>(engine: CodeIntelligence, fn: () => Promise<T>): Promise<T> => { const p = (queues.get(engine) ?? Promise.resolve()).catch(() => { }).then(fn); queues.set(engine, p); void p.finally(() => { if (queues.get(engine) === p)
        queues.delete(engine); }).catch(() => { }); return p; };
    const handler = (req: IncomingMessage, res: ServerResponse) => {
        const respond = (status: number, data: unknown) => { if (res.writableEnded)
            return; res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' }); res.end(JSON.stringify(data)); };
        if (![address, ...(options.allowedHosts ?? [])].includes(req.headers.host ?? '') || req.headers.origin) {
            respond(403, { error: 'Request denied' });
            return;
        }
        const raw = req.headers.authorization ?? '';
        if (!raw.startsWith('Bearer ') || raw.length > 4096) {
            respond(401, { error: 'Authorization required' });
            return;
        }
        const digest = createHash('sha256').update(raw.slice(7)).digest(), matches = hashes.filter(g => timingSafeEqual(g.digest, digest));
        if (matches.length !== 1) {
            respond(401, { error: 'Authorization required' });
            return;
        }
        const grant = matches[0]!.grant;
        if (req.method !== 'POST' || req.url !== '/v1') {
            respond(404, { error: 'Not found' });
            return;
        }
        if (active >= 8) {
            respond(429, { error: 'Server busy' });
            return;
        }
        active++;
        res.once('close', () => active--);
        const controller = new AbortController();
        req.once('aborted', () => controller.abort());
        res.once('close', () => { if (!res.writableFinished)
            controller.abort(); });
        const deadline = new Deadline(options.timeoutMs ?? 120000, controller.signal);
        let size = 0, rejected = false;
        const chunks: Buffer[] = [];
        req.on('data', (chunk: Buffer) => { size += chunk.length; if (size > 64000000) {
            rejected = true;
            respond(413, { error: 'Request too large' });
        }
        else if (!rejected)
            chunks.push(chunk); });
        req.on('end', () => {
            if (rejected)
                return;
            void (async () => {
                try {
                    const input = object(JSON.parse(Buffer.concat(chunks).toString('utf8'))), action = text(input.action, 'action', 32);
                    const permission = action === 'export' ? 'export' : action === 'import' ? 'import' : 'read';
                    assert(grant.permissions.includes(permission), 'FORBIDDEN', 'Request denied');
                    const ids = input.projects ?? (input.projectId ? [input.projectId] : grant.projects);
                    assert(Array.isArray(ids) && ids.length > 0 && ids.length <= 32 && ids.every(x => typeof x === 'string'), 'INVALID_ARGUMENT', 'Invalid project selection');
                    // Reject the entire request before lookup to avoid existence leaks and cross-tenant data.
                    assert(ids.every(id => grant.projects.includes(id)), 'FORBIDDEN', 'Request denied');
                    const selected = ids.map(id => options.projects.find(p => p.tenantId === grant.tenantId && p.projectId === id));
                    assert(selected.every(Boolean), 'FORBIDDEN', 'Request denied');
                    if (action === 'find') {
                        const query = text(input.query, 'query', 4000), limit = integer(input.limit, 'limit', 1, 100, 20), result: Record<string, unknown>[] = [];
                        const scopes: Record<string, unknown>[] = [];
                        for (const p of selected as FederationProject[]) {
                            deadline.check();
                            try {
                                const answer = await scheduled(p.engine, async () => { deadline.check(); const requested = object(input.snapshots ?? {})[p.projectId]; assert(requested === undefined || typeof requested === 'string', 'INVALID_ARGUMENT', 'Invalid snapshot selection'); const snapshot = p.engine.workspace.snapshot(requested as string | undefined, deadline); if (snapshot.federation)
                                    assert(snapshot.federation.tenantId === p.tenantId && snapshot.federation.projectId === p.projectId, 'FORBIDDEN', 'Snapshot belongs to another signed scope'); await p.engine.indexer.ensure(snapshot, deadline); return { snapshot, answer: p.engine.query.find(snapshot, query, { kind: 'all', limit }, deadline) }; });
                                scopes.push({ projectId: p.projectId, snapshotId: answer.snapshot.id, total: answer.answer.total, coverage: p.engine.workspace.store.graph(answer.snapshot.id)?.coverage });
                                answer.answer.items.forEach((v, i) => result.push({ projectId: p.projectId, snapshotId: answer.snapshot.id, rank: i + 1, rankScore: 1 / (60 + i + 1), item: v }));
                            }
                            catch (e) {
                                scopes.push({ projectId: p.projectId, status: 'unavailable', code: (e as any).code ?? 'PROVIDER_ERROR' });
                            }
                        }
                        result.sort((a, b) => (b.rankScore as number) - (a.rankScore as number) || String(a.projectId).localeCompare(String(b.projectId)));
                        respond(200, { items: result.slice(0, limit), scopes, exhaustive: false, ranking: 'independent-corpus-rank-fusion', tenantId: grant.tenantId });
                    }
                    else if (action === 'export' || action === 'import') {
                        assert(selected.length === 1, 'INVALID_ARGUMENT', 'Sync selects exactly one project');
                        const p = selected[0]!;
                        assert(p.sync, 'CAPABILITY_UNAVAILABLE', 'Sync not configured');
                        const scope: SyncIdentity = { tenantId: p.tenantId, projectId: p.projectId, keyId: p.sync.keyId };
                        const data = await scheduled(p.engine, async () => { deadline.check(); if (action === 'export') {
                            assert(p.sync!.privateKey, 'CAPABILITY_UNAVAILABLE', 'Export key not configured');
                            return exportSnapshot(p.engine.workspace, p.engine.workspace.snapshot(input.snapshotId as string | undefined, deadline), scope, p.sync!.privateKey);
                        } assert(p.sync!.publicKey, 'CAPABILITY_UNAVAILABLE', 'Import key not configured'); return importSnapshot(p.engine.workspace, input.envelope, scope, p.sync!.publicKey); });
                        respond(200, data);
                    }
                    else
                        respond(400, { error: 'Unsupported operation' });
                }
                catch (e) {
                    respond((e as any).code === 'FORBIDDEN' ? 403 : 400, { error: { code: (e as any).code ?? 'INVALID_REQUEST', message: 'Federation request rejected' } });
                }
            })();
        });
    };
    const server = options.tls ? httpsServer(options.tls, handler) : httpServer(handler);
    server.requestTimeout = 15000;
    server.headersTimeout = 10000;
    await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(port, host, () => { const addr = server.address(); assert(addr && typeof addr !== 'string', 'SERVER_FAILED', 'No federation address'); address = `${host.includes(':') ? '[' + host + ']' : host}:${addr.port}`; resolve(); }); });
    return { endpoint: `${options.tls ? 'https' : 'http'}://${address}/v1`, close: () => { server.closeAllConnections(); server.close(); } };
}
/** Client uses platform TLS verification; credentials cannot follow redirects. */
export async function federationRequest(endpoint: string, token: string, input: unknown, timeoutMs = 120000): Promise<unknown> {
    const url = new URL(endpoint);
    assert(!url.username && !url.password && !url.hash, 'INVALID_ENDPOINT', 'Invalid federation endpoint');
    assert(url.protocol === 'https:' || url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname), 'TLS_REQUIRED', 'Remote exchange requires HTTPS');
    const response = await fetch(url, { method: 'POST', headers: { authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify(input), signal: AbortSignal.timeout(timeoutMs), redirect: 'error' });
    assert(response.ok, 'FEDERATION_REJECTED', `Federation HTTP ${response.status}`);
    assert(response.body, 'FEDERATION_REJECTED', 'No response body');
    const parts: Uint8Array[] = [];
    let size = 0;
    const reader = response.body.getReader();
    try {
        for (;;) {
            const { done, value } = await reader.read();
            if (done)
                break;
            size += value.length;
            assert(size <= 64000000, 'CAPACITY_EXCEEDED', 'Federation response too large');
            parts.push(value);
        }
    }
    finally {
        await reader.cancel();
    }
    return JSON.parse(Buffer.concat(parts).toString('utf8'));
}
