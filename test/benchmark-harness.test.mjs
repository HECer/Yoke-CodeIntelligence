/** Protocol fixtures validate accounting; these are NOT real agent performance results. */
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync, readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { fixture } from './helpers.mjs';
const script = fileURLToPath(new URL('../benchmarks/paired.mjs', import.meta.url));
test('paired harness uses real independent verification and charges all protocol-fixture attempts', t => { const f = fixture(t, { 'source.js': 'const value=1;' }), out = mkdtempSync(join(tmpdir(), 'ci-bench-fixture-')); t.after(() => rmSync(out, { recursive: true, force: true })); const code = 'let s="";process.stdin.on("data",b=>s+=b).on("end",()=>{const r=JSON.parse(s);process.stdout.write(JSON.stringify({schema:1,model:r.model,actualCost:0.01,usage:{inputTokens:10,outputTokens:2}}));})'; const config = { schema: 1, model: 'TEST_PROTOCOL_NOT_A_REAL_MODEL', iterations: 1, maxCost: 1, maxCostPerAttempt: .1, arms: [{ id: 'fixture-a', command: process.execPath, args: ['-e', code] }, { id: 'fixture-b', command: process.execPath, args: ['-e', code] }], tasks: [{ id: 'fixture', root: f.root, prompt: 'protocol-test-only', protectedPaths: ['source.js'], verify: [{ command: process.execPath, args: ['-e', 'if(require("node:fs").readFileSync("source.js","utf8")!=="const value=1;")process.exit(1)'] }] }] }; const configPath = join(out, 'config.json'), report = join(out, 'report.json'); writeFileSync(configPath, JSON.stringify(config)); const r = spawnSync(process.execPath, [script, '--config', configPath, '--trust-drivers', '--output', report], { encoding: 'utf8', timeout: 30000 }); assert.equal(r.status, 0, r.stderr); const data = JSON.parse(readFileSync(report)); assert.equal(data.attempts.length, 4); assert.equal(data.actualKnown, .04); assert.equal(data.unknownCostReserved, 0); assert(data.attempts.every(a => a.verification === 'passed')); });
test('paired harness refuses implicit driver execution', () => { const r = spawnSync(process.execPath, [script], { encoding: 'utf8' }); assert.equal(r.status, 2); assert.match(r.stderr, /trust-drivers/); });
