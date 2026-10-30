'use strict';
// In-memory inherited-pipe doubles only: no browser, worker, process or network.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {EventEmitter} = require('node:events');
const {CDP, CdpPipeEOF} = require('../host_history_browser.cjs');
const fs = require('node:fs');
const vm = require('node:vm');

function startFixture(variant = 'native', environment = {}, occupied = false) {
  const source = fs.readFileSync(require.resolve('../host_history_browser.cjs'), 'utf8');
  const helper = source.slice(source.indexOf('function admitStart('), source.indexOf('async function start('));
  const path = require('node:path').win32, probes = [];
  const context = {path, URL, __dirname: 'C:\\owned-repository\\scripts',
    need: value => { if (!value) throw Error('admission-or-assertion'); },
    fs: {existsSync: name => { probes.push(name); return occupied; }},
    process: {platform: 'win32', cwd: () => 'C:\\owned-fixture', env: {GITHUB_ACTIONS: 'true',
      YOKE_BROWSER_WORKER_NONCE: 'a'.repeat(64), YOKE_BROWSER_WORKER_VARIANT: variant, ...environment}}};
  vm.createContext(context);
  vm.runInContext(helper + '\nthis.admit = admitStart;', context);
  const suffix = {native: '', serena: '-serena', 'persistent-python': '-persistent-python', http: '-http'}[variant];
  const input = {phase: 'start', variant, browser: 'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
    profile: 'C:\\owned-fixture\\edge-profile', output: 'C:\\owned-repository\\validation\\ci-host-document-history-browser' + suffix,
    url: 'http://127.0.0.1:1234/#' + 'b'.repeat(64), document: 'new', draft: '\ufeffπ 🦀 e\u0301\r\n',
    dashboardSha256: 'c'.repeat(64)};
  return {input, context, probes, admit: value => context.admit(value)};
}

test('start admits every fixed variant only at its exact repository evidence path', () => {
  for (const variant of ['native', 'serena', 'persistent-python', 'http']) {
    const f = startFixture(variant);
    assert.equal(f.admit(f.input).href, f.input.url);
    assert.deepEqual(f.probes, ['C:\\owned-fixture\\edge-profile']);
  }
});

test('start refuses unbound variants, occupied profiles and all alternate evidence paths before browser work', () => {
  for (const variant of ['native', 'serena', 'persistent-python', 'http']) {
    const cases = [{variant: '__proto__'}, {variant: 'foreign'}, {variant: ['native']},
      {variant: variant === 'native' ? 'serena' : 'native'}, {phase: 'before'}, {extra: 'unadmitted'},
      {output: 'C:\\foreign\\ci-host-document-history-browser' + (variant === 'native' ? '' : '-' + variant)},
      {output: 'C:\\owned-repository\\validation\\foreign\\..\\' + require('node:path').win32.basename(startFixture(variant).input.output)},
      {output: 'relative'}, {profile: 'C:\\foreign\\edge-profile'},
      {browser: 'C:\\owned-repository\\msedge.exe'}, {url: 'https://127.0.0.1:1234/#' + 'b'.repeat(64)}];
    for (const changed of cases) {
      const f = startFixture(variant);
      assert.throws(() => f.admit({...f.input, ...changed}), /admission-or-assertion/);
      assert.deepEqual(f.probes, []);
    }
    const occupied = startFixture(variant, {}, true);
    assert.throws(() => occupied.admit(occupied.input), /admission-or-assertion/);
    assert.equal(occupied.probes.length, 1);
  }
});

test('start retains Windows CI and nonce guards before any path observation', () => {
  for (const environment of [{GITHUB_ACTIONS: 'false'}, {YOKE_BROWSER_WORKER_NONCE: 'invalid'},
    {YOKE_BROWSER_WORKER_VARIANT: 'serena'}]) {
    const f = startFixture('native', environment);
    assert.throws(() => f.admit(f.input), /admission-or-assertion/);
    assert.deepEqual(f.probes, []);
  }
  const f = startFixture(); f.context.process.platform = 'linux';
  assert.throws(() => f.admit(f.input), /admission-or-assertion/);
  assert.deepEqual(f.probes, []);
});

