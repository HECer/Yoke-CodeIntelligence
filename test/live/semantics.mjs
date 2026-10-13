import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fixture } from '../helpers.mjs';
import { runSemantic } from '../../dist/adapters/semantic.js';
import { runLsp } from '../../dist/adapters/lsp-command.js';
// Genuine installed-provider tests are opt-in, not silently presented as baseline unit tests.
const mode = process.env.CI_LIVE_SEMANTICS;
if (mode) {
    test('LIVE Jedi definition/references/rename cross files without executing Python', async (t) => {
        const f = fixture(t, { 'a.py': 'def greet(name):\n    return "Hi " + name\n', 'b.py': 'from a import greet\nvalue = greet("A")\n' }), s = f.snapshot();
        const opt = { trusted: true, snapshotId: s.id, byte: Buffer.byteLength('from a import greet\nvalue = '), timeoutMs: 120000 };
        const d = await runSemantic(f.engine, 'python', 'definition', 'b.py', opt);
        assert(d.items.some(i => i.path === 'a.py' && i.name === 'greet'));
        const r = await runSemantic(f.engine, 'python', 'references', 'b.py', opt);
        assert(r.items.some(i => i.path === 'b.py'));
        const p = await runSemantic(f.engine, 'python', 'rename', 'b.py', { ...opt, newName: 'welcome' });
        const plan = f.engine.changes.plan(p.planId);
        assert.equal(plan.files.length, 2);
        assert(f.read('a.py').includes('greet'));
        assert.equal(p.sourceExecuted, false);
    });
    test('LIVE Jedi refuses invalid identifier and missing trust', async (t) => { const f = fixture(t, { 'a.py': 'x=1\n' }); await assert.rejects(runSemantic(f.engine, 'python', 'definition', 'a.py', { trusted: false, byte: 0 })); await assert.rejects(runSemantic(f.engine, 'python', 'rename', 'a.py', { trusted: true, byte: 0, newName: 'class' })); });
    test('LIVE Go type semantics local modules and safe rename', async (t) => {
        const f = fixture(t, { 'go.mod': 'module example.test/demo\n\ngo 1.23\n', 'lib/greet.go': 'package lib\nfunc Greet() int { return 3 }\n', 'main.go': 'package main\nimport "example.test/demo/lib"\nfunc main(){ _ = lib.Greet() }\n' }), s = f.snapshot(), opt = { trusted: true, snapshotId: s.id, byte: Buffer.byteLength('package main\nimport "example.test/demo/lib"\nfunc main(){ _ = lib.'), timeoutMs: 120000 };
        const d = await runSemantic(f.engine, 'go', 'definition', 'main.go', opt);
        assert(d.items.some(i => i.path === 'lib/greet.go' && i.name === 'Greet'));
        const p = await runSemantic(f.engine, 'go', 'rename', 'main.go', { ...opt, newName: 'Welcome' });
        assert.equal(f.engine.changes.plan(p.planId).files.length, 2);
        assert(f.read('main.go').includes('Greet'));
    });
    test('LIVE Go blocks a rename when dependencies cannot be resolved', async (t) => { const f = fixture(t, { 'go.mod': 'module demo\n', 'a.go': 'package a\nimport "missing.test/p"\nvar Value = p.X\n' }); await assert.rejects(runSemantic(f.engine, 'go', 'rename', 'a.go', { trusted: true, byte: Buffer.byteLength('package a\nimport "missing.test/p"\nvar '), newName: 'Next', timeoutMs: 120000 }), /type-error-free/); });
    test('LIVE clangd actual definition, references and rename plan', async (t) => {
        const f = fixture(t, { 'demo.cpp': 'int square(int x) { return x*x; }\nint main() { return square(4); }\n' }), s = f.snapshot(), command = process.env.CLANGD ?? 'clangd';
        const options = { trusted: true, command, args: ['--background-index=false', '--clang-tidy=false', '--log=error'], snapshotId: s.id, byte: Buffer.byteLength('int square(int x) { return x*x; }\nint main() { return '), timeoutMs: 120000 };
        const d = await runLsp(f.engine, 'definition', 'demo.cpp', options);
        assert(d.data.items.some(i => i.path === 'demo.cpp' && i.startByte === 4));
        const refs = await runLsp(f.engine, 'references', 'demo.cpp', options);
        assert(refs.data.items.length >= 2);
        const p = await runLsp(f.engine, 'rename', 'demo.cpp', { ...options, newName: 'squared' });
        assert.equal(f.engine.changes.plan(p.data.planId).files.length, 1);
        assert(f.read('demo.cpp').includes('square(4)'));
    });
}
