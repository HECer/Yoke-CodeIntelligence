import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { rm } from 'node:fs/promises';
import { CodeIntelligence } from '../dist/engine.js';
import { Deadline } from '../dist/core/util.js';
export function fixture(t, files = {}, options = {}) {
    const root = mkdtempSync(join(tmpdir(), 'ci-test-'));
    if (options.git !== false) {
        execFileSync('git', ['init', '-q', root]);
        execFileSync('git', ['-C', root, 'config', 'core.autocrlf', 'false']);
    }
    const write = (path, content) => { mkdirSync(dirname(join(root, path)), { recursive: true }); writeFileSync(join(root, path), content); };
    for (const [path, content] of Object.entries(files))
        write(path, content);
    const engine = new CodeIntelligence(root, { allowWrites: true, ...options, config: { timeoutMs: 120000, maxResultBytes: 300000, ...options.config } });
    const disposers = [];
    t.after(async () => { try { for (const dispose of disposers.reverse()) await dispose(); } finally { engine.close(); await rm(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 }); } });
    return { root, engine, ws: engine.workspace, cleanup: dispose => disposers.push(dispose), write, read: path => readFileSync(join(root, path), 'utf8'),
        snapshot: () => engine.workspace.capture(new Deadline(120000)),
        async indexed() { const s = engine.workspace.capture(new Deadline(120000)); await engine.indexer.ensure(s, new Deadline(120000)); return { snapshot: s, graph: engine.workspace.store.graph(s.id) }; }
    };
}
export function named(graph, name, path) {
    const result = graph.entities.find(e => e.name === name && e.kind !== 'file' && (!path || e.path === path));
    if (!result)
        throw new Error(`No entity ${name}: ${graph.entities.map(e => e.qualifiedName).join(', ')}`);
    return result;
}
export function replaceFile(f, s, path, value) { return { kind: 'replace', path, expectedHash: f.ws.file(s, path).hash, startByte: 0, endByte: f.ws.read(s, path).length, text: value }; }
export async function okInvoke(engine, tool, args) {
    const out = await engine.invoke(tool, args);
    if (out.status === 'error')
        throw new Error(`${out.error.code}: ${out.error.message}`);
    return out;
}