function focusFixture(sequence, blocked = false) {
  const source = fs.readFileSync(require.resolve('../host_history_browser.cjs'), 'utf8');
  const helper = source.slice(source.indexOf('async function focusPendingControlWithKeyboard('),
    source.indexOf('async function confirm('));
  const requests = [];
  let focus = '', position = 0;
  const context = {
    until: async (p, expression) => {
      assert.equal(expression, "!document.getElementById('pending-documents').disabled");
      if (blocked) throw Error('owned-control-still-disabled');
    },
    evaluate: async (p, expression) => {
      assert.equal(expression, "document.activeElement?.id==='pending-documents'");
      return focus === 'pending-documents';
    },
    cdp: {send: async (method, params, session) => {
      assert.equal(method, 'Input.dispatchKeyEvent');
      assert.equal(session, 'owned-session');
      assert.equal(params.key, 'Tab'); assert.equal(params.code, 'Tab');
      assert.equal(params.windowsVirtualKeyCode, 9);
      requests.push(params.type);
      if (params.type === 'keyUp') focus = sequence[position++] || '';
    }},
  };
  vm.createContext(context);
  vm.runInContext(helper + '\nthis.run = focusPendingControlWithKeyboard;', context);
  return {run: () => context.run({session: 'owned-session'}), requests};
}

test('pending focus follows actual Tab input after a completed transiently disabled request', async () => {
  const f = focusFixture(['document-content', '', 'pending-documents']);
  await f.run();
  assert.deepEqual(f.requests, ['keyDown', 'keyUp', 'keyDown', 'keyUp', 'keyDown', 'keyUp']);
});

test('pending focus refuses disabled and unreachable controls within twelve Tab pairs', async () => {
  const disabled = focusFixture([], true);
  await assert.rejects(disabled.run(), /owned-control-still-disabled/);
  assert.deepEqual(disabled.requests, []);
  const boundary = focusFixture([...Array(11).fill('document-content'), 'pending-documents']);
  await boundary.run();
  assert.equal(boundary.requests.length, 24);
  const unreachable = focusFixture(Array(12).fill('document-content'));
  await assert.rejects(unreachable.run(), /keyboard-focus-deadline/);
  assert.equal(unreachable.requests.length, 24);
  assert.ok(unreachable.requests.every((type, i) => type === (i % 2 ? 'keyUp' : 'keyDown')));
});

