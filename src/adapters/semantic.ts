import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, lstatSync, rmSync, existsSync, mkdirSync } from 'node:fs';
import { CodeIntelligence } from '../engine.js';
import { Deadline, TextIndex, hash, object, assert } from '../core/util.js';
import { safePath } from '../core/paths.js';
import { dependencyPath } from '../core/dependencies.js';
import { runProcess } from '../core/process.js';
import type { FileEdit } from '../core/types.js';
export interface SemanticOptions {
    trusted: boolean;
    byte: number;
    newName?: string;
    snapshotId?: string;
    timeoutMs?: number;
}
/** Real Python/Go semantic providers, isolated source copy. Not an OS sandbox. */
export async function runSemantic(engine: CodeIntelligence, provider: 'python' | 'go', operation: 'definition' | 'references' | 'rename', path: string, options: SemanticOptions): Promise<Record<string, unknown>> {
    assert(options.trusted, 'PROVIDER_UNTRUSTED', 'Host permission is required for external semantic providers');
    assert(['definition', 'references', 'rename'].includes(operation), 'INVALID_ARGUMENT', 'Invalid semantic operation');
    const deadline = new Deadline(Math.min(options.timeoutMs ?? 120000, engine.workspace.config.timeoutMs));
    const ws = engine.workspace, s = ws.snapshot(options.snapshotId, deadline), f = ws.file(s, path);
    assert(f.language === provider, 'INVALID_ARGUMENT', 'Selected provider differs from source language');
    const source = ws.read(s, path);
    new TextIndex(source.toString('utf8')).utf16(options.byte);
    const directory = join(ws.store.directory, 'changes', `semantic-${randomUUID()}`), root = join(directory, 'tree');
    ws.materialize(s, root);
    try {
        let command: string, args: string[], env: Record<string, string> = {};
        if (provider === 'python') {
            command = ws.config.python;
            args = ['-I', fileURLToPath(new URL('../../workers/python_semantic.py', import.meta.url))];
        }
        else {
            const worker = fileURLToPath(new URL('../../workers/go_semantic.go', import.meta.url)), workerDir = join(ws.store.directory, 'workers');
            mkdirSync(workerDir, { recursive: true });
            command = join(workerDir, `go-semantic-${hash(readFileSync(worker)).slice(0, 16)}${process.platform === 'win32' ? '.exe' : ''}`);
            args = [];
            env = { GOCACHE: join(workerDir, 'gocache'), GO111MODULE: 'off', GOTOOLCHAIN: 'local', GOPROXY: 'off', GOSUMDB: 'off', CGO_ENABLED: '0', HOME: workerDir };
            if (!existsSync(command)) {
                const build = await runProcess('go', ['build', '-trimpath', '-o', command, worker], { cwd: workerDir, deadline, env, maxBytes: 1000000 });
                assert(build.exitCode === 0, 'GO_UNAVAILABLE', build.stderr || 'Go semantic worker failed to compile');
            }
        }
        const run = await runProcess(command, args, { cwd: root, deadline, env, maxBytes: 32000000, input: JSON.stringify({ root, path: f.path, byte: options.byte, operation, newName: options.newName, files: Object.fromEntries(s.files.map(x => [x.path, x.hash])) }) });
        assert(!run.truncated && run.exitCode === 0, 'SEMANTIC_PROVIDER_FAILED', run.stderr || run.stdout.slice(0, 2000) || 'Semantic provider unavailable');
        const data = object(JSON.parse(run.stdout));
        assert(Array.isArray(data.items) && Array.isArray(data.edits), 'ADAPTER_CONTRACT', 'Invalid semantic result');
        for (const file of s.files) {
            deadline.check();
            const p = file.readOnly ? dependencyPath(root, file.path) : safePath(root, file.path), st = lstatSync(p);
            assert(st.isFile() && hash(readFileSync(p)) === file.hash, 'SNAPSHOT_STALE', 'Semantic provider modified captured source');
        }
        const items = data.items.map(raw => { const v = object(raw); assert(typeof v.path === 'string' && typeof v.name === 'string' && Number.isSafeInteger(v.startByte) && Number.isSafeInteger(v.endByte), 'ADAPTER_CONTRACT', 'Invalid source location'); const file = ws.file(s, v.path), bytes = ws.read(s, v.path), ti = new TextIndex(bytes.toString('utf8')); assert(ti.isBoundary(v.startByte as number) && ti.isBoundary(v.endByte as number) && bytes.subarray(v.startByte as number, v.endByte as number).toString('utf8') === v.name, 'ADAPTER_CONTRACT', 'Semantic source range did not match the snapshot'); return { ...v, contentHash: file.hash }; });
        const result: Record<string, unknown> = { ...data, items, snapshotId: s.id, networkIsolation: 'not_enforced', exhaustive: false };
        if (operation === 'rename') {
            const edits: FileEdit[] = data.edits.map(raw => { const v = object(raw); assert(typeof v.path === 'string' && typeof v.text === 'string', 'ADAPTER_CONTRACT', 'Invalid replacement'); const file = ws.file(s, v.path); assert(!file.readOnly, 'READ_ONLY_DEPENDENCY', 'Cannot rename dependency declarations'); return { kind: 'replace', path: file.path, expectedHash: file.hash, startByte: 0, endByte: file.size, text: v.text }; });
            const plan = engine.changes.prepare(s, edits, deadline);
            delete result.edits;
            result.planId = plan.id;
            result.applied = false;
            result.validation = 'not_run';
        }
        else
            delete result.edits;
        ws.store.event('semantic_provider', { provider, version: data.version, snapshotId: s.id, operation, items: items.length, sourceExecuted: false });
        return result;
    }
    finally {
        rmSync(directory, { recursive: true, force: true, maxRetries: 2, retryDelay: 20 });
    }
}
