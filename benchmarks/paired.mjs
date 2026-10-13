#!/usr/bin/env node
/** Independent acceptance harness for HOST-SELECTED agent drivers. Never fabricate an agent run.
 * Driver consumes one JSON request on stdin, modifies the isolated tree, and returns one JSON
 * response {schema:1,model,usage:{inputTokens,outputTokens},actualCost:number|null} on stdout.
 * Setup/indexing costs must be included by that driver. Verifiers are separate host commands.
 */
import { readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { performance } from 'node:perf_hooks';
import { CodeIntelligence, Deadline, hash } from '../dist/index.js';
import { runProcess } from '../dist/core/process.js';
const argv = process.argv.slice(2), at = argv.indexOf('--config');
if (at < 0 || !argv.includes('--trust-drivers')) {
    console.error('Usage: node benchmarks/paired.mjs --config HOST.json --trust-drivers [--output report.json]\nDrivers/verifiers execute locally; copied trees are NOT an OS sandbox. No bundled agent, provider key or measured result is implied.');
    process.exitCode = 2;
}
else {
    const config = JSON.parse(readFileSync(resolve(argv[at + 1]), 'utf8'));
    if (config.schema !== 1 || !config.model || !Array.isArray(config.tasks) || !config.tasks.length || !Array.isArray(config.arms) || config.arms.length < 2 || config.tasks.length > 100 || config.arms.length > 10)
        throw Error('Invalid bounded schema/model/tasks/arms');
    const iterations = config.iterations ?? 3;
    if (!Number.isInteger(iterations) || iterations < 1 || iterations > 10)
        throw Error('Invalid iteration count');
    const budget = config.maxCost;
    if (typeof budget !== 'number' || !Number.isFinite(budget) || budget <= 0)
        throw Error('Explicit finite maxCost required');
    const reserve = config.maxCostPerAttempt;
    if (typeof reserve !== 'number' || !Number.isFinite(reserve) || reserve <= 0 || reserve > budget)
        throw Error('maxCostPerAttempt reservation required');
    const attempts = [];
    let actual = 0, held = 0;
    const outAt = argv.indexOf('--output'), outPath = resolve(outAt < 0 ? 'paired-report.json' : argv[outAt + 1]);
    const logs = outPath + '.logs';
    mkdirSync(logs, { recursive: true });
    const checkpoint = () => { const scored = attempts.filter(a => a.verification === 'passed'), totals = Object.fromEntries(config.arms.map(arm => { const values = attempts.filter(a => a.arm === arm.id), unknown = values.some(a => a.actualCost === null); return [arm.id, { attempts: values.length, passed: values.filter(a => a.verification === 'passed').length, knownCost: values.reduce((s, a) => s + (a.actualCost ?? 0), 0), costComplete: !unknown, costPerPassedTask: unknown || !values.some(a => a.verification === 'passed') ? null : values.reduce((s, a) => s + a.actualCost, 0) / values.filter(a => a.verification === 'passed').length }]; })); writeFileSync(outPath, JSON.stringify({ schema: 1, kind: 'paired_agent_benchmark', model: config.model, measuredAt: new Date().toISOString(), actualKnown: actual, unknownCostReserved: held, totals, attempts, limitations: ['Driver usage is reported by the host driver, not independently metered API billing. Missing usage/cost stays unknown.', 'Independent commands and protected paths are checked; trusted verifier quality defines acceptance.', 'Provide identical model/reasoning/budgets and upstream versions in each driver arm. The harness does not assume model-equivalent external settings.', 'No cross-platform or statistical generalization from a small corpus is implied.'] }, null, 2)); };
    outer: for (const task of config.tasks) {
        if (!task.id || !task.root || !task.prompt || !Array.isArray(task.verify) || !task.verify.length)
            throw Error('Every task requires id/root/prompt and independent verify commands');
        const engine = new CodeIntelligence(resolve(task.root), { config: { timeoutMs: 120000 } });
        try {
            const snapshot = engine.workspace.capture(new Deadline(120000));
            for (let trial = 0; trial < iterations; trial++)
                for (const arm of [...config.arms.slice(trial % config.arms.length), ...config.arms.slice(0, trial % config.arms.length)])
                    for (const temperature of ['cold', 'warm']) {
                        if (actual + held + reserve > budget) {
                            checkpoint();
                            break outer;
                        }
                        const dir = mkdtempSync(join(tmpdir(), 'ci-paired-')), tree = join(dir, 'tree');
                        engine.workspace.materialize(snapshot, tree);
                        const id = `${attempts.length + 1}`, start = performance.now();
                        const item = { id, task: task.id, arm: arm.id, trial, temperature, snapshotId: snapshot.id, verification: 'not_run', actualCost: null, usage: null, latencyMs: null, errors: [] };
                        held += reserve;
                        attempts.push(item);
                        checkpoint();
                        try {
                            if (!arm.id || !arm.command || !Array.isArray(arm.args))
                                throw Error('Each arm needs id/command/args');
                            const result = await runProcess(arm.command, arm.args.map(a => String(a).replaceAll('{workspace}', tree)), { cwd: tree, input: JSON.stringify({ schema: 1, model: config.model, taskId: task.id, prompt: task.prompt, root: tree, snapshotId: snapshot.id, temperature, limits: { ...(config.limits ?? {}), maxCost: reserve }, instruction: 'Include cold or warm preparation/indexing costs and every retry in your usage and actualCost. Do not modify protected acceptance tests.' }), deadline: new Deadline(config.timeoutMs ?? 600000), maxBytes: 2000000, env: arm.env });
                            writeFileSync(join(logs, id + '-driver.json'), JSON.stringify(result));
                            let data;
                            try {
                                data = JSON.parse(result.stdout);
                            }
                            catch { }
                            if (typeof data?.actualCost === 'number' && Number.isFinite(data.actualCost) && data.actualCost >= 0) {
                                item.actualCost = data.actualCost;
                                actual += data.actualCost;
                                held -= reserve;
                            }
                            item.usage = data?.usage ?? null;
                            if (result.exitCode !== 0 || result.truncated || data?.schema !== 1 || data.model !== config.model)
                                throw Error('Driver failed or changed the requested model identity');
                            if (item.actualCost !== null && item.actualCost > reserve)
                                throw Error('Driver exceeded declared cost reservation');
                            for (const file of snapshot.files.filter(f => (task.protectedPaths ?? []).includes(f.path)))
                                if (!existsSync(join(tree, file.path)) || hash(readFileSync(join(tree, file.path))) !== file.hash)
                                    throw Error('Protected acceptance source modified');
                            item.verification = 'passed';
                            for (let n = 0; n < task.verify.length; n++) {
                                const v = task.verify[n];
                                if (!v.command || !Array.isArray(v.args))
                                    throw Error('Invalid independent verifier');
                                const checked = await runProcess(v.command, v.args.map(a => String(a).replaceAll('{workspace}', tree)), { cwd: tree, deadline: new Deadline(config.verifyTimeoutMs ?? 120000), maxBytes: 1000000, env: v.env });
                                writeFileSync(join(logs, `${id}-verify-${n}.json`), JSON.stringify(checked));
                                if (checked.exitCode !== 0 || checked.truncated)
                                    item.verification = 'failed';
                            }
                        }
                        catch (e) {
                            item.errors.push(e.message);
                            item.verification = 'failed';
                        }
                        finally {
                            item.latencyMs = performance.now() - start;
                            rmSync(dir, { recursive: true, force: true, maxRetries: 5 });
                            checkpoint();
                        }
                    }
        }
        finally {
            engine.close();
        }
    }
    checkpoint();
    console.log(JSON.stringify({ report: outPath, attempts: attempts.length, knownCost: actual, unknownReserved: held }));
    if (!attempts.length || attempts.some(a => a.verification !== 'passed' || a.actualCost === null))
        process.exitCode = 1;
}
