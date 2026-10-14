import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { EventEmitter } from 'node:events';

// Labeled protocol fixture for lifecycle races; the actual extension-host
// integration has a separate test-host.cjs runner.
function fixture({ documents = ['main.rs'], delayHandshake = false, delayOpen = false, delayBuffer = false, failBuffer = false } = {}) {
    const commands = new Map(), children = [], requests = [], information = [], warnings = [], errors = [];
    const root = path.resolve('vscode-fixture');
    const folder = { uri: { scheme: 'file', fsPath: root, toString: () => 'file:///fixture' } };
    const docs = documents.map(name => ({ uri: { scheme: 'file', fsPath: path.join(root, name) }, version: 1, getText: () => 'fixture text' }));
    const events = {};
    const disposable = () => ({ dispose() {} });
    const vscode = {
        workspace: { isTrusted: true, workspaceFolders: [folder], textDocuments: docs,
            getWorkspaceFolder: () => folder, getConfiguration: () => ({ inspect: key => ({ globalValue: key === 'runtime' ? 'rust' : undefined }) }),
            onDidOpenTextDocument: disposable, onDidChangeTextDocument: fn => { events.change = fn; return disposable(); },
            onDidCloseTextDocument: fn => { events.close = fn; return disposable(); }, onDidSaveTextDocument: disposable,
            openTextDocument: async value => value },
        window: { activeTextEditor: { document: docs[0] }, showTextDocument: async () => {},
            showInformationMessage: value => information.push(value), showWarningMessage: value => warnings.push(value), showErrorMessage: value => errors.push(value) },
        commands: { registerCommand(name, fn) { commands.set(name, fn); return disposable(); } }
    };
    let releaseOpen;
    const opening = new Promise(resolve => { releaseOpen = resolve; });
    let releaseBuffer;
    const buffering = new Promise(resolve => { releaseBuffer = resolve; });
    const handshake = child => child.stdout.emit('data', JSON.stringify({ endpoint: 'http://127.0.0.1:32100/editor', token: 'a'.repeat(64) }) + '\n');
    const spawn = () => {
        const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter(); child.kills = 0;
        child.kill = () => { child.kills++; child.exitCode = 0; child.emit('exit', 0); };
        children.push(child); if (!delayHandshake) queueMicrotask(() => handshake(child)); return child;
    };
    const fetch = async (_url, options) => {
        const request = JSON.parse(options.body); requests.push(request);
        if (request.operation === 'open' && delayOpen) await opening;
        if (request.operation === 'buffer' && delayBuffer) await buffering;
        const rejected = request.operation === 'buffer' && (request.buffer.path === '.env' || request.buffer.text === 'refused fixture' || failBuffer);
        const status = rejected ? (failBuffer ? 500 : 400) : 200;
        return { ok: !rejected, status, json: async () => rejected ? { ok: false, error: { code: failBuffer ? 'INTERNAL_ERROR' : 'OPERATION_FAILED' } } :
            { ok: true, data: request.operation === 'open' ? { id: 'session' } : request.operation === 'capture' ? { kind: 'overlay' } : {} } };
    };
    const context = { extensionPath: root, subscriptions: [] }, module = { exports: {} };
    vm.runInNewContext(readFileSync(new URL('../extensions/vscode/extension.cjs', import.meta.url), 'utf8'), {
        module, require: name => name === 'vscode' ? vscode : name === 'node:child_process' ? { spawn } : name === 'node:path' ? path : null,
        process: { platform: process.platform }, fetch, URL, AbortSignal, setTimeout, clearTimeout, queueMicrotask
    }, { filename: 'extension.cjs' });
    module.exports.activate(context);
    return { commands, children, requests, information, warnings, errors, docs, events, releaseOpen, releaseBuffer, handshake, deactivate: module.exports.deactivate };
}
const turn = () => new Promise(resolve => setImmediate(resolve));

