'use strict';
const vscode = require('vscode');
const { spawn } = require('node:child_process');
const path = require('node:path');
const instances = new Map(), pendingStarts = new Map();
let stopping = null;

function userSetting(key, fallback) {
  return vscode.workspace.getConfiguration('codeIntelligence').inspect(key)?.globalValue ?? fallback;
}
function projectFor(doc) {
  if (doc.uri.scheme === 'file') return vscode.workspace.getWorkspaceFolder(doc.uri);
}
function relative(folder, doc) {
  const name = path.relative(folder.uri.fsPath, doc.uri.fsPath).replaceAll('\\', '/');
  if (!name || name.startsWith('../') || path.isAbsolute(name)) throw Error('Document is outside the workspace');
  return name;
}
function cancelled() {
  const error = Error('Editor startup cancelled'); error.code = 'CANCELLED'; return error;
}
function checkStart(task) {
  if (task.cancelled) throw cancelled();
}
async function post(instance, data, timeout = 20000) {
  const response = await fetch(instance.endpoint, {
    method: 'POST', headers: { authorization: 'Bearer ' + instance.token, 'Content-Type': 'application/json' },
    body: JSON.stringify(data), signal: AbortSignal.timeout(timeout), redirect: 'error'
  });
  const value = await response.json();
  if (!response.ok || !value.ok) {
    const error = Error(value.error?.code ?? 'Editor request failed');
    error.httpStatus = response.status; throw error;
  }
  return value.data;
}
function enqueue(instance, operation) {
  const next = instance.queue.catch(() => {}).then(operation); instance.queue = next; return next;
}
async function upload(instance, doc) {
  const name = relative(instance.folder, doc);
  const version = doc.version, text = doc.getText();
  try {
    await post(instance, { operation: 'buffer', sessionId: instance.sessionId,
      buffer: { path: name, version, text } });
    instance.excluded.delete(name);
    instance.acceptedVersions.set(name, version);
  } catch (error) {
    if (error.httpStatus !== 400) throw error;
    instance.excluded.set(name, { path: name, version, code: error.message });
    vscode.window.showWarningMessage('Code Intelligence excluded buffer ' + name + ': ' + error.message);
  }
}
async function shutdown(instance) {
  if (instance.cleanup) return instance.cleanup;
  instance.closed = true;
  if (instances.get(instance.key) === instance) instances.delete(instance.key);
  instance.cleanup = (async () => {
    try {
      await instance.queue.catch(() => {});
      if (instance.sessionId && !instance.exited) await post(instance, { operation: 'close', sessionId: instance.sessionId }, 5000);
    } catch {} finally {
      if (!instance.exited) instance.child.kill();
    }
  })();
  return instance.cleanup;
}
async function start(folder, context) {
  if (stopping) await stopping;
  if (!folder) throw Error('Open a local workspace first');
  const key = folder.uri.toString();
  if (pendingStarts.has(key)) return pendingStarts.get(key).promise;
  if (instances.has(key)) return instances.get(key);
  const task = { cancelled: false, cancelHandshake: null, instance: null, promise: null };
  pendingStarts.set(key, task);
  task.promise = startInstance(folder, context, task).finally(() => {
    if (pendingStarts.get(key) === task) pendingStarts.delete(key);
  });
  return task.promise;
}
async function startInstance(folder, context, task) {
  if (!vscode.workspace.isTrusted) throw Error('Workspace trust is required');
  if (folder.uri.scheme !== 'file') throw Error('A local filesystem workspace is required');
  checkStart(task);
  const runtime = userSetting('runtime', 'rust');
  if (!['rust', 'typescript'].includes(runtime)) throw Error('Choose a supported user-level runtime');
  const executable = runtime === 'rust'
    ? userSetting('nativeExecutable', '') || path.join(context.extensionPath, 'runtime', 'native', process.platform === 'win32' ? 'yoke.exe' : 'yoke')
    : userSetting('nodeExecutable', 'node');
  const cli = userSetting('cliPath', '') || path.join(context.extensionPath, 'runtime', 'dist', 'cli.js');
  const args = runtime === 'rust' ? ['--root', folder.uri.fsPath, '--timeout', '120000', 'editor']
    : [cli, 'editor', '--root', folder.uri.fsPath, '--timeout', '120000'];
  const child = spawn(executable, args, { shell: false, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const instance = { child, folder, key: folder.uri.toString(), queue: Promise.resolve(), sessionId: null,
    excluded: new Map(), acceptedVersions: new Map(), closed: false, exited: false, cleanup: null };
  task.instance = instance;
  child.once('exit', () => {
    instance.exited = true; instance.closed = true;
    if (instances.get(instance.key) === instance) instances.delete(instance.key);
  });
  child.stderr.on('data', () => {});
  child.on('error', () => {
    instance.closed = true;
    if (instances.get(instance.key) === instance) instances.delete(instance.key);
  });
  try {
    const connection = await new Promise((resolve, reject) => {
      let output = '', finished = false;
      const timer = setTimeout(() => finish(Error('Editor bridge startup timed out')), 20000);
      const finish = (error, value) => {
        if (finished) return; finished = true; clearTimeout(timer);
        task.cancelHandshake = null;
        child.stdout.removeListener('data', receive);
        child.removeListener('error', failed); child.removeListener('exit', exited);
        error ? reject(error) : resolve(value);
      };
      const failed = () => finish(Error('Cannot start configured ' + runtime + ' runtime'));
      const exited = () => finish(Error('Editor bridge exited before startup'));
      const receive = bytes => {
        output += bytes.toString();
        if (output.length > 16000) return finish(Error('Invalid bridge handshake'));
        let value;
        try { value = JSON.parse(output); } catch { return; }
        try {
          const url = new URL(value.endpoint);
          if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || url.pathname !== '/editor'
            || !url.port || url.username || url.password || url.search || url.hash || !/^[a-f0-9]{64}$/.test(value.token)) {
            throw Error('Invalid endpoint');
          }
          finish(null, value);
        } catch { finish(Error('Invalid bridge handshake')); }
      };
      task.cancelHandshake = () => finish(cancelled());
      child.on('error', failed); child.once('exit', exited); child.stdout.on('data', receive);
    });
    instance.endpoint = connection.endpoint; instance.token = connection.token; checkStart(task);
    instance.sessionId = (await post(instance, { operation: 'open' })).id;
    checkStart(task);
    // Commands still await pendingStarts; document events can now queue behind
    // the first upload instead of losing edits made while startup is pending.
    instances.set(instance.key, instance);
    for (const doc of vscode.workspace.textDocuments) {
      if (projectFor(doc)?.uri.toString() === instance.key) {
        await enqueue(instance, () => upload(instance, doc)); checkStart(task);
      }
    }
    await instance.queue; checkStart(task);
    if (instance.closed) throw Error('Editor bridge exited during startup');
    checkStart(task); instances.set(instance.key, instance); return instance;
  } catch (error) {
    await shutdown(instance); throw error;
  }
}
async function render(value, instance) {
  if (instance.excluded.size) value = { ...value, editorBufferFailures: [...instance.excluded.values()] };
  const doc = await vscode.workspace.openTextDocument({ content: JSON.stringify(value, null, 2), language: 'json' });
  await vscode.window.showTextDocument(doc, { preview: true });
}
function stop() {
  if (stopping) return stopping;
  const tasks = [...pendingStarts.values()], live = [...instances.values()];
  for (const task of tasks) { task.cancelled = true; task.cancelHandshake?.(); }
  stopping = (async () => {
    await Promise.allSettled(tasks.map(task => task.promise));
    await Promise.allSettled(live.map(shutdown));
  })().finally(() => { stopping = null; });
  return stopping;
}
function activate(context) {
  const guard = fn => async (...args) => {
    try { return await fn(...args); } catch (error) {
      if (error.code !== 'CANCELLED') vscode.window.showErrorMessage('Code Intelligence: ' + error.message);
    }
  };
  const activeFolder = () => {
    const doc = vscode.window.activeTextEditor?.document;
    return (doc && projectFor(doc)) || vscode.workspace.workspaceFolders?.[0];
  };
  const handleDoc = (doc, closed = false) => {
    const folder = projectFor(doc), instance = folder && instances.get(folder.uri.toString());
    if (!instance || instance.closed) return;
    void enqueue(instance, async () => {
      if (instance.closed) return;
      if (closed) {
        const name = relative(folder, doc);
        instance.excluded.delete(name);
        const version = instance.acceptedVersions.get(name);
        if (version === undefined) return;
        await post(instance, { operation: 'closeFile', sessionId: instance.sessionId, path: name, version });
        instance.acceptedVersions.delete(name);
      } else await upload(instance, doc);
    }).catch(error => vscode.window.showWarningMessage('Code Intelligence buffer update: ' + error.message));
  };
  context.subscriptions.push(
    vscode.commands.registerCommand('codeIntelligence.start', guard(async () => {
      await start(activeFolder(), context);
      vscode.window.showInformationMessage('Code Intelligence editor bridge started. Source writes remain disabled.');
    })),
    vscode.commands.registerCommand('codeIntelligence.context', guard(async () => {
      const query = await vscode.window.showInputBox({ prompt: 'Find code context across saved and unsaved source', ignoreFocusOut: true });
      if (!query) return;
      const instance = await start(activeFolder(), context);
      await render(await enqueue(instance, () => post(instance, { operation: 'tool', sessionId: instance.sessionId,
        tool: 'ci_context', arguments: { query, maxBytes: 32000 } })), instance);
    })),
    vscode.commands.registerCommand('codeIntelligence.snapshot', guard(async () => {
      const instance = await start(activeFolder(), context);
      await render(await enqueue(instance, () => post(instance, { operation: 'capture', sessionId: instance.sessionId })), instance);
    })),
    vscode.commands.registerCommand('codeIntelligence.stop', guard(stop)),
    vscode.workspace.onDidOpenTextDocument(doc => handleDoc(doc)),
    vscode.workspace.onDidChangeTextDocument(event => handleDoc(event.document)),
    vscode.workspace.onDidCloseTextDocument(doc => handleDoc(doc, true)),
    vscode.workspace.onDidSaveTextDocument(doc => {
      const folder = projectFor(doc), instance = folder && instances.get(folder.uri.toString());
      if (instance && !instance.closed) void enqueue(instance, () => post(instance, { operation: 'capture', sessionId: instance.sessionId }))
        .catch(error => vscode.window.showWarningMessage('Code Intelligence disk conflict: ' + error.message));
    }),
    { dispose: () => { void stop(); } }
  );
}
module.exports = { activate, deactivate: stop };
