import test from 'node:test';
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { killTree } from '../dist/core/process.js';

async function bounded(promise, message) {
    let timer;
    try {
        return await Promise.race([
            promise,
            new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), 10000); })
        ]);
    } finally {
        clearTimeout(timer);
    }
}

test('a failed Windows tree helper falls back only after its actual nonzero exit', { skip: process.platform !== 'win32', concurrency: false }, async t => {
    const originalSpawn = childProcess.spawn;
    const child = originalSpawn(process.execPath, ['-e', 'process.stdout.write("ready\\n"); setInterval(() => {},1000)'], { stdio: ['pipe', 'pipe', 'pipe'] });
    child.stdin.on('error', () => {});
    const closed = new Promise(resolve => child.once('close', resolve));
    const streamClosed = {};
    const streams = ['stdin', 'stdout', 'stderr'].map(name => new Promise(resolve => {
        streamClosed[name] = false;
        child[name].once('close', () => { streamClosed[name] = true; resolve(); });
    }));
    const originalKill = child.kill;
    const attempts = [];
    let helper, helperClosed;
    const helperState = { exited: false, closed: false };
    child.kill = function(signal) {
        attempts.push({ signal, helperExited: helperState.exited, helperExitCode: helperState.exitCode });
        return originalKill.call(this, signal);
    };
    t.after(async () => {
        childProcess.spawn = originalSpawn;
        syncBuiltinESMExports();
        // This cleanup cannot make the earlier lifetime assertions pass.
        if (child.exitCode === null && child.signalCode === null)
            originalKill.call(child, 'SIGKILL');
        child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy();
        if (helper && helper.exitCode === null && helper.signalCode === null)
            helper.kill('SIGKILL');
        await bounded(Promise.all([closed, helperClosed]), 'Owned failure fixtures did not close during final cleanup');
    });
    await bounded(new Promise((resolve, reject) => { child.stdout.once('data', resolve); child.once('error', reject); }), 'Owned target did not become ready');
    assert(Number.isInteger(child.pid) && child.pid > 0);
    child.stdin.end();
    childProcess.spawn = function(command, args, options) {
        if (command !== 'taskkill')
            return originalSpawn.call(this, command, args, options);
        assert.equal(helper, undefined, 'Exactly one tree helper may be launched');
        assert.deepEqual(args, ['/PID', String(child.pid), '/T', '/F']);
        // Labeled helper-failure injection: an actual owned OS process exits 7.
        // This is not a claim that the installed taskkill failed on this host.
        helper = originalSpawn(process.execPath, ['-e', 'process.exitCode=7'], { ...options, stdio: 'ignore' });
        assert(Number.isInteger(helper.pid) && helper.pid > 0);
        helper.once('exit', code => { helperState.exited = true; helperState.exitCode = code; });
        helperClosed = new Promise(resolve => helper.once('close', () => { helperState.closed = true; resolve(); }));
        return helper;
    };
    syncBuiltinESMExports();
    try {
        killTree(child);
        assert(helper, 'The Windows helper branch must actually execute');
        await bounded(Promise.all([closed, helperClosed, ...streams]), 'Failed helper left its actual owned target or stdio open');
        assert.equal(helperState.exitCode, 7);
        assert.equal(helperState.closed, true);
        assert.deepEqual(attempts, [{ signal: 'SIGKILL', helperExited: true, helperExitCode: 7 }]);
        for (const name of ['stdin', 'stdout', 'stderr']) {
            assert.equal(streamClosed[name], true);
            assert.equal(child[name].destroyed, true);
        }
    } finally {
        t.diagnostic(JSON.stringify({ fixture: 'controlled Windows helper nonzero exit', actualTargetPid: child.pid, actualHelperPid: helper?.pid, helperState, attempts, beforeSafetyCleanup: streamClosed }));
    }
});
