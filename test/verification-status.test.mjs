import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from './helpers.mjs';
import { Deadline } from '../dist/core/util.js';
test('a real static failure is not hidden by disabled Python analysis', async (t) => {
    const f = fixture(t, { 'main.ts': 'export const n: number = "wrong";', 'code.py': 'def value(): return 1\n' }, { config: { pythonEnabled: false } });
    const r = await f.engine.verifier.verify(f.snapshot(), {}, new Deadline(60000));
    assert.equal(r.status, 'failed');
    assert(r.static.newErrors.length);
    assert(r.static.limitations.some(x => x.includes('disabled')));
});
test('a failing executed check stays failed when a later check times out', async (t) => {
    const f = fixture(t, { 'README.md': '# test\n' }, { trustProject: true, config: { verify: {
                fail: { command: process.execPath, args: ['-e', 'process.exit(2)'] },
                timeout: { command: process.execPath, args: ['-e', 'setTimeout(()=>{},10000)'], timeoutMs: 100 }
            } } });
    const r = await f.engine.verifier.verify(f.snapshot(), { profiles: ['fail', 'timeout'] }, new Deadline(10000));
    assert.equal(r.status, 'failed');
    assert.equal(r.jobs[0].status, 'failed');
    assert.equal(r.jobs[1].status, 'incomplete');
});
