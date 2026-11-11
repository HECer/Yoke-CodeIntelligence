import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import path from 'node:path';
import crypto from 'node:crypto';
import { readFileSync } from 'node:fs';

// Inert API/protocol adversaries for the fixture, not real VS Code acceptance.
function fixture({ badHash = false, missingSymbol = false, native = undefined, cliOverride = undefined } = {}) {
    const root = path.resolve('owned-typescript-fixture'), extensionPath = path.resolve('owned-vsix-extension');
    const node = path.resolve('owned-tools/node.exe'), resultPath = path.join(root, 'result.json');
    const source = path.join(root, 'main.ts'), saved = 'export function saved_symbol(): number { return 1; }\n';
    const files = new Map([[source, saved], [path.join(root, '.vscode/settings.json'), JSON.stringify({
        'codeIntelligence.runtime': 'rust', 'codeIntelligence.nodeExecutable': path.join(root, 'must-not-run.exe'),
        'codeIntelligence.cliPath': path.join(root, 'must-not-run.exe') })]]);
    const settings = new Map(cliOverride === undefined ? [] : [['cliPath', cliOverride]]), calls = [], launches = [];
    let text = saved;
    const doc = { uri: source, isDirty: false, getText: () => text };
    const docs = [], cli = path.join(extensionPath, 'runtime/dist/cli.js');
    const vscode = {
        version: '1.143.1', ConfigurationTarget: { Global: 1 },
        Position: class {}, WorkspaceEdit: class { insert(uri, position, value) { this.value = value; } },
        workspace: { workspaceFolders: [{ uri: { fsPath: root } }], textDocuments: docs,
            getConfiguration: () => ({ inspect: key => ({ globalValue: settings.get(key) }),
                update: async (key, value, target) => { assert.equal(target, 1); settings.set(key, value); } }),
            openTextDocument: async name => name === source ? doc : {},
            applyEdit: async edit => { text = edit.value + text; doc.isDirty = true; return true; } },
        window: { showTextDocument: async () => {} },
        extensions: { getExtension: () => ({ extensionPath, activate: async () => {} }) },
        commands: { executeCommand: async command => {
            calls.push(command);
            if (command.endsWith('.snapshot')) docs.push({ isUntitled: true, languageId: 'json', getText: () => JSON.stringify({
                kind: 'overlay', id: 'owned-overlay', editorBufferFailures: [{ path: '.env' }],
                files: [{ path: 'main.ts', hash: badHash ? 'forged' : crypto.createHash('sha256').update(text).digest('hex') }] }) });
        } }
    };
    const modules = { 'node:assert/strict': assert, 'node:path': path, 'node:crypto': crypto, vscode,
        'node:fs': { readFileSync: name => { assert(files.has(name)); return files.get(name); },
            writeFileSync: (name, value) => files.set(name, value),
            statSync: name => { assert.equal(name, cli); return { isFile: () => true }; } },
        'node:child_process': { execFileSync: (program, args, options) => {
            launches.push({ program, args, options });
            assert.equal(program, node);
            assert.deepEqual(Array.from(args), [cli, 'find', 'editor_unsaved_symbol', '--root', root, '--snapshot', 'owned-overlay']);
            assert.equal(options.timeout, 10000);
            return JSON.stringify({ status: 'ok', data: { items: missingSymbol ? [] : [{ name: 'editor_unsaved_symbol' }] } });
        } }
    };
    const exports = {};
    vm.runInNewContext(readFileSync('scripts/vscode_typescript_host.cjs', 'utf8'), {
        exports, process: { env: { YOKE_VSCODE_NODE: node, YOKE_VSCODE_RESULT: resultPath, YOKE_VSCODE_NATIVE: native } },
        require: name => { assert(Object.hasOwn(modules, name)); return modules[name]; }
    });
    return { run: exports.run, files, source, saved, settings, calls, launches, resultPath };
}

test('TypeScript host fixture selects global Node and bundled CLI without source writes', async () => {
    const f = fixture();
    await f.run();
    assert.equal(f.settings.get('runtime'), 'typescript');
    assert.equal(f.settings.get('cliPath'), undefined);
    assert.equal(f.launches.length, 1);
    assert.equal(f.files.get(f.source), f.saved);
    assert.equal(f.calls.at(-1), 'codeIntelligence.stop');
    const report = JSON.parse(f.files.get(f.resultPath));
    assert.equal(report.runtime, 'typescript');
    assert.equal(report.bundledCliSelected, true);
    assert.equal(report.protectedOpenBufferExcluded, true);
});

test('TypeScript host fixture refuses stale overlay bytes before querying', async () => {
    const f = fixture({ badHash: true });
    await assert.rejects(f.run());
    assert.equal(f.launches.length, 0);
    assert(!f.files.has(f.resultPath));
});

test('TypeScript host fixture cannot report success without the unsaved symbol', async () => {
    const f = fixture({ missingSymbol: true });
    await assert.rejects(f.run());
    assert(!f.files.has(f.resultPath));
});

test('TypeScript host fixture refuses native selection and global CLI overrides', async () => {
    for (const options of [{ native: 'foreign-native' }, { cliOverride: 'foreign-cli' }]) {
        const f = fixture(options);
        await assert.rejects(f.run());
        assert.equal(f.launches.length, 0);
        assert(!f.files.has(f.resultPath));
    }
});