function pendingPhaseFixture({blurOnRequest = true, mode = 'normal'} = {}) {
  // Run the actual pending-phase orchestration, click and keyboard helpers.
  // Only CDP/DOM/network observations are doubled; this is no browser acceptance.
  const source = fs.readFileSync(require.resolve('../host_history_browser.cjs'), 'utf8');
  const helpers = source.slice(source.indexOf('async function click('), source.indexOf('async function confirm('));
  const body = source.slice(source.indexOf("  if (phase === 'pending')"), source.indexOf("  if (phase === 'after')"));
  const pages = ['desktop', 'mobile'].map(name => ({name, session: name, receipts: [], inputs: [], snapshots: 0}));
  for (const p of pages) {
    const nodes = Object.fromEntries(['pending-documents', 'updated', 'document-content', 'plan-document', 'apply-document']
      .map(id => [id, {id, disabled: false, readOnly: false, textContent: 'before',
        getClientRects: () => [1], scrollIntoView: () => {},
        getBoundingClientRect: () => ({x: 10, y: 10, width: 20, height: 20})}]));
    const document = {activeElement: {id: ''}, getElementById: id => nodes[id],
      querySelector: selector => mode === 'missing-target' ? null : nodes[selector.slice(1)]};
    p.nodes = nodes; p.document = document; p.dom = vm.createContext({document});
  }
  const context = {pages, phase: 'pending', commands: 3, scriptErrors: 0, consoleErrors: 0,
    checkpoint: '', observedViewport: '', Number, JSON,
    need: value => { if (!value) throw Error('admission-or-assertion'); },
    evaluate: async (p, expression) => vm.runInContext(expression, p.dom),
    until: async (p, expression) => {
      if (expression.startsWith("document.getElementById('updated').textContent!==")) p.nodes.updated.textContent = 'after';
      if (!vm.runInContext(expression, p.dom)) throw Error('dom-deadline-double');
    },
    receipt: async (p, kind, since) => {
      const item = p.receipts.slice(since).find(row => row.kind === kind);
      if (!item) throw Error('receipt-deadline-double');
      return item.data;
    },
    snapshot: async (p, state) => { assert.equal(state, 'pending'); p.snapshots++; return {}; },
    cdp: {send: async (method, params, session) => {
      const p = pages.find(page => page.session === session);
      p.inputs.push({method, type: params.type});
      if (method === 'Input.dispatchMouseEvent') {
        assert.equal(params.button, 'left'); assert.equal(params.clickCount, 1);
        if (params.type === 'mousePressed') p.document.activeElement = p.nodes['pending-documents'];
        else {
          assert.equal(params.type, 'mouseReleased');
          // manage() disables its initiating button and may blur it before
          // the CDP acknowledgement; the later reply re-enables the control.
          p.nodes['pending-documents'].disabled = true;
          if (blurOnRequest) p.document.activeElement = {id: ''};
          p.nodes['document-content'].readOnly = true;
          p.nodes['plan-document'].disabled = mode !== 'enabled-review';
          p.nodes['apply-document'].disabled = true;
          if (mode !== 'missing-receipt') p.receipts.push({kind: 'pending', data: {
            complete: mode === 'wrong-reply', blockedByHistoryArchive: true, journals: [],
            fileChanged: false, metadataChanged: false, sourceWritten: false}});
          p.nodes['pending-documents'].disabled = mode === 'still-disabled';
        }
      } else {
        assert.equal(method, 'Input.dispatchKeyEvent'); assert.equal(params.key, 'Tab');
        assert.equal(params.code, 'Tab'); assert.equal(params.windowsVirtualKeyCode, 9);
        if (params.type === 'keyUp') p.document.activeElement = p.nodes[
          mode === 'unreachable-focus' || p.document.activeElement.id === 'pending-documents' ? 'document-content' : 'pending-documents'];
        else assert.equal(params.type, 'keyDown');
      }
    }},
  };
  vm.createContext(context);
  vm.runInContext(helpers + '\nthis.run = async function(){\n' + body + '\n};', context);
  return {pages, run: () => context.run()};
}

test('pending phase verifies real-input focus after the request reply even when the click immediately blurs', async () => {
  for (const blurOnRequest of [true, false]) {
    const f = pendingPhaseFixture({blurOnRequest});
    const result = await f.run();
    assert.equal(result.states.length, 2);
    for (const p of f.pages) {
      assert.equal(p.receipts.length, 2); assert.equal(p.snapshots, 1);
      assert.equal(p.document.activeElement.id, 'pending-documents');
      assert.deepEqual(p.inputs.filter(row => row.method === 'Input.dispatchMouseEvent').map(row => row.type),
        ['mousePressed', 'mouseReleased', 'mousePressed', 'mouseReleased']);
    }
  }
});

test('pending phase still refuses missing targets, replies, blocking state and usable keyboard focus', async () => {
  for (const mode of ['missing-target', 'missing-receipt', 'wrong-reply', 'enabled-review', 'still-disabled', 'unreachable-focus']) {
    const f = pendingPhaseFixture({mode});
    await assert.rejects(f.run(), /admission-or-assertion|receipt-deadline-double|dom-deadline-double|keyboard-focus-deadline/);
    assert.ok(f.pages.every(p => p.snapshots === 0));
    assert.ok(f.pages.every(p => p.inputs.length <= 30));
  }
});

function fixture(respond) {
  const output = new EventEmitter(), input = new EventEmitter();
  const requests = [], failures = [];
  input.write = (raw, callback) => {
    const request = JSON.parse(raw.subarray(0, raw.length - 1));
    requests.push(request);
    respond(request, output, callback);
  };
  const cdp = new CDP({stdio: [null, null, null, input, output]}, error => failures.push(error));
  return {cdp, output, requests, failures};
}
function reply(output, request, result = {}) {
  output.emit('data', Buffer.from(JSON.stringify({id: request.id, result}) + '\0'));
}
const tick = () => new Promise(resolve => setImmediate(resolve));

