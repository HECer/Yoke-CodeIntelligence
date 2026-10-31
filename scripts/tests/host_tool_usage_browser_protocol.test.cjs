'use strict';
// Pure admission functions: importing starts no driver, browser or process.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const usage = require('../host_tool_usage_browser.cjs');
const view = () => ({schema: 'native_tool_usage_display_v1', estimator: 'CHAR_COUNT',
  scope: 'current_session_instance_successful_serena_operations', revision: '4', hostGrantRemoteAttempts: '0',
  tools: {read_file: {numTimesCalled: '2', inputTokens: '13', outputTokens: '18446744073709551615'}}});
const input = () => ({id: 'a'.repeat(64), sessionStateSha256: 'b'.repeat(64), project: 'default',
  action: {kind: 'resetToolUsage', expectedStatisticsRevision: '4'}});
const conflict = () => ({schema: 'native_dashboard_management_v1', id: 'a'.repeat(64),
  error: 'tool_usage_statistics_changed_refresh', notApplied: true});
const success = () => ({schema: 'native_dashboard_management_v1', id: 'a'.repeat(64), outcome: 'completed',
  metadataChanged: false, fileChanged: false, sourceWritten: false, toolUsageChanged: true, data: {}});

test('count-only totals retain exact u64 values and larger sums', () => {
  const model = usage.counts(view());
  assert.equal(model.totals.tokens, 18446744073709551628n);
  assert.equal(model.totals.calls, 2n);
  assert.equal(model.rows[0].output, 18446744073709551615n);
});
test('malformed and overflowing decimal claims refuse', () => {
  for (const value of [4, true, null, '01', '-1', '1e3', '18446744073709551616', '', '1'.repeat(21)])
    assert.throws(() => usage.decimal(value));
});
test('unselected estimators, remote authority and arbitrary payload fields refuse', () => {
  for (const change of [{estimator: 'TIKTOKEN_GPT4O'}, {scope: 'all_sessions'}, {hostGrantRemoteAttempts: '1'},
    {tools: []}, {extra: 'private text'}, {tools: {read_file: {numTimesCalled: '1', inputTokens: '0',
      outputTokens: '1', resultText: 'private'}}}]) assert.throws(() => usage.counts({...view(), ...change}));
  const many = Object.fromEntries(Array.from({length: 97}, (_, i) => ['tool_' + i, view().tools.read_file]));
  assert.throws(() => usage.counts({...view(), tools: many}));
});
test('reset admission binds exact owning request and reviewed statistics revision', () => {
  assert.deepEqual(usage.request(input()), {kind: 'resetToolUsage', id: 'a'.repeat(64), expectedRevision: '4'});
  for (const change of [{id: 'foreign'}, {project: 'other'}, {sessionStateSha256: 'x'.repeat(64)},
    {extra: true}, {action: {kind: 'shutdown'}}, {action: {...input().action, expectedStatisticsRevision: '04'}},
    {action: {...input().action, retry: true}}]) assert.throws(() => usage.request({...input(), ...change}));
});
test('only the exact real stale-reset refusal is accepted at HTTP 409', () => {
  const selected = usage.request(input());
  assert.deepEqual(usage.response(selected, 409, conflict()), {status: 409, notApplied: true, expectedRevision: '4'});
  for (const change of [{id: 'b'.repeat(64)}, {notApplied: false}, {error: 'session_changed'},
    {schema: 'other'}, {extra: 'private'}]) assert.throws(() => usage.response(selected, 409, {...conflict(), ...change}));
  for (const status of [200, 403, 422, 500]) assert.throws(() => usage.response(selected, status, conflict()));
});
test('successful reset must attest no source or persisted metadata write', () => {
  const selected = usage.request(input());
  assert.equal(usage.response(selected, 200, success()).resetCompleted, true);
  for (const field of ['metadataChanged', 'fileChanged', 'sourceWritten'])
    assert.throws(() => usage.response(selected, 200, {...success(), [field]: true}));
  for (const change of [{id: 'b'.repeat(64)}, {outcome: 'pending'}, {toolUsageChanged: false}])
    assert.throws(() => usage.response(selected, 200, {...success(), ...change}));
  assert.throws(() => usage.response(selected, 409, success()));
});

