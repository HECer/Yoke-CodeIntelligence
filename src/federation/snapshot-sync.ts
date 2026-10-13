/** Signed immutable source exchange. Not a database copy and never an automatic checkout. */
import { sign, verify, createPublicKey, createPrivateKey } from 'node:crypto';
import { Workspace } from '../core/workspace.js';
import { assert, hash, stable, object, text } from '../core/util.js';
import { normalizePath, safePath, secretLike, language, glob } from '../core/paths.js';
import { dependencyPath } from '../core/dependencies.js';
import type { Snapshot, ManifestFile } from '../core/types.js';
export interface SyncIdentity {
    tenantId: string;
    projectId: string;
    keyId: string;
}
export interface SignedSnapshot {
    payload: Record<string, unknown>;
    signature: string;
}
function identity(value: SyncIdentity): void { for (const item of [value.tenantId, value.projectId, value.keyId])
    assert(/^[A-Za-z0-9_.-]{1,128}$/.test(item), 'INVALID_SCOPE', 'Invalid signed scope identifier'); }
export function exportSnapshot(ws: Workspace, snapshot: Snapshot, scope: SyncIdentity, privateKey: string, ttlMs = 86400000): SignedSnapshot {
    identity(scope);
    assert(snapshot.workspaceId === ws.store.workspaceId, 'SCOPE_VIOLATION', 'Wrong export workspace');
    assert(ttlMs > 0 && ttlMs <= 7 * 86400000, 'INVALID_ARGUMENT', 'Export expiry must be within seven days');
    const key = createPrivateKey(privateKey);
    assert(key.asymmetricKeyType === 'ed25519', 'INVALID_KEY', 'Use an Ed25519 signing key');
    const payload = { schema: 1, ...scope, sourceSnapshotId: snapshot.id, issuedAt: new Date().toISOString(), expiresAt: new Date(Date.now() + ttlMs).toISOString(),
        files: snapshot.files.map(f => ({ ...f, content: ws.read(snapshot, f.path).toString('base64') })), omissions: snapshot.omissions };
    const serialized = stable(payload);
    assert(Buffer.byteLength(serialized) <= 64000000, 'CAPACITY_EXCEEDED', 'Export exceeds 64 MB');
    return { payload, signature: sign(null, Buffer.from(serialized), key).toString('base64') };
}
export function importSnapshot(ws: Workspace, input: unknown, scope: SyncIdentity, publicKey: string): {
    snapshotId: string;
    manifestId: string;
    idempotent: boolean;
    sourceWritten: false;
} {
    identity(scope);
    const envelope = object(input), payload = object(envelope.payload);
    const bytes = Buffer.from(stable(payload));
    assert(bytes.length <= 64000000, 'CAPACITY_EXCEEDED', 'Import exceeds 64 MB');
    assert(payload.schema === 1 && payload.tenantId === scope.tenantId && payload.projectId === scope.projectId && payload.keyId === scope.keyId, 'SCOPE_VIOLATION', 'Signed scope does not match the host grant');
    const signature = text(envelope.signature, 'signature', 100), key = createPublicKey(publicKey);
    assert(key.asymmetricKeyType === 'ed25519', 'INVALID_KEY', 'Use an Ed25519 verification key');
    const sig = Buffer.from(signature, 'base64');
    assert(sig.length === 64 && sig.toString('base64') === signature && verify(null, bytes, key, sig), 'INVALID_SIGNATURE', 'Snapshot signature is invalid');
    const issued = Date.parse(String(payload.issuedAt)), expires = Date.parse(String(payload.expiresAt));
    assert(Number.isFinite(issued) && Number.isFinite(expires) && issued <= Date.now() + 300000 && expires > Date.now() && expires > issued && expires - issued <= 7 * 86400000, 'SIGNATURE_EXPIRED', 'Signed snapshot has invalid or expired validity');
    assert(typeof payload.sourceSnapshotId === 'string' && /^[a-f0-9]{64}$/.test(payload.sourceSnapshotId), 'INVALID_MANIFEST', 'Source snapshot ID required');
    assert(Array.isArray(payload.files) && payload.files.length <= ws.config.maxFiles, 'CAPACITY_EXCEEDED', 'Too many imported files');
    const manifestId = hash(bytes), prior = ws.store.meta(`sync:${manifestId}`);
    if (prior) {
        ws.store.snapshot(prior);
        return { snapshotId: prior, manifestId, idempotent: true, sourceWritten: false };
    }
    let total = 0;
    const seen = new Set<string>(), prepared: {
        file: ManifestFile;
        bytes: Buffer;
    }[] = [];
    // Validate the entire manifest before persisting anything. No source/provider execution.
    for (const raw of payload.files) {
        const v = object(raw), path = normalizePath(text(v.path, 'path', 4096));
        assert(!seen.has(path.toLocaleLowerCase('en')), 'INVALID_MANIFEST', 'Duplicate or case-colliding path');
        seen.add(path.toLocaleLowerCase('en'));
        assert(!ws.config.exclude.some(pattern => glob(pattern).test(path)), 'PATH_DENIED', 'Imported file excluded by receiving host policy');
        const dep = path.split('/').includes('node_modules');
        assert(!dep || ws.config.dependencyMode === 'types', 'PATH_DENIED', 'Receiving host does not accept dependency declarations');
        if (dep)
            dependencyPath(ws.root, path);
        else
            safePath(ws.root, path);
        const encoded = text(v.content, 'content', ws.config.maxFileBytes * 2), content = Buffer.from(encoded, 'base64');
        assert(content.toString('base64') === encoded && content.length <= ws.config.maxFileBytes, 'INVALID_MANIFEST', 'Invalid or oversized base64 source');
        total += content.length;
        assert(total <= 48000000, 'CAPACITY_EXCEEDED', 'Decoded snapshot exceeds 48 MB');
        assert(hash(content) === v.hash && content.length === v.size, 'HASH_MISMATCH', 'Imported source hash or size differs');
        const decoded = new TextDecoder('utf-8', { fatal: true }).decode(content);
        assert(!decoded.includes('\0') && !secretLike(decoded), 'PATH_DENIED', 'Binary or potential secret source refused');
        const file: ManifestFile = { path, hash: v.hash as string, size: content.length, mode: v.mode === 0o755 ? 0o755 : 0o644, language: language(path), ...(dep ? { readOnly: true, dependency: 'signed-declarations' } : {}) };
        prepared.push({ file, bytes: content });
    }
    const body = { workspaceId: ws.store.workspaceId, projectId: ws.store.projectId, configHash: hash(stable(ws.config)), kind: 'federated' as const,
        federation: { ...scope, sourceSnapshotId: payload.sourceSnapshotId, manifestId }, files: prepared.map(p => p.file).sort((a, b) => a.path.localeCompare(b.path)),
        omissions: [{ path: '*', reason: 'Remote signed source is authenticated, not verified complete or safe; sender omissions are retained in the signed artifact.' }], captureConsistency: 'derived-from-immutable' as const };
    const snapshot: Snapshot = { ...body, id: hash(stable(body)), createdAt: new Date().toISOString() };
    ws.store.transaction(() => { for (const f of prepared)
        ws.store.putBlob(f.bytes); ws.store.putSnapshot(snapshot); ws.store.setMeta(`sync:${manifestId}`, snapshot.id); ws.store.artifact(snapshot.id, { kind: 'signed_snapshot_receipt', manifestId, scope, sourceSnapshotId: payload.sourceSnapshotId, signature, issuedAt: payload.issuedAt, expiresAt: payload.expiresAt, omissions: payload.omissions ?? [] }); });
    return { snapshotId: snapshot.id, manifestId, idempotent: false, sourceWritten: false };
}