test('close drains a previously admitted request handler before Browser.close', async () => {
  let held;
  const f = fixture((request, output, callback) => {
    callback();
    if (request.method === 'Fetch.continueRequest') held = () => reply(output, request);
    else { assert.equal(request.method, 'Browser.close'); output.emit('end'); }
  });
  f.cdp.event = () => f.cdp.send('Fetch.continueRequest');
  f.output.emit('data', Buffer.from('{"method":"Fetch.requestPaused"}\0'));
  const close = f.cdp.close();
  await tick();
  assert.deepEqual(f.requests.map(r => r.method), ['Fetch.continueRequest']);
  held();
  await close;
  assert.deepEqual(f.requests.map(r => r.method), ['Fetch.continueRequest', 'Browser.close']);
  assert.equal(f.cdp.pending.size, 0);
  assert.deepEqual(f.failures, []);
});

test('failure of an in-flight handler refuses close rather than hiding it as teardown', async () => {
  let refuse;
  const f = fixture((request, output, callback) => {
    callback();
    refuse = () => output.emit('data', Buffer.from(JSON.stringify({id: request.id, error: {code: -1}}) + '\0'));
  });
  f.cdp.event = () => f.cdp.send('Fetch.continueRequest');
  f.output.emit('data', Buffer.from('{"method":"Fetch.requestPaused"}\0'));
  const close = f.cdp.close();
  refuse();
  await assert.rejects(close, /unsupported-or-refused-cdp/);
  assert.deepEqual(f.requests.map(r => r.method), ['Fetch.continueRequest']);
  assert.equal(f.failures.length, 1);
});

test('a close acknowledgement succeeds without a fabricated EOF receipt', async () => {
  const f = fixture((request, output, callback) => { callback(); reply(output, request); });
  await f.cdp.close();
  assert.deepEqual(f.requests.map(r => r.method), ['Browser.close']);
  assert.deepEqual(f.failures, []);
});

test('only typed transport EOF is permitted before the close acknowledgement', async () => {
  for (const error of [new Error('cdp-eof'), new Error('cdp-deadline'), new Error('unsupported-or-refused-cdp')]) {
    const f = fixture(() => { throw Error('Must use the selected error'); });
    f.cdp.send = async () => { throw error; };
    await assert.rejects(f.cdp.close(), value => value === error);
  }
  const f = fixture((request, output, callback) => { callback(); output.emit('end'); });
  await f.cdp.close();
  assert.deepEqual(f.requests.map(r => r.method), ['Browser.close']);
  assert.deepEqual(f.failures, []);
  assert.ok(new CdpPipeEOF() instanceof Error);
});

test('Browser.close protocol refusal and pipe write failure remain failures', async () => {
  const refused = fixture((request, output, callback) => {
    callback();
    output.emit('data', Buffer.from(JSON.stringify({id: request.id, error: {code: -32601}}) + '\0'));
  });
  await assert.rejects(refused.cdp.close(), /unsupported-or-refused-cdp/);
  const failedWrite = fixture((request, output, callback) => callback(new Error('owned-write-refusal')));
  await assert.rejects(failedWrite.cdp.close(), /owned-write-refusal/);
});

test('partial EOF and preexisting unowned pending requests cannot become successful close', async () => {
  const f = fixture((request, output, callback) => {
    callback(); output.emit('data', Buffer.from('{')); output.emit('end');
  });
  await assert.rejects(f.cdp.close(), /unexpected-cdp-eof/);
  assert.equal(f.failures.length, 1);
  const pending = fixture(() => { throw Error('Must refuse before sending close'); });
  pending.cdp.pending.set(99, {});
  await assert.rejects(pending.cdp.close(), /admission-or-assertion/);
  assert.deepEqual(pending.requests, []);
});
