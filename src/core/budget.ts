import type { Envelope } from './types.js';
import { Store } from '../store/store.js';
import { assert } from './util.js';
export const MIN_RESPONSE_BYTES = 1024;
export function measure(envelope: Envelope): Envelope {
    for (let i = 0; i < 12; i++) {
        const bytes = Buffer.byteLength(JSON.stringify(envelope));
        const estimated = Math.ceil(bytes / 3);
        if (bytes === envelope.metrics.serializedBytes && estimated === envelope.metrics.estimatedTokens)
            return envelope;
        envelope.metrics.serializedBytes = bytes;
        envelope.metrics.estimatedTokens = estimated;
    }
    return envelope;
}
export function fit(envelope: Envelope, store: Store, maxBytes: number, tokenBudget?: number): Envelope {
    assert(maxBytes >= MIN_RESPONSE_BYTES, 'BUDGET_TOO_SMALL', `Minimum structured response budget is ${MIN_RESPONSE_BYTES} bytes`);
    const guidance = tokenBudget === undefined ? maxBytes : Math.min(maxBytes, Math.max(MIN_RESPONSE_BYTES, tokenBudget * 3));
    if (measure(envelope).metrics.serializedBytes <= guidance)
        return envelope;
    envelope = structuredClone(envelope);
    const original = structuredClone(envelope.data);
    const artifactId = store.artifact(envelope.snapshotId, original);
    envelope.artifactId = artifactId;
    envelope.nextCursor = store.cursor.encode({ kind: 'artifact', artifactId, snapshotId: envelope.snapshotId, offset: 0 });
    if (envelope.status === 'ok')
        envelope.status = 'partial';
    envelope.warnings.push('RESULT_TRUNCATED: full result is available through ci_read with nextCursor. Token count is an estimate, not a measured model limit.');
    const data = envelope.data;
    while (measure(envelope).metrics.serializedBytes > guidance) {
        if (Array.isArray(data.items) && data.items.length) {
            data.items.pop();
            if (Array.isArray(data.edges)) {
                const ids = new Set((data.items as Record<string, unknown>[]).map(n => n.id));
                data.edges = (data.edges as any[]).filter(e => ids.has(e.source) && (e.target === null || ids.has(e.target)));
                if (Array.isArray(data.evidence)) {
                    const evidence = new Set((data.edges as any[]).flatMap(e => e.evidenceIds));
                    data.evidence = (data.evidence as any[]).filter(e => evidence.has(e.id));
                }
                data.traversalComplete = false;
            }
            continue;
        }
        // Do not destroy edit receipt identity or source positions. Large values are
        // moved to an immutable artifact rather than sliced into invalid structures.
        let largest: string | undefined, max = 0;
        for (const [key, value] of Object.entries(data))
            if (!['id', 'planId', 'status', 'snapshotId', 'mode', 'applied', 'path', 'contentHash', 'startByte', 'endByte'].includes(key)) {
                const size = Buffer.byteLength(JSON.stringify(value) ?? '');
                if (size > max && size > 32) {
                    largest = key;
                    max = size;
                }
            }
        if (largest) {
            delete data[largest];
            data.detailsInArtifact = true;
            continue;
        }
        if (envelope.warnings.length > 1) {
            envelope.warnings = ['RESULT_TRUNCATED: read nextCursor for complete data.'];
            continue;
        }
        assert(measure(envelope).metrics.serializedBytes <= maxBytes, 'BUDGET_TOO_SMALL', 'Response identity exceeds the byte budget');
        break;
    }
    return measure(envelope);
}
export function artifactPage(store: Store, cursor: string, requestedBytes = 8000): {
    snapshotId: string | null;
    data: Record<string, unknown>;
    nextCursor?: string;
} {
    const decoded = store.cursor.decode(cursor);
    assert(decoded.kind === 'artifact' && typeof decoded.artifactId === 'string' && Number.isInteger(decoded.offset), 'INVALID_CURSOR', 'Expected artifact cursor');
    const artifact = store.readArtifact(decoded.artifactId);
    assert(artifact.snapshot === decoded.snapshotId, 'INVALID_CURSOR', 'Artifact snapshot mismatch');
    const raw = Buffer.from(JSON.stringify(artifact.value));
    const start = Number(decoded.offset);
    assert(start >= 0 && start <= raw.length, 'INVALID_CURSOR', 'Invalid artifact offset');
    let end = Math.min(raw.length, start + requestedBytes);
    while (end > start && end < raw.length && (raw[end]! & 0xc0) === 0x80)
        end--;
    const nextCursor = end < raw.length ? store.cursor.encode({ ...decoded, offset: end }) : undefined;
    return { snapshotId: artifact.snapshot, data: { artifactId: decoded.artifactId, format: 'json-text-page', startByte: start, endByte: end, totalBytes: raw.length, text: raw.subarray(start, end).toString('utf8'), complete: end === raw.length }, nextCursor };
}