test('excluded open documents do not poison startup or prevent allowed buffer capture', async () => {
    const f = fixture({ documents: ['main.rs', '.env'] });
    await f.commands.get('codeIntelligence.start')();
    await f.commands.get('codeIntelligence.snapshot')();
    assert.equal(f.errors.length, 0); assert(f.warnings.length > 0);
    assert(f.requests.some(request => request.operation === 'capture'));
    assert.equal(f.children.length, 1); assert.equal(f.children[0].kills, 0);
    await f.deactivate(); assert(f.requests.some(request => request.operation === 'close'));
});
test('failed startup closes its session and permits a fresh process on retry', async () => {
    const f = fixture({ failBuffer: true });
    await f.commands.get('codeIntelligence.start')();
    assert(f.requests.some(request => request.operation === 'close'));
    assert.equal(f.children[0].kills, 1);
    await f.commands.get('codeIntelligence.start')();
    assert.equal(f.children.length, 2); await f.deactivate();
});
test('stop cancels a pending handshake before it can open a session', async () => {
    const f = fixture({ delayHandshake: true });
    const start = f.commands.get('codeIntelligence.start')(); await turn();
    await f.commands.get('codeIntelligence.stop')();
    f.handshake(f.children[0]); await start;
    assert.equal(f.children[0].kills, 1); assert(!f.requests.some(request => request.operation === 'open'));
    assert.equal(f.information.length, 0); assert.equal(f.errors.length, 0);
});
test('stop waits for an in-flight open and closes the resulting session', async () => {
    const f = fixture({ delayOpen: true });
    const start = f.commands.get('codeIntelligence.start')(); await turn();
    assert(f.requests.some(request => request.operation === 'open'));
    const stop = f.commands.get('codeIntelligence.stop')(); f.releaseOpen(); await Promise.all([start, stop]);
    assert(f.requests.some(request => request.operation === 'close'));
    assert.equal(f.children[0].kills, 1); assert.equal(f.information.length, 0);
    assert.equal(f.errors.length, 0);
});
test('closing a refused newer update removes its previously accepted buffer version', async () => {
    const f = fixture(); await f.commands.get('codeIntelligence.start')();
    const doc = f.docs[0]; doc.version = 2; doc.getText = () => 'refused fixture';
    f.events.change({ document: doc }); await turn(); f.events.close(doc); await turn();
    const closed = f.requests.find(request => request.operation === 'closeFile');
    assert(closed); assert.equal(closed.version, 1); await f.deactivate();
});
test('startup reconciles changes arriving while its first buffer upload is in flight', async () => {
    const f = fixture({ delayBuffer: true });
    const start = f.commands.get('codeIntelligence.start')(); await turn();
    assert(f.requests.some(request => request.operation === 'buffer' && request.buffer.version === 1));
    const doc = f.docs[0]; doc.version = 2; doc.getText = () => 'new unsaved text';
    f.events.change({ document: doc }); f.releaseBuffer(); await start;
    await f.commands.get('codeIntelligence.snapshot')();
    const uploads = f.requests.filter(request => request.operation === 'buffer');
    assert.equal(uploads.at(-1).buffer.version, 2); assert.equal(uploads.at(-1).buffer.text, 'new unsaved text');
    f.events.close(doc); await turn();
    assert.equal(f.requests.find(request => request.operation === 'closeFile').version, 2); await f.deactivate();
});
test('accepted version describes transmitted bytes when document changes during upload', async () => {
    const f = fixture({ delayBuffer: true });
    const start = f.commands.get('codeIntelligence.start')(); await turn();
    const doc = f.docs[0]; doc.version = 2; doc.getText = () => 'refused fixture';
    f.events.change({ document: doc }); f.releaseBuffer(); await start;
    await f.commands.get('codeIntelligence.snapshot')(); f.events.close(doc); await turn();
    assert.equal(f.requests.find(request => request.operation === 'closeFile')?.version, 1); await f.deactivate();
});
