'use strict';
// Additive real-page statistics flow. No application writes through evaluation,
// fetch shims, synthetic responses, test-only endpoints or external network.
const assert = value => { if (!value) throw Error('statistics-admission-or-assertion'); };
const keys = (value, names) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).sort().join(',') === names.sort().join(',');
function decimal(value) {
  assert(typeof value === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(value)
    && BigInt(value) <= 18446744073709551615n);
  return BigInt(value);
}
function counts(value) {
  assert(keys(value, ['schema', 'estimator', 'scope', 'revision', 'hostGrantRemoteAttempts', 'tools'])
    && value.schema === 'native_tool_usage_display_v1' && value.estimator === 'CHAR_COUNT'
    && value.scope === 'current_session_instance_successful_serena_operations' && value.hostGrantRemoteAttempts === '0');
  decimal(value.revision);
  assert(value.tools && typeof value.tools === 'object' && !Array.isArray(value.tools)
    && Object.keys(value.tools).length <= 96);
  const totals = {calls: 0n, input: 0n, output: 0n};
  const rows = Object.entries(value.tools).map(([name, row]) => {
    assert(/^[A-Za-z0-9_]{1,100}$/.test(name) && keys(row, ['numTimesCalled', 'inputTokens', 'outputTokens']));
    const result = {name, calls: decimal(row.numTimesCalled), input: decimal(row.inputTokens), output: decimal(row.outputTokens)};
    for (const key of Object.keys(totals)) totals[key] += result[key];
    return {...result, tokens: result.input + result.output};
  });
  return {rows, totals: {...totals, tokens: totals.input + totals.output}};
}
function request(value) {
  assert(keys(value, ['id', 'sessionStateSha256', 'project', 'action']) && /^[a-f0-9]{64}$/.test(value.id)
    && /^[a-f0-9]{64}$/.test(value.sessionStateSha256) && value.project === 'default'
    && keys(value.action, ['kind', 'expectedStatisticsRevision']) && value.action.kind === 'resetToolUsage');
  decimal(value.action.expectedStatisticsRevision);
  return {kind: 'resetToolUsage', id: value.id, expectedRevision: value.action.expectedStatisticsRevision};
}
function response(selected, status, reply) {
  assert(reply && reply.schema === 'native_dashboard_management_v1' && reply.id === selected.id);
  if (status === 409) {
    assert(keys(reply, ['schema', 'id', 'error', 'notApplied']) && reply.notApplied === true
      && reply.error === 'tool_usage_statistics_changed_refresh');
    return {status, notApplied: true, expectedRevision: selected.expectedRevision};
  }
  assert(status === 200 && reply.outcome === 'completed' && reply.metadataChanged === false
    && reply.fileChanged === false && reply.sourceWritten === false && reply.toolUsageChanged === true);
  return {status, resetCompleted: true, expectedRevision: selected.expectedRevision};
}
function observeRequest(page, body) {
  const selected = request(body);
  page.resetRequestCount = (page.resetRequestCount || 0) + 1;
  assert(page.resetRequestCount === 1); // Any delayed retry/cancel POST is a hard failure.
  return selected;
}
function finalRequests(pages) {
  assert(pages.length === 2 && pages.map(p => p.name).sort().join(',') === 'desktop,mobile'
    && pages.every(p => p.resetRequestCount === 1));
  return {desktop: 1, mobile: 1};
}

