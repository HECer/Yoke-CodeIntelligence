'use strict';
// Real VS Code host using only the TypeScript runtime extracted from the VSIX.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const vscode = require('vscode');

// VS Code may lowercase a Windows drive letter. Keep every other byte exact,
// including opaque Unicode directory names; this is not a filesystem alias test.
const driveSpelling = value => value.replace(/^[A-Z](?=:)/, letter => letter.toLowerCase());

exports.run = async function () {
    const node = process.env.YOKE_VSCODE_NODE, resultPath = process.env.YOKE_VSCODE_RESULT;
    assert(node && resultPath, 'Owned launcher must supply Node and result paths');
    assert.equal(process.env.YOKE_VSCODE_NATIVE, undefined, 'TypeScript fixture must not select a native CLI');
    const folder = vscode.workspace.workspaceFolders?.[0];
    assert(folder, 'Local test folder required');
    const source = path.join(folder.uri.fsPath, 'main.ts'), before = fs.readFileSync(source, 'utf8');
    const settings = JSON.parse(fs.readFileSync(path.join(folder.uri.fsPath, '.vscode/settings.json'), 'utf8'));
    assert.equal(settings['codeIntelligence.runtime'], 'rust');
    for (const key of ['nodeExecutable', 'cliPath'])
        assert.equal(driveSpelling(settings['codeIntelligence.' + key]),
            driveSpelling(path.join(folder.uri.fsPath, 'must-not-run.exe')));
    const config = vscode.workspace.getConfiguration('codeIntelligence');
    assert.equal(config.inspect('cliPath').globalValue, undefined, 'Bundled CLI must have no global override');
    assert.equal(config.inspect('nativeExecutable').globalValue, undefined);
    await config.update('runtime', 'typescript', vscode.ConfigurationTarget.Global);
    await config.update('nodeExecutable', node, vscode.ConfigurationTarget.Global);
    const extension = vscode.extensions.getExtension('hecer.code-intelligence-local');
    assert(extension, 'Extracted VSIX extension not loaded');
    const cli = path.join(extension.extensionPath, 'runtime/dist/cli.js');
    assert(fs.statSync(cli).isFile(), 'Bundled TypeScript CLI missing');
    await extension.activate();
    const doc = await vscode.workspace.openTextDocument(source);
    await vscode.window.showTextDocument(doc);
    fs.writeFileSync(path.join(folder.uri.fsPath, '.env'), 'TEST_FIXTURE=excluded\n');
    await vscode.workspace.openTextDocument(path.join(folder.uri.fsPath, '.env'));
    const edit = new vscode.WorkspaceEdit();
    edit.insert(doc.uri, new vscode.Position(0, 0), '// Unsaved TypeScript fixture\nexport function editor_unsaved_symbol(): number { return 2; }\n');
    assert(await vscode.workspace.applyEdit(edit));
    assert(doc.isDirty, 'Fixture must remain unsaved');
    await vscode.commands.executeCommand('codeIntelligence.start');
    await vscode.commands.executeCommand('codeIntelligence.snapshot');
    const rendered = vscode.workspace.textDocuments
        .filter(document => document.isUntitled && document.languageId === 'json')
        .map(document => { try { return JSON.parse(document.getText()); } catch { return null; } })
        .find(value => value?.kind === 'overlay');
    assert(rendered, 'Actual TypeScript host did not render an overlay');
    assert(rendered.editorBufferFailures?.some(item => item.path === '.env'));
    assert(!rendered.files.some(file => file.path === '.env'));
    const file = rendered.files.find(file => file.path === 'main.ts');
    assert(file, 'Unsaved TypeScript source absent');
    assert.equal(file.hash, crypto.createHash('sha256').update(doc.getText(), 'utf8').digest('hex'));
    const found = JSON.parse(execFileSync(node, [cli, 'find', 'editor_unsaved_symbol', '--root', folder.uri.fsPath,
        '--snapshot', rendered.id], { encoding: 'utf8', timeout: 10000, windowsHide: true }));
    assert.equal(found.status, 'ok');
    assert(found.data.items.some(item => item.name === 'editor_unsaved_symbol'));
    assert.equal(fs.readFileSync(source, 'utf8'), before, 'TypeScript extension wrote saved source');
    await vscode.commands.executeCommand('codeIntelligence.stop');
    fs.writeFileSync(resultPath, JSON.stringify({ kind: 'actual-vscode-extension-host', passed: true,
        runtime: 'typescript', bundledRuntime: true, vscodeVersion: vscode.version,
        unsavedSourceBound: true, workspaceExecutableIgnored: true, workspaceRuntimeIgnored: true,
        workspaceCliIgnored: true, bundledCliSelected: true, symbolFound: true,
        protectedOpenBufferExcluded: true, savedSourceUnchanged: true }) + '\n');
};
