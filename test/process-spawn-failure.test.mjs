import test from 'node:test';
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { mkdtempSync, existsSync, rmdirSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { runProcess } from '../dist/core/process.js';
import { Deadline } from '../dist/core/util.js';

const watchdogMs = 10000;
const script = 'setInterval(() => {}, 1000)';

async function bounded(promise, message) {
    let timer;
    try {
        return await Promise.race([
            promise,
            new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), watchdogMs); })
        ]);
    } finally {
        clearTimeout(timer);
    }
}

function snapshot(record) {
    return {
        pid: record.child.pid ?? null,
        initialPid: record.initialPid ?? null,
        childClosed: record.closed,
        errors: record.errors.map(error => ({ name: error.name, code: error.code, syscall: error.syscall })),
        streams: Object.fromEntries(['stdin', 'stdout', 'stderr'].map(name => {
            const stream = record.child[name];
            return [name, stream === null ? { shape: 'null' }
                : stream === undefined ? { shape: 'undefined' }
                : { shape: stream.constructor.name, closeObserved: record.streamClosed[name], destroyed: stream.destroyed }];
        }))
    };
}

async function realSpawnFailure(t, mode) {
    const root = mkdtempSync(join(tmpdir(), 'yoke-owned-spawn-failure-'));
    const missingExecutable = join(root, 'missing-owned-executable.exe');
    const missingCwd = join(root, 'missing-owned-cwd');
    assert.equal(existsSync(missingExecutable), false);
    assert.equal(existsSync(missingCwd), false);
    const command = mode === 'missing executable' ? missingExecutable : process.execPath;
    const cwd = mode === 'missing cwd' ? missingCwd : root;
    const originalSpawn = childProcess.spawn;
    const records = [];
    childProcess.spawn = function(actualCommand, args, options) {
        const child = originalSpawn.call(childProcess, actualCommand, args, options);
        if (actualCommand === command && options?.cwd === cwd && args?.length === 2 && args[0] === '-e' && args[1] === script) {
            const record = { child, initialPid: child.pid, closed: false, errors: [], streamClosed: {} };
            // Observe the real failed-spawn error before returning to production.
            // Even a production setup throw must not crash the test runner.
            child.on('error', error => record.errors.push(error));
            record.closedPromise = new Promise(resolve => child.once('close', () => { record.closed = true; resolve(); }));
            for (const name of ['stdin', 'stdout', 'stderr']) {
                record.streamClosed[name] = false;
                child[name]?.on('error', () => {});
                child[name]?.once('close', () => { record.streamClosed[name] = true; });
            }
            records.push(record);
        }
        return child;
    };
    syncBuiltinESMExports();
    try {
        const result = await bounded(runProcess(command, ['-e', script], {
            cwd, deadline: new Deadline(60000), input: 'must not be sent to a nonexistent process'
        }).then(
            value => ({ value, atSettlement: records.map(snapshot) }),
            error => ({ error, atSettlement: records.map(snapshot) })
        ), 'runProcess did not settle for an actual OS spawn failure');
        // This waits only for observation. The assertion below uses the saved
        // settlement snapshot, before any fixture cleanup, to catch early reject.
        await Promise.all(records.map(record => bounded(record.closedPromise, 'Actual failed-spawn child did not emit close')));
        t.diagnostic(JSON.stringify({ fixture: mode, actualLaunches: records.length,
            resultError: result.error && { name: result.error.name, code: result.error.code, message: result.error.message },
            atSettlement: result.atSettlement, finalObserved: records.map(snapshot) }));
        assert.equal(records.length, 1, 'Exactly one actual OS spawn attempt is required');
        assert.equal(records[0].initialPid, undefined, 'The actual OS failure must have no child PID');
        assert.equal(records[0].errors.length, 1, 'The real failed spawn must emit one error');
        assert.equal(records[0].errors[0].code, 'ENOENT');
        assert.equal(result.error?.code, 'PROCESS_UNAVAILABLE', result.error?.stack ?? 'Failed spawn unexpectedly succeeded');
        assert.equal(result.atSettlement[0].childClosed, true, 'runProcess rejected before actual child close');
        for (const [name, stream] of Object.entries(result.atSettlement[0].streams)) {
            if (stream.shape !== 'null' && stream.shape !== 'undefined') {
                assert.equal(stream.closeObserved, true, `Failed spawn left ${name} without an observed close before rejection`);
                assert.equal(stream.destroyed, true, `Failed spawn left ${name} open before rejection`);
            }
        }
    } finally {
        childProcess.spawn = originalSpawn;
        syncBuiltinESMExports();
        for (const { child } of records) {
            if (Number.isInteger(child.pid) && child.pid > 0 && child.exitCode === null && child.signalCode === null)
                child.kill('SIGKILL');
            for (const name of ['stdin', 'stdout', 'stderr'])
                child[name]?.destroy();
        }
        await Promise.all(records.map(record => bounded(record.closedPromise, 'Owned failed-spawn fixture did not close during safety cleanup')));
        rmdirSync(root);
    }
}

test('a real missing owned executable reports unavailable after failed-spawn resource cleanup', { concurrency: false }, async t => {
    await realSpawnFailure(t, 'missing executable');
});

test('a real nonexistent owned cwd reports unavailable after failed-spawn resource cleanup', { concurrency: false }, async t => {
    await realSpawnFailure(t, 'missing cwd');
});
