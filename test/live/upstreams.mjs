/** Mandatory live assertions; missing packages FAIL, never turn into simulated passes. */
import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../helpers.mjs';
import { runBridge } from '../../dist/adapters/bridges.js';
import { runSerena } from '../../dist/adapters/serena-command.js';
if (!process.env.CI_LIVE_UPSTREAMS)
    throw Error('Run only with explicit CI_LIVE_UPSTREAMS=1 after reviewing installed providers');
for (const backend of ['graft', 'graphify'])
    test(`LIVE installed ${backend} pinned structural extraction`, async (t) => { const f = fixture(t, { 'a.py': 'def greet(name):\n    return name\n', 'b.py': 'from a import greet\nx=greet("one")\n' }), s = f.snapshot(); const r = await runBridge(f.engine, backend, s.id, { trusted: true, timeoutMs: 120000 }); assert.equal(r.imported, true); assert(f.ws.store.graph(s.id).entities.some(e => e.name === 'greet')); assert.equal(f.read('a.py'), 'def greet(name):\n    return name\n'); });
test('LIVE installed Serena capability discovery and selected semantic symbol query', async (t) => { assert(process.env.CI_SERENA_COMMAND, 'Set reviewed CI_SERENA_COMMAND and JSON CI_SERENA_ARGS including {workspace}'); const args = JSON.parse(process.env.CI_SERENA_ARGS ?? '[]'), f = fixture(t, { 'a.py': 'def greet(name):\n    return name\n' }), s = f.snapshot(), options = { trusted: true, command: process.env.CI_SERENA_COMMAND, args, snapshotId: s.id, timeoutMs: 120000 }; const caps = await runSerena(f.engine, 'capabilities', undefined, {}, options); assert(caps.capabilities.length > 0); const result = await runSerena(f.engine, 'read', 'find_symbol', { name_path_pattern: 'greet', relative_path: 'a.py', include_body: false }, options); assert.equal(result.sourceFilesUnchanged, true); assert(JSON.stringify(result.result).includes('greet')); });
