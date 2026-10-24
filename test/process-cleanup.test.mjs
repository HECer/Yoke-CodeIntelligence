import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { killTree } from '../dist/core/process.js';

// Match the neighboring actual-process tests. Host taskkill enumeration is not
// a three-second runtime SLA; the watchdog still detects a real resource leak.
const watchdogMs = 10000;

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

test('provider cleanup closes a real child and its stdio within a bounded interval', async t => {
    const child = spawn(process.execPath, ['-e', 'process.stdout.write("ready\\n"); setInterval(() => {}, 1000)'], { stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    const observed = { exited: false, closed: false, streams: {} };
    child.once('exit', () => { observed.exited = true; });
    const closed = new Promise(resolve => child.once('close', () => { observed.closed = true; resolve(); }));
    const streams = ['stdin', 'stdout', 'stderr'].map(name => new Promise(resolve => {
        observed.streams[name] = false;
        child[name].once('close', () => { observed.streams[name] = true; resolve(); });
    }));
    child.stdin.on('error', () => {});
    t.after(async () => {
        // Assertions run before this safety cleanup, including on a RED result.
        if (child.exitCode === null && child.signalCode === null)
            child.kill('SIGKILL');
        child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy();
        await bounded(closed, 'Owned fixture did not close during final cleanup');
    });
    await bounded(new Promise((resolve, reject) => { child.stdout.once('data', resolve); child.once('error', reject); }), 'Owned fixture did not become ready');
    assert(Number.isInteger(child.pid) && child.pid > 0, 'The ready fixture must have an actual OS child PID');
    // killTree owns pid/kill only; its caller owns the writable input stream.
    child.stdin.end();
    killTree(child);
    try {
        await bounded(Promise.all([closed, ...streams]), 'Provider process or stdio remained open after cleanup');
        assert.equal(observed.exited, true, 'The actual child must exit');
        assert.equal(observed.closed, true, 'The actual child must emit close');
        for (const name of ['stdin', 'stdout', 'stderr']) {
            assert.equal(observed.streams[name], true, `The actual ${name} close must be observed`);
            assert.equal(child[name].destroyed, true, `The actual ${name} must be destroyed`);
        }
    } finally {
        t.diagnostic(JSON.stringify({ actualChildPid: child.pid, beforeSafetyCleanup: observed }));
    }
});
