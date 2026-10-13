import { join, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { lstatSync, readFileSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { CodeIntelligence } from '../engine.js';
import { Snapshot } from '../core/types.js';
import { Deadline, assert, hash, object, stable } from '../core/util.js';
import { safePath, normalizePath, secretLike } from '../core/paths.js';
import { runProcess } from '../core/process.js';
import { modelRequest, type ModelProvider } from './enrichment.js';
export async function ingestMedia(engine: CodeIntelligence, s: Snapshot, path: string, options: {
    transcript?: string;
    provider?: ModelProvider;
    expectedHash?: string;
}, deadline: Deadline): Promise<Record<string, unknown>> {
    path = normalizePath(path);
    const absolute = safePath(engine.workspace.root, path), before = lstatSync(absolute);
    assert(before.isFile() && before.size <= 50000000, 'CAPACITY_EXCEEDED', 'Media must be a regular file at most 50 MB');
    const bytes = readFileSync(absolute), after = lstatSync(absolute);
    assert(before.size === after.size && before.mtimeMs === after.mtimeMs && before.ino === after.ino, 'SNAPSHOT_STALE', 'Media changed during read');
    const digest = hash(bytes);
    assert(!options.expectedHash || options.expectedHash === digest, 'SNAPSHOT_STALE', 'Media hash differs');
    const directory = join(engine.workspace.store.directory, 'changes', `media-${randomUUID()}`);
    mkdirSync(directory, { recursive: true });
    const copy = join(directory, 'input' + extname(path));
    writeFileSync(copy, bytes);
    let extracted: Record<string, unknown>;
    try {
        const result = await runProcess(engine.workspace.config.python, ['-I', fileURLToPath(new URL('../../workers/media_extract.py', import.meta.url))], { cwd: directory, deadline, input: JSON.stringify({ path: copy }), maxBytes: 16000000 });
        assert(result.exitCode === 0 && !result.truncated, 'MEDIA_UNAVAILABLE', result.stdout.slice(0, 1000) || result.stderr.slice(0, 1000));
        extracted = object(JSON.parse(result.stdout));
    }
    finally {
        rmSync(directory, { recursive: true, force: true, maxRetries: 2, retryDelay: 20 });
    }
    assert(Array.isArray(extracted.segments), 'ADAPTER_CONTRACT', 'Native media segments missing');
    const segments = extracted.segments.map(x => object(x));
    if (options.transcript !== undefined) {
        assert(options.transcript.length <= 2000000 && !secretLike(options.transcript), 'INVALID_TRANSCRIPT', 'Invalid transcript');
        segments.push({ text: options.transcript, origin: 'host_supplied_transcript_not_verified_against_audio' });
    }
    if (options.provider) {
        assert(bytes.length <= 8000000, 'CAPACITY_EXCEEDED', 'Provider media payload limit is 8 MB');
        const description = await modelRequest(engine, options.provider, 'describe_media', { media: { path, hash: digest, extension: extname(path), base64: bytes.toString('base64') }, nativeMetadata: extracted.metadata }, deadline);
        assert(typeof description.text === 'string' && description.text.length <= 2000000, 'ADAPTER_CONTRACT', 'Media provider must return bounded text');
        segments.push({ text: description.text, origin: 'model_interpretation', attemptId: description.attemptId });
    }
    const markdown = `# Media: ${path}\n\nOriginal SHA-256: ${digest}\n\n${JSON.stringify(extracted.metadata)}\n\n` + segments.map((s, i) => `## Segment ${i + 1}${s.page ? ` — page ${s.page}` : ''}\n\n${String(s.text)}\n`).join('\n');
    assert(!secretLike(markdown), 'SECRET_DETECTED', 'Extracted media includes potential secrets');
    const target = `ci-imports/media/${digest.slice(0, 24)}.md`;
    assert(!s.files.some(f => f.path === target), 'IMPORT_EXISTS', 'This source representation already exists');
    const derived = engine.workspace.derive(s, [{ path: target, content: Buffer.from(markdown) }]);
    const mediaHash = engine.workspace.store.putBlob(bytes);
    const artifact = { kind: 'media_provenance', snapshotId: derived.id, sourcePath: path, sourceHash: mediaHash, representationPath: target, metadata: extracted.metadata, segments, warnings: extracted.warnings, exhaustive: false };
    return { snapshotId: derived.id, artifactId: engine.workspace.store.artifact(derived.id, artifact), representationPath: target, sourceHash: mediaHash, metadata: extracted.metadata, warnings: extracted.warnings, sourceWritten: false };
}
