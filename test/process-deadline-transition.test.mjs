import test from 'node:test';
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { runProcess } from '../dist/core/process.js';
import { Deadline } from '../dist/core/util.js';

const stalledScript = 'process.stdout.write("ready\\n"); setInterval(() => {}, 1000)';
const successfulScript = 'process.stdout.write("owned stdout"); process.stderr.write("owned stderr")';
const watchdogMs = 10000;

async function bounded(promise, description) {
    let timer;
    try {
        return await Promise.race([
            promise,
            new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(description)), watchdogMs); })
        ]);
    } finally {
        clearTimeout(timer);
    }
}

async function withRecordedChild(script, afterSpawn, body) {
    const originalSpawn = childProcess.spawn;
    const records = [];
    childProcess.spawn = function(command, args, options) {
        const child = originalSpawn.call(childProcess, command, args, options);
        if (command === process.execPath && args?.length === 2 && args[0] === '-e' && args[1] === script) {
            const record = { child, closed: false, streamClosed: { stdin: false, stdout: false, stderr: false } };
            record.closedPromise = new Promise(resolve => child.once('close', () => { record.closed = true; resolve(); }));
            record.readyPromise = new Promise((resolve, reject) => {
                child.stdout.once('data', resolve);
                child.once('error', reject);
            });
            // A post-spawn failure may occur before runProcess installs its own
            // stream error handler. Keep fixture cleanup from becoming unhandled.
            child.stdin.on('error', () => {});
            for (const name of ['stdin', 'stdout', 'stderr'])
                child[name].once('close', () => { record.streamClosed[name] = true; });
            records.push(record);
            afterSpawn?.(record);
        }
        return child;
    };
    syncBuiltinESMExports();
    try {
        await body(records);
    } finally {
        childProcess.spawn = originalSpawn;
        syncBuiltinESMExports();
        // Assertions above run before this safety cleanup. A leaked child must
        // fail the test, while a RED result must still let the test runner exit.
        for (const { child } of records) {
            if (child.exitCode === null && child.signalCode === null)
                child.kill('SIGKILL');
            child.stdin.destroy();
            child.stdout.destroy();
            child.stderr.destroy();
        }
        await Promise.all(records.map(record => bounded(record.closedPromise, 'Owned fixture did not close during final cleanup')));
    }
}

function assertOneClosedChild(records) {
    assert.equal(records.length, 1, 'runProcess must launch exactly one actual owned fixture child');
    const record = records[0];
    assert(Number.isInteger(record.child.pid) && record.child.pid > 0, 'The real spawn must have an OS child PID');
    assert.equal(record.closed, true, 'runProcess rejected before its actual child closed');
    for (const name of ['stdin', 'stdout', 'stderr']) {
        assert.equal(record.streamClosed[name], true, `runProcess settled before child ${name} closed`);
        assert.equal(record.child[name].destroyed, true, `runProcess left child ${name} open`);
    }
}

function observed(promise) {
    return promise.then(value => ({ value }), error => ({ error }));
}

async function expectClosedRejection(outcome, records, code) {
    const result = await bounded(outcome, 'runProcess did not settle after its shared deadline or cancellation');
    assert.equal(result.error?.code, code, result.error?.stack ?? 'runProcess unexpectedly succeeded');
    assertOneClosedChild(records);
}

test('an expired process deadline refuses before any actual child launch', { concurrency: false }, async () => {
    await withRecordedChild(stalledScript, undefined, async records => {
        assert.throws(() => runProcess(process.execPath, ['-e', stalledScript], {
            cwd: process.cwd(), deadline: new Deadline(-1)
        }), error => error.code === 'DEADLINE_EXCEEDED');
        assert.equal(records.length, 0);
    });
});

test('an already cancelled process refuses before any actual child launch', { concurrency: false }, async () => {
    const controller = new AbortController();
    controller.abort();
    await withRecordedChild(stalledScript, undefined, async records => {
        assert.throws(() => runProcess(process.execPath, ['-e', stalledScript], {
            cwd: process.cwd(), deadline: new Deadline(60000, controller.signal)
        }), error => error.code === 'CANCELLED');
        assert.equal(records.length, 0);
    });
});

test('a deadline expiring immediately after actual spawn closes the child and stdio before rejection', { concurrency: false }, async () => {
    const deadline = new Deadline(60000);
    deadline.check();
    let transitions = 0;
    await withRecordedChild(stalledScript, () => {
        // Deterministically cross the actual Deadline's time boundary only after
        // the operating system has created the real fixture process.
        deadline.until = -1;
        transitions++;
    }, async records => {
        const outcome = observed(runProcess(process.execPath, ['-e', stalledScript], { cwd: process.cwd(), deadline }));
        await expectClosedRejection(outcome, records, 'DEADLINE_EXCEEDED');
        assert.equal(transitions, 1);
    });
});

test('cancellation immediately after actual spawn closes the child and stdio before rejection', { concurrency: false }, async () => {
    const controller = new AbortController();
    const deadline = new Deadline(60000, controller.signal);
    deadline.check();
    let transitions = 0;
    await withRecordedChild(stalledScript, () => {
        controller.abort();
        transitions++;
    }, async records => {
        const outcome = observed(runProcess(process.execPath, ['-e', stalledScript], { cwd: process.cwd(), deadline }));
        await expectClosedRejection(outcome, records, 'CANCELLED');
        assert.equal(controller.signal.aborted, true);
        assert.equal(transitions, 1);
    });
});

test('an armed ordinary process deadline closes a stalled actual child and all stdio', { concurrency: false }, async () => {
    await withRecordedChild(stalledScript, undefined, async records => {
        const outcome = observed(runProcess(process.execPath, ['-e', stalledScript], {
            cwd: process.cwd(), deadline: new Deadline(1000)
        }));
        await expectClosedRejection(outcome, records, 'DEADLINE_EXCEEDED');
    });
});

test('an armed ordinary cancellation closes a ready actual child and all stdio', { concurrency: false }, async () => {
    const controller = new AbortController();
    await withRecordedChild(stalledScript, undefined, async records => {
        const outcome = observed(runProcess(process.execPath, ['-e', stalledScript], {
            cwd: process.cwd(), deadline: new Deadline(60000, controller.signal)
        }));
        assert.equal(records.length, 1);
        await bounded(records[0].readyPromise, 'Owned fixture did not become ready');
        controller.abort();
        await expectClosedRejection(outcome, records, 'CANCELLED');
    });
});

test('an ordinary successful actual process preserves captured output and closes all stdio', { concurrency: false }, async () => {
    await withRecordedChild(successfulScript, undefined, async records => {
        const result = await bounded(runProcess(process.execPath, ['-e', successfulScript], {
            cwd: process.cwd(), deadline: new Deadline(60000)
        }), 'Successful owned child did not settle');
        assert.equal(result.exitCode, 0);
        assert.equal(result.stdout, 'owned stdout');
        assert.equal(result.stderr, 'owned stderr');
        assert.equal(result.truncated, false);
        assertOneClosedChild(records);
    });
});
