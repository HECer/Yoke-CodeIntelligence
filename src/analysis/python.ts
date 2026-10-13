import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Extraction, Snapshot } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { Deadline, assert, hash, stable } from '../core/util.js';
import { runProcess } from '../core/process.js';
const runtimes = new Map<string, string>();
const WORKER = fileURLToPath(new URL('../../workers/python_ast.py', import.meta.url));
export async function extractPython(workspace: Workspace, snapshot: Snapshot, deadline: Deadline): Promise<Extraction> {
    const result: Extraction = { entities: [], observations: [], diagnostics: [], limitations: [] };
    const files = snapshot.files.filter(f => f.language === 'python');
    if (!files.length)
        return result;
    if (!workspace.config.pythonEnabled) {
        result.limitations.push('Python AST extraction is disabled; text/file search only.');
        return result;
    }
    let runtime = runtimes.get(workspace.config.python);
    if (!runtime) {
        const probe = await runProcess(workspace.config.python, ['-I', '-c', 'import sys; print(sys.version)'], { cwd: workspace.root, deadline, maxBytes: 10000 });
        assert(probe.exitCode === 0, 'PYTHON_UNAVAILABLE', 'Python runtime probe failed');
        runtime = probe.stdout.trim();
        runtimes.set(workspace.config.python, runtime);
    }
    const workerHash = hash(readFileSync(WORKER));
    const missing: typeof files = [];
    const append = (value: Extraction) => { result.entities.push(...value.entities); result.observations.push(...value.observations); result.diagnostics.push(...value.diagnostics); result.limitations.push(...value.limitations); };
    const key = (file: typeof files[number]) => hash(stable({ worker: workerHash, runtime, path: file.path, hash: file.hash, command: workspace.config.python }));
    for (const file of files) {
        const cached = workspace.store.cache<Extraction>(key(file));
        if (cached)
            append(cached);
        else
            missing.push(file);
    }
    if (missing.length) {
        const output = await runProcess(workspace.config.python, ['-I', WORKER], { cwd: workspace.root, deadline, maxBytes: 48000000, input: JSON.stringify({ files: missing.map(f => ({ path: f.path, content: workspace.store.blob(f.hash).toString('utf8') })) }) });
        assert(output.exitCode === 0 && !output.truncated, 'PYTHON_EXTRACTION_FAILED', output.stderr || 'Python AST output failed or exceeded the limit');
        const parsed = JSON.parse(output.stdout) as {
            results: Extraction[];
        };
        assert(parsed.results?.length === missing.length, 'ADAPTER_CONTRACT', 'Python worker result count mismatch');
        parsed.results.forEach((value, i) => { workspace.store.setCache(key(missing[i]!), value); append(value); });
    }
    // Resolve only module imports here. Attribute calls remain unresolved.
    const paths = new Set(snapshot.files.map(f => f.path));
    for (const edge of result.observations)
        if (edge.relation === 'imports' && edge.targetLabel) {
            const label = edge.targetLabel;
            let module: string;
            if (label.startsWith('.')) {
                const dots = /^\.+/.exec(label)![0].length;
                const owner = edge.path.split('/').slice(0, -dots);
                module = [...owner, ...label.slice(dots).split('.').filter(Boolean)].join('/');
            }
            else
                module = label.replaceAll('.', '/');
            const matches = [module + '.py', module + '/__init__.py'].filter(p => paths.has(p));
            if (matches.length === 1) {
                edge.targetKey = `file:${matches[0]}`;
                edge.resolution = 'syntactic';
            }
        }
    return result;
}
