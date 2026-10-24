import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { killTree } from '../dist/core/process.js';

test('provider cleanup closes a real child and its stdio within a bounded interval', async t => {
    const child = spawn(process.execPath, ['-e', 'process.stdout.write("ready\\n"); setInterval(() => {}, 1000)'], { stdio: ['pipe', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    t.after(() => { child.kill('SIGKILL'); child.stdin.destroy(); child.stdout.destroy(); child.stderr.destroy(); });
    await new Promise((resolve, reject) => { child.stdout.once('data', resolve); child.once('error', reject); });
    const closed = new Promise(resolve => child.once('close', resolve));
    killTree(child);
    const exit = await Promise.race([closed.then(() => true), new Promise(resolve => { const timer = setTimeout(() => resolve(false), 3000); timer.unref(); })]);
    assert.equal(exit, true, 'provider process or inherited stdio remained open after cleanup');
});
