#!/usr/bin/env node
/** Reproducible real CLI smoke on the included two-package example.
 * No external packages, network or provider fixture are used in this smoke.
 */
import { strict as assert } from 'node:assert';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = mkdtempSync(join(tmpdir(), 'ci-configured-smoke-'));
const cli = fileURLToPath(new URL('../dist/cli.js', import.meta.url));
const version = JSON.parse(readFileSync(new URL('../package.json', import.meta.url))).version;
function call(...args) {
    const raw = execFileSync(process.execPath, [cli, ...args, '--root', root, '--timeout', '120000'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 2000000 });
    const result = JSON.parse(raw);
    assert.notEqual(result.status, 'error', JSON.stringify(result.error));
    return result;
}
try {
    cpSync(new URL('../examples/monorepo/', import.meta.url), root, { recursive: true, filter: path => !String(path).split(/[\\/]/).includes('.code-intelligence') });
    const source = 'packages/core/src/money.ts', consumer = 'packages/app/src/invoice.ts';
    const original = readFileSync(join(root, source), 'utf8'), originalConsumer = readFileSync(join(root, consumer), 'utf8');
    const indexed = call('index');
    const found = call('find', 'subtotal', '--snapshot', indexed.snapshotId);
    const symbol = found.data.items.map(item => item.entity ?? item).find(item => item.path === source && item.name === 'subtotal');
    assert(symbol, 'Source declaration not returned');
    const prepared = call('tool', 'ci_change_prepare', '--json', JSON.stringify({ snapshotId: indexed.snapshotId, rename: { entityId: symbol.id, newName: 'sumLineItems' } }));
    const plan = prepared.data;
    assert.deepEqual(plan.files.map(f => f.path).sort(), [consumer, source].sort());
    const verified = call('verify', plan.id);
    assert.equal(verified.data.status, 'passed');
    assert.equal(verified.data.static.afterErrors, 0);
    const applied = call('apply', plan.id, '--key', 'configured-smoke');
    assert.equal(applied.data.mode, 'isolated');
    assert.equal(applied.data.status, 'applied');
    assert.equal(readFileSync(join(root, source), 'utf8'), original);
    assert.equal(readFileSync(join(root, consumer), 'utf8'), originalConsumer);
    assert.match(readFileSync(join(applied.data.sandboxPath, source), 'utf8'), /export function sumLineItems/);
    assert.match(readFileSync(join(applied.data.sandboxPath, consumer), 'utf8'), /return sumLineItems\(items\)/);
    const report = { version, measuredAt: new Date().toISOString(), passed: true, mode: 'actual_cli_native_compiler',
        steps: ['index', 'find', 'cross_project_rename_prepare', 'static_verify', 'isolated_apply', 'original_source_unchanged'],
        analysisContexts: indexed.data.coverage.analysisContexts.map(c => c.id), filesChanged: plan.files.map(f => f.path),
        verification: { status: verified.data.status, beforeErrors: verified.data.static.beforeErrors, afterErrors: verified.data.static.afterErrors },
        originalSourceUnchanged: true, noProjectCodeExecuted: true, upstreamServerUsed: false,
        limitation: 'Small configured-project fixture. Not tsc --build, functional tests or a live upstream comparison.' };
    console.log(JSON.stringify(report, null, 2));
    if (process.argv.includes('--save'))
        writeFileSync(new URL('../docs/validation/configured-smoke.json', import.meta.url), JSON.stringify(report, null, 2) + '\n');
}
finally {
    rmSync(root, { recursive: true, force: true, maxRetries: 5 });
}