function flowFixture(mode = '') {
  const events = [], pages = ['desktop', 'mobile'].map(name => ({name,
    width: name === 'desktop' ? 1440 : 390, session: name, receipts: []}));
  let expected = '4';
  const flow = usage.create({need: value => assert.ok(value),
    evaluate: async (p, expression) => expression.startsWith('(()=>{const ids=') ? {
      width: p.width, scrollWidth: p.width, nodes: Array.from({length: 10}, () => ({text: 'owned'}))} : true,
    until: async (p, expression) => { events.push(['observe', p.name, expression]); },
    click: async (p, selector) => {
      events.push(['click', p.name, selector]);
      if (mode === 'reset-on-open' && selector === '#clear-usage') usage.observeRequest(p, input());
    },
    confirm: async (p, prefix) => {
      events.push(['confirm', p.name, prefix]);
      const body = input();
      body.action.expectedStatisticsRevision = p.name === 'desktop' ? '4' : '5';
      usage.observeRequest(p, body);
    },
    cdp: () => ({send: async (method, data) => { events.push(['input', method, data]); }}),
    captureSnapshot: async (p, phase) => ({screenshot: p.name + '-' + phase + '.png'}),
    receipt: async (p, kind) => {
      const data = p.name === 'desktop' ? {status: 409, notApplied: true, expectedRevision: expected}
        : {status: 200, resetCompleted: true, expectedRevision: '5'};
      if (mode === 'wrong-revision') data.expectedRevision = '3';
      p.receipts.push({kind, data});
      if (mode === 'retry') usage.observeRequest(p, input());
      return data;
    }});
  return {flow, pages, events, setExpected: value => { expected = value; }};
}
test('actual flow calls real input helpers for filter/sort/cancel/stale reset and six distinct captures', async () => {
  const {flow, pages, events} = flowFixture(), initial = view(), advanced = {...initial, revision: '5'};
  const before = await flow.before(pages, initial);
  const pending = await flow.pending(pages, initial);
  const conflict = await flow.after(pages, 'desktop', advanced);
  const cleared = await flow.after(pages, 'mobile', advanced);
  const images = [...before.states, ...pending.states, conflict, cleared].map(row => row.screenshot);
  assert.equal(new Set(images).size, 6);
  assert.equal(pending.cancelNoRequest, true);
  assert.equal(conflict.staleResetRefused, true);
  assert.equal(cleared.explicitResetObserved, true);
  assert.deepEqual(usage.finalRequests(pages), {desktop: 1, mobile: 1});
  assert.equal(events.filter(row => row[0] === 'click' && row[2].includes('value="cancel"')).length, 1);
  assert.equal(events.filter(row => row[0] === 'confirm').length, 2);
  assert.ok(events.some(row => row[0] === 'input' && row[2].key === 'ArrowDown'));
  assert.ok(events.some(row => row[0] === 'input' && row[1] === 'Input.insertText' && row[2].text === 'read_file'));
});
test('a delayed retry after the desktop phase cannot hide in a new baseline or shutdown', async () => {
  const f = flowFixture(), initial = view(), advanced = {...initial, revision: '5'};
  await f.flow.pending(f.pages, initial);
  await f.flow.after(f.pages, 'desktop', advanced);
  assert.throws(() => usage.observeRequest(f.pages[0], input()));
  assert.throws(() => usage.finalRequests(f.pages));
});
test('the actual CDP event handler refuses a reset POST while close drains events', async () => {
  const fs = require('node:fs'), vm = require('node:vm');
  const source = fs.readFileSync(require.resolve('../host_history_browser.cjs'), 'utf8');
  const handler = source.slice(source.indexOf('class CDP {'), source.indexOf('async function evaluate('));
  const context = vm.createContext({config: {variant: 'tool-usage'}});
  vm.runInContext(handler + '\nthis.event = CDP.prototype.event;', context);
  const event = method => ({method: 'Network.requestWillBeSent', params: {request: {method}}});
  await assert.rejects(context.event.call({closing: true}, event('POST')), /statistics-request-during-close/);
  await context.event.call({closing: true}, event('GET'));
});
test('a request on dialog open, wrong reviewed revision or automatic reset retry refuses the flow', async () => {
  const initial = view(), advanced = {...initial, revision: '5'};
  const opened = flowFixture('reset-on-open');
  await assert.rejects(opened.flow.pending(opened.pages, initial));
  for (const mode of ['wrong-revision', 'retry']) {
    const f = flowFixture(mode);
    await f.flow.pending(f.pages, initial);
    await assert.rejects(f.flow.after(f.pages, 'desktop', advanced));
  }
});
