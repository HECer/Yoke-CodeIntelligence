#!/usr/bin/env node
/** Deterministic local microbenchmark. Not an agent/upstream comparison. */
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir, platform, arch, cpus } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { CodeIntelligence, VERSION } from '../dist/index.js';
const root = mkdtempSync(join(tmpdir(), 'ci-bench-'));
execFileSync('git', ['init', '-q', root]);
mkdirSync(join(root, 'src'));
for (let i = 0; i < 100; i++)
    writeFileSync(join(root, 'src', `module${i}.ts`), `export function calculate${i}(input: number): number { return input + ${i}; }\n` +
        `export function process${i}(input: number): number { return calculate${i}(input) * 2; }\n`);
writeFileSync(join(root, 'README.md'), '# Benchmark fixture\n100 generated TypeScript modules.\n');
const engine = new CodeIntelligence(root, { config: { timeoutMs: 120000, maxResultBytes: 32768 } });
try {
    const started = performance.now();
    const indexed = await engine.invoke('ci_status', { mode: 'index' });
    if (indexed.status === 'error')
        throw new Error(indexed.error.message);
    const coldIndexMs = performance.now() - started;
    const times = [], bytes = [];
    let exactHits = 0;
    for (let i = 0; i < 60; i++) {
        const start = performance.now();
        const result = await engine.invoke('ci_find', { snapshotId: indexed.snapshotId, query: `calculate${i}`, limit: 5 });
        times.push(performance.now() - start);
        bytes.push(Buffer.byteLength(JSON.stringify(result)));
        if (result.data.items?.some(item => (item.entity ?? item).name === `calculate${i}`))
            exactHits++;
    }
    times.sort((a, b) => a - b);
    const graph = engine.workspace.store.graph(indexed.snapshotId);
    const output = { version: VERSION, measuredAt: new Date().toISOString(), kind: 'local_microbenchmark',
        environment: { node: process.version, platform: platform(), arch: arch(), cpu: cpus()[0]?.model },
        fixture: { files: 101, typeScriptModules: 100, queries: 60 },
        results: { coldIndexMs: +coldIndexMs.toFixed(2), warmExplicitSnapshotP50Ms: +times[Math.floor(times.length * .50)].toFixed(2),
            warmExplicitSnapshotP95Ms: +times[Math.floor(times.length * .95)].toFixed(2),
            meanEnvelopeBytes: Math.round(bytes.reduce((a, b) => a + b, 0) / bytes.length), exactNameHits: exactHits,
            entities: graph.entities.length, observations: graph.observations.length, rssBytes: process.memoryUsage().rss, modelCalls: 0 },
        limitations: ['Warm queries reuse an explicit immutable snapshot; a fresh disk capture still reads source bytes.',
            'No agent reasoning, API token billing, independent patch tasks or upstream systems were benchmarked.',
            'A small synthetic fixture on this host is not a production latency guarantee.'] };
    console.log(JSON.stringify(output, null, 2));
    if (process.argv.includes('--save')) {
        mkdirSync(new URL('../docs/validation/', import.meta.url), { recursive: true });
        writeFileSync(new URL('../docs/validation/benchmark.json', import.meta.url), JSON.stringify(output, null, 2) + '\n');
    }
    if (exactHits !== 60)
        process.exitCode = 1;
}
finally {
    engine.close();
    rmSync(root, { recursive: true, force: true, maxRetries: 5 });
}
