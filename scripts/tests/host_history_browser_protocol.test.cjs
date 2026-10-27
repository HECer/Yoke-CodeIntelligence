'use strict';
// In-memory inherited-pipe doubles only: no browser, worker, process or network.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {EventEmitter} = require('node:events');
const {CDP, CdpPipeEOF} = require('../host_history_browser.cjs');

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
