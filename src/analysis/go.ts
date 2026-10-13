import { fileURLToPath } from 'node:url';
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { Workspace } from '../core/workspace.js';
import type { Snapshot, Extraction } from '../core/types.js';
import { Deadline, hash, stable, assert } from '../core/util.js';
import { runProcess } from '../core/process.js';
const WORKER = fileURLToPath(new URL('../../workers/go_ast.go', import.meta.url));
export async function extractGo(workspace: Workspace, snapshot: Snapshot, deadline: Deadline): Promise<Extraction> {
    const result: Extraction = { entities: [], observations: [], diagnostics: [], limitations: [] };
    const files = snapshot.files.filter(f => f.language === 'go');
    if (!files.length)
        return result;
    const directory = join(workspace.store.directory, 'workers');
    mkdirSync(directory, { recursive: true });
    const sourceHash = hash(readFileSync(WORKER));
    const bin = join(directory, `go-ast-${sourceHash.slice(0, 16)}${process.platform === 'win32' ? '.exe' : ''}`);
    if (!existsSync(bin)) {
        const built = await runProcess('go', ['build', '-trimpath', '-o', bin, WORKER], { cwd: directory, deadline, maxBytes: 1000000, env: { GOCACHE: join(directory, 'gocache'), GO111MODULE: 'off', GOTOOLCHAIN: 'local', GOPROXY: 'off', GOSUMDB: 'off', CGO_ENABLED: '0', HOME: directory } });
        assert(built.exitCode === 0, 'GO_UNAVAILABLE', built.stderr || 'Go worker compilation failed');
    }
    const binaryHash = hash(readFileSync(bin));
    const pending: typeof files = [];
    const key = (file: typeof files[number]) => hash(stable([binaryHash, file.path, file.hash]));
    const append = (x: Extraction) => { result.entities.push(...x.entities); result.observations.push(...x.observations); result.diagnostics.push(...x.diagnostics); result.limitations.push(...x.limitations); };
    for (const file of files) {
        const cached = workspace.store.cache<Extraction>(key(file));
        if (cached)
            append(cached);
        else
            pending.push(file);
    }
    if (pending.length) {
        const run = await runProcess(bin, [], { cwd: directory, deadline, maxBytes: 48000000, input: JSON.stringify({ files: pending.map(f => ({ path: f.path, content: workspace.store.blob(f.hash).toString('utf8') })) }) });
        assert(run.exitCode === 0 && !run.truncated, 'GO_EXTRACTION_FAILED', run.stderr || 'Invalid Go worker output');
        const parsed = JSON.parse(run.stdout);
        assert(parsed.results?.length === pending.length, 'ADAPTER_CONTRACT', 'Go worker count mismatch');
        parsed.results.forEach((x: Extraction, i: number) => { workspace.store.setCache(key(pending[i]!), x); append(x); });
    }
    return result;
}