function create(api) {
  const {need, evaluate, until, click, confirm, cdp, captureSnapshot, receipt} = api;
  let reviewedRevision;
  const format = value => value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const resetCount = p => p.resetRequestCount || 0;
  async function keyboard(p, key, code, windowsVirtualKeyCode, modifiers = 0) {
    for (const type of ['keyDown', 'keyUp']) await cdp().send('Input.dispatchKeyEvent',
      {type, key, code, windowsVirtualKeyCode, modifiers}, p.session);
  }
  async function filter(p, text) {
    await click(p, '#usage-filter', 'usage-filter');
    await keyboard(p, 'a', 'KeyA', 65, 2);
    await keyboard(p, 'Backspace', 'Backspace', 8);
    if (text) await cdp().send('Input.insertText', {text}, p.session);
    await until(p, `document.getElementById('usage-filter').value===${JSON.stringify(text)}`);
  }
  async function refresh(p, value) {
    const expected = counts(value);
    await click(p, '#refresh-usage');
    await until(p, `document.getElementById('usage-calls').textContent===${JSON.stringify(format(expected.totals.calls))}
      && document.getElementById('usage-input').textContent===${JSON.stringify(format(expected.totals.input))}
      && document.getElementById('usage-output').textContent===${JSON.stringify(format(expected.totals.output))}
      && document.getElementById('usage-total').textContent===${JSON.stringify(format(expected.totals.tokens))}`);
    need(await evaluate(p, `document.getElementById('usage-estimator').textContent==='Character estimate'`));
    return expected;
  }
  async function rows(p, value, query = '', order = 'calls') {
    const model = counts(value);
    const expected = model.rows.filter(row => row.name.includes(query)).sort((a, b) =>
      a[order] === b[order] ? (a.name < b.name ? -1 : a.name > b.name ? 1 : 0) : a[order] > b[order] ? -1 : 1)
      .map(row => [row.name, format(row.calls), format(row.input), format(row.output)]);
    const expression = `Array.from(document.querySelectorAll('#usage-results tbody tr'),tr=>
      Array.from(tr.cells).slice(0,4).map((cell,index)=>index===0?cell.querySelector('code').textContent:
        Array.from(cell.childNodes).filter(n=>n.nodeType===3).map(n=>n.textContent).join('')))`;
    await until(p, `JSON.stringify(${expression})===${JSON.stringify(JSON.stringify(expected))}`);
  }
  async function snapshot(p, state) {
    const observation = await evaluate(p, `(()=>{const ids=['usage-estimator','usage-calls','usage-input','usage-output',
      'usage-total','usage-status','usage-filter','usage-sort','clear-usage','management-status'];
      return {width:innerWidth,scrollWidth:document.documentElement.scrollWidth,
        focus:document.activeElement?.id||'',dialogOpen:document.getElementById('confirm-dialog').open,
        nodes:ids.map(id=>{const e=document.getElementById(id),r=e.getBoundingClientRect();
          return {id,visible:!!e.getClientRects().length,disabled:e.disabled===true,text:e.textContent,
            x:r.x,y:r.y,width:r.width,height:r.height};})};})()`);
    need(observation.width === p.width && observation.scrollWidth <= p.width && observation.nodes.length === 10);
    for (const node of observation.nodes) need(typeof node.text === 'string' && Buffer.byteLength(node.text) <= 8000);
    return captureSnapshot(p, state, observation);
  }
  async function openReset(p) {
    await until(p, `!document.getElementById('clear-usage').disabled`);
    await click(p, '#clear-usage');
    await until(p, `document.getElementById('confirm-dialog').open`);
    need(await evaluate(p, `document.getElementById('confirm-message').textContent===
      'Clear the counted calls and token totals in this session instance? Remote counting attempts are kept.'
      && document.activeElement?.value==='cancel'`));
  }
  async function before(pages, value) {
    counts(value);
    const states = [];
    for (const p of pages) {
      await refresh(p, value);
      await click(p, '#usage-sort', 'usage-sort');
      await keyboard(p, 'Home', 'Home', 36);
      await keyboard(p, 'ArrowDown', 'ArrowDown', 40);
      await keyboard(p, 'Tab', 'Tab', 9);
      await until(p, `document.getElementById('usage-sort').value==='calls'`);
      await filter(p, 'read_file');
      await rows(p, value, 'read_file');
      await filter(p, '');
      await rows(p, value);
      states.push(await snapshot(p, 'before'));
    }
    return {states, filterAndSortObserved: true};
  }
  async function pending(pages, value) {
    counts(value);
    const states = [], prior = pages.map(resetCount);
    // Cancel is actual dialog input. Keep the desktop confirmation open while
    // the parent executes one more successful Serena call on the same Session.
    for (const p of [...pages].reverse()) {
      await refresh(p, value);
      await openReset(p);
      states.push(await snapshot(p, 'pending'));
      if (p.name === 'mobile') {
        await click(p, '#confirm-dialog button[value="cancel"]');
        await until(p, `!document.getElementById('confirm-dialog').open`);
      }
    }
    need(pages.every((p, i) => resetCount(p) === prior[i]));
    reviewedRevision = value.revision;
    return {states, cancelNoRequest: true, expectedRevision: reviewedRevision};
  }
  async function after(pages, viewport, value) {
    counts(value);
    const p = pages.find(row => row.name === viewport), since = p.receipts.length, count = resetCount(p);
    if (viewport === 'desktop') {
      need(BigInt(value.revision) === BigInt(reviewedRevision) + 1n);
      await confirm(p, 'Clear the counted calls');
      const reply = await receipt(p, 'resetToolUsage', since);
      need(reply.status === 409 && reply.notApplied === true && reply.expectedRevision === reviewedRevision);
      await until(p, `document.getElementById('management-status').textContent===
        'Tool totals or the session changed. Review the latest totals and confirm clearing again.'`);
      await refresh(p, value);
      await rows(p, value);
      need(resetCount(p) === count + 1);
      return {...await snapshot(p, 'after'), staleResetRefused: true, resetRequests: 1};
    }
    need(viewport === 'mobile');
    await refresh(p, value);
    await rows(p, value);
    await openReset(p);
    await confirm(p, 'Clear the counted calls');
    const reply = await receipt(p, 'resetToolUsage', since);
    need(reply.status === 200 && reply.resetCompleted === true && reply.expectedRevision === value.revision);
    const cleared = {...value, revision: (BigInt(value.revision) + 1n).toString(), tools: {}};
    await refresh(p, cleared);
    await until(p, `document.getElementById('management-status').textContent==='Tool totals cleared.'
      && document.getElementById('clear-usage').disabled
      && document.querySelectorAll('#usage-results tbody tr').length===0`);
    need(resetCount(p) === count + 1);
    return {...await snapshot(p, 'after'), explicitResetObserved: true, resetRequests: 1};
  }
  return {before, pending, after};
}
module.exports = {decimal, counts, request, response, observeRequest, finalRequests, create};
