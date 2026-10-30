'use strict';
// Dependency-free CDP over inherited pipes. No server, websocket URL, page
// replacement, route fulfillment, fetch shim, or protection-bypass flags.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawn} = require('node:child_process');
const {TextDecoder} = require('node:util');
const FRAME = 4_000_000, JSON_LIMIT = 1_000_000, PNG_LIMIT = 2_000_000;
const decoder = new TextDecoder('utf-8', {fatal: true});
const hash = data => crypto.createHash('sha256').update(data).digest('hex');
const need = value => { if (!value) throw Error('admission-or-assertion'); };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const deadline = async (promise, ms) => {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(Error('deadline')), ms);
  })]); } finally { clearTimeout(timer); }
};
let config, browser, cdp, pages = [], closing = false, fatal = false, finished = false;
let input = Buffer.alloc(0), serial = Promise.resolve(), commands = 0, commandBytes = 0;
let browserLogs = 0, eventCount = 0, scriptErrors = 0, consoleErrors = 0, pngTotal = 0;
let closeExit, pipeEOF = false, browserOutputEOF = 0, phase = 'start';
let checkpoint = 'phase-entry', observedViewport = 'none';

function safe(value) {
  const text = JSON.stringify(value);
  need(Buffer.byteLength(text) <= JSON_LIMIT && !text.includes(config.url)
    && !text.includes(new URL(config.url).hash.slice(1)) && !text.includes(new URL(config.url).origin));
  return value;
}
function emit(value) { process.stdout.write(JSON.stringify(safe(value)) + '\n'); }
function fail() {
  if (fatal) return;
  fatal = true;
  // Fixed public failure only; raw protocol errors/URLs never escape memory.
  process.stdout.write(JSON.stringify({phase, ok: false, reason: 'real-browser-failed',
    checkpoint, viewport: observedViewport}) + '\n');
  process.exitCode = 1;
  // The outer named Job is the only forced tree cleanup owner.
  setTimeout(() => process.exit(1), 1500);
}

class CdpPipeEOF extends Error {
  constructor() { super('cdp-eof'); }
}

class CDP {
  constructor(child, onFailure = fail) {
    this.child = child; this.pending = new Map(); this.next = 0; this.buffer = Buffer.alloc(0);
    this.events = new Set(); this.failure = null; this.closing = false; this.onFailure = onFailure;
    child.stdio[4].on('data', chunk => {
      try {
        need(chunk.length <= FRAME && this.buffer.length + chunk.length <= FRAME);
        this.buffer = Buffer.concat([this.buffer, chunk]);
        let end;
        while ((end = this.buffer.indexOf(0)) >= 0) {
          const raw = this.buffer.subarray(0, end);
          this.buffer = this.buffer.subarray(end + 1);
          need(raw.length > 0 && raw.length <= FRAME);
          const message = JSON.parse(decoder.decode(raw));
          need(message && typeof message === 'object' && !Array.isArray(message));
          if (message.id !== undefined) {
            const pending = this.pending.get(message.id);
            need(pending);
            this.pending.delete(message.id);
            clearTimeout(pending.timer);
            if (message.error) pending.reject(Error('unsupported-or-refused-cdp'));
            else pending.resolve(message.result);
          } else {
            need(++eventCount <= 20000 && typeof message.method === 'string');
            need(this.events.size < 128);
            const work = this.event(message);
            this.events.add(work);
            work.then(() => this.events.delete(work), error => {
              this.events.delete(work); this.reportFailure(error);
            });
          }
        }
      } catch (error) { this.reportFailure(error); }
    });
    child.stdio[4].on('end', () => {
      pipeEOF = true;
      if (this.buffer.length || !this.closing) this.reportFailure(Error('unexpected-cdp-eof'));
      for (const entry of this.pending.values()) { clearTimeout(entry.timer); entry.reject(new CdpPipeEOF()); }
      this.pending.clear();
    });
    for (const stream of [child.stdio[3], child.stdio[4]]) stream.on('error', error => {
      if (!this.closing) this.reportFailure(error);
    });
  }
  reportFailure(error) {
    this.failure ||= error;
    this.onFailure(error);
  }
  send(method, params = {}, sessionId) {
    need(!fatal && this.pending.size < 32);
    const id = ++this.next;
    const data = Buffer.from(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}) + '\0');
    need(data.length <= FRAME);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(Error('cdp-deadline')); }, 6000);
      this.pending.set(id, {resolve, reject, timer});
      this.child.stdio[3].write(data, error => {
        if (error && this.pending.has(id)) {
          this.pending.delete(id); clearTimeout(timer); reject(error);
        }
      });
    });
  }
  async close() {
    this.closing = true;
    // New teardown events do no work. Previously admitted handlers must finish
    // before the close command can invalidate their outstanding CDP requests.
    await deadline(Promise.allSettled([...this.events]), 6000);
    if (this.failure) throw this.failure;
    need(this.pending.size === 0);
    try { await this.send('Browser.close'); }
    catch (error) { if (!(error instanceof CdpPipeEOF)) throw error; }
    if (this.failure) throw this.failure;
  }
  async event(message) {
    if (this.closing) return;
    const p = pages.find(page => page.session === message.sessionId);
    const e = message.params || {};
    if (message.method === 'Runtime.exceptionThrown') { scriptErrors++; throw Error('script-error'); }
    if (message.method === 'Runtime.consoleAPICalled' && e.type === 'error') { consoleErrors++; throw Error('console-error'); }
    if (!p) return;
    if (message.method === 'Fetch.requestPaused') {
      const url = new URL(e.request.url);
      const allowed = url.origin === new URL(config.url).origin && !url.username && !url.password
        && !url.search && ['/', '/api/state', '/api/manage', '/favicon.ico'].includes(url.pathname);
      if (!allowed) {
        await this.send('Fetch.failRequest', {requestId: e.requestId, errorReason: 'BlockedByClient'}, p.session);
        throw Error('unselected-network-request');
      }
      // Admission only: forward the original request unchanged. Never fulfill
      // a route, replace a response, or synthesize application success.
      await this.send('Fetch.continueRequest', {requestId: e.requestId}, p.session);
    }
    if (message.method === 'Network.requestWillBeSent') {
      need(p.requests.size < 512);
      const url = new URL(e.request.url);
      need(url.origin === new URL(config.url).origin && !url.username && !url.password
        && !url.search && ['/', '/api/state', '/api/manage', '/favicon.ico'].includes(url.pathname));
      let kind = null;
      if (url.pathname === '/api/manage') {
        need(e.request.method === 'POST' && typeof e.request.postData === 'string'
          && Buffer.byteLength(e.request.postData) <= 128000);
        const action = JSON.parse(e.request.postData).action;
        need(action.kind === 'document' && ['read', 'plan', 'pending', 'apply'].includes(action.request.kind)
          && action.request.documentId === config.document);
        kind = action.request.kind;
      }
      p.requests.set(e.requestId, {kind, bootstrap: url.pathname === '/'});
    }
    if (message.method === 'Network.responseReceived') {
      const request = p.requests.get(e.requestId);
      if (request) {
        request.status = e.response.status;
        if (request.kind || request.bootstrap) need(request.status === 200);
      }
    }
    if (message.method === 'Network.loadingFinished') {
      const request = p.requests.get(e.requestId);
      if (!request) return;
      p.requests.delete(e.requestId);
      if (!request.kind && !request.bootstrap) return;
      need(e.encodedDataLength <= JSON_LIMIT);
      const response = await this.send('Network.getResponseBody', {requestId: e.requestId}, p.session);
      need(typeof response.body === 'string' && Buffer.byteLength(response.body) <= FRAME);
      const bytes = response.base64Encoded ? Buffer.from(response.body, 'base64') : Buffer.from(response.body);
      need(bytes.length <= JSON_LIMIT);
      if (request.bootstrap) {
        need(hash(bytes) === config.dashboardSha256);
        p.bootstrapSha256 = hash(bytes);
      } else {
        const reply = JSON.parse(decoder.decode(bytes));
        need(reply.schema === 'native_dashboard_management_v1' && reply.outcome === 'completed');
        need(p.receipts.length < 64);
        // Retain only response data in memory. Selected fields are emitted below.
        p.receipts.push({kind: request.kind, data: reply.data});
      }
    }
    if (message.method === 'Network.loadingFailed') {
      const request = p.requests.get(e.requestId);
      p.requests.delete(e.requestId);
      if (request && (request.kind || request.bootstrap)) throw Error('real-request-failed');
    }
  }
}

async function evaluate(p, expression) {
  const result = await cdp.send('Runtime.evaluate', {expression, returnByValue: true, awaitPromise: false}, p.session);
  need(!result.exceptionDetails && result.result && !result.result.unserializableValue);
  need(Buffer.byteLength(JSON.stringify(result.result)) <= 128000);
  return result.result.value;
}
async function until(p, expression) {
  const end = Date.now() + 6000;
  for (let tries = 0; tries < 60 && Date.now() < end; tries++) {
    need(!fatal);
    if (await evaluate(p, expression)) return;
    await delay(75);
  }
  throw Error('dom-deadline');
}
async function receipt(p, kind, since) {
  const end = Date.now() + 6000;
  while (Date.now() < end) {
    need(!fatal);
    const found = p.receipts.slice(since).find(item => item.kind === kind);
    if (found) return found.data;
    await delay(30);
  }
  throw Error('real-network-receipt-deadline');
}
async function click(p, selector, focused) {
  const rect = await evaluate(p, `(()=>{const e=document.querySelector(${JSON.stringify(selector)});
    if(!e||e.disabled||!e.getClientRects().length)return null;e.scrollIntoView({block:'center'});
    const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
  need(rect && Number.isFinite(rect.x) && Number.isFinite(rect.y));
  await cdp.send('Input.dispatchMouseEvent', {type: 'mousePressed', button: 'left', clickCount: 1, ...rect}, p.session);
  await cdp.send('Input.dispatchMouseEvent', {type: 'mouseReleased', button: 'left', clickCount: 1, ...rect}, p.session);
  if (focused) await until(p, `document.activeElement?.id===${JSON.stringify(focused)}`);
}
async function focusPendingControlWithKeyboard(p) {
  // Disabling the initiating button during a real request can move focus to
  // the body after click() has already observed it. Return through actual Tab
  // input after the reply; never assign DOM focus or alter disabled controls.
  await until(p, `!document.getElementById('pending-documents').disabled`);
  for (let attempt = 0; attempt < 12; attempt++) {
    if (await evaluate(p, `document.activeElement?.id==='pending-documents'`)) return;
    await cdp.send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9}, p.session);
    await cdp.send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9}, p.session);
  }
  if (await evaluate(p, `document.activeElement?.id==='pending-documents'`)) return;
  throw Error('keyboard-focus-deadline');
}
async function confirm(p, prefix) {
  await until(p, `document.getElementById('confirm-dialog').open`);
  need(await evaluate(p, `document.getElementById('confirm-message').textContent.startsWith(${JSON.stringify(prefix)})`));
  need(await evaluate(p, `document.activeElement?.value==='cancel'`));
  await click(p, '#confirm-dialog button[value="confirm"]');
  await until(p, `!document.getElementById('confirm-dialog').open`);
}
async function load(p, discard) {
  const since = p.receipts.length;
  await click(p, '#load-document');
  if (discard) await confirm(p, 'Discard this file draft');
  const data = await receipt(p, 'read', since);
  await until(p, `!document.getElementById('document-editor').hidden && !document.getElementById('load-document').disabled`);
  need(data.document.documentId === config.document && Buffer.byteLength(data.document.content) <= 64000);
  return data.document;
}
async function draftAndReview(p, draft) {
  await click(p, '#document-content', 'document-content');
  await cdp.send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2}, p.session);
  await cdp.send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2}, p.session);
  await cdp.send('Input.insertText', {text: draft}, p.session);
  await until(p, `document.getElementById('document-content').value===${JSON.stringify(draft.replace(/\r\n/g, '\n'))}`);
  const since = p.receipts.length;
  await click(p, '#plan-document');
  const result = await receipt(p, 'plan', since);
  need(result.plan.content === draft && result.plan.afterHash === hash(Buffer.from(draft))
    && /^[a-f0-9]{64}$/.test(result.plan.id) && /^[a-f0-9]{64}$/.test(result.plan.ownerGeneration));
  await until(p, `!document.getElementById('document-review').hidden && !document.getElementById('apply-document').disabled`);
  p.draft = draft; p.planId = result.plan.id;
  return result.plan;
}

async function snapshot(p, state) {
  need(['before', 'pending', 'after'].includes(state));
  const observation = await evaluate(p, `(()=>{
    const ids=['document-choice','load-document','pending-documents','document-content','plan-document',
      'apply-document','document-review','document-state','document-journal-list','management-status'];
    return {width:innerWidth,scrollWidth:document.documentElement.scrollWidth,focus:document.activeElement?.id||'',
      recoveryButtons:document.querySelectorAll('#document-journal-list button').length,
      nodes:ids.map(id=>{const e=document.getElementById(id),r=e.getBoundingClientRect();
        return {id,visible:!!e.getClientRects().length,disabled:e.disabled===true,readOnly:e.readOnly===true,
          selected:id==='document-choice'?e.value:undefined,text:id==='document-content'?e.value:
            ['document-state','document-journal-list','management-status'].includes(id)?e.textContent:undefined,
          x:r.x,y:r.y,width:r.width,height:r.height};})};})()`);
  need(observation.width === p.width && observation.scrollWidth <= p.width);
  for (const id of ['document-choice', 'load-document', 'pending-documents', 'document-content',
    'plan-document', 'apply-document']) need(observation.nodes.find(node => node.id === id).visible);
  for (const node of observation.nodes) need(!node.text || Buffer.byteLength(node.text) <= 8000);
  const draftNode = observation.nodes.find(n => n.id === 'document-content');
  need(draftNode.text === p.draft.replace(/\r\n/g, '\n'));
  if (state === 'pending') {
    need(draftNode.readOnly && observation.nodes.find(n => n.id === 'plan-document').disabled
      && observation.nodes.find(n => n.id === 'apply-document').disabled && observation.recoveryButtons === 0);
    const blocker = observation.nodes.find(n => n.id === 'document-journal-list');
    need(blocker.visible && blocker.text.includes('Host journal archival blocks this file.')
      && blocker.text.includes('Preserve your draft') && blocker.text.includes('This page cannot retire journals'));
    need(observation.focus === 'pending-documents');
  }
  // Selected real DOM observations only. Never serialize location, storage,
  // script source, headers, a full DOM, or arbitrary console/error strings.
  draftNode.textSha256 = hash(Buffer.from(draftNode.text));
  delete draftNode.text;
  safe(observation);
  const layout = await cdp.send('Page.getLayoutMetrics', {}, p.session);
  const content = layout.cssContentSize;
  need(content && content.width <= p.width + 1 && content.height > 0 && content.height <= 4096
    && Math.ceil(content.height) * p.width <= 4_000_000);
  const shot = await cdp.send('Page.captureScreenshot', {format: 'png', fromSurface: true, captureBeyondViewport: true,
    clip: {x: 0, y: 0, width: p.width, height: Math.ceil(content.height), scale: 1}}, p.session);
  need(typeof shot.data === 'string' && shot.data.length <= Math.ceil(PNG_LIMIT / 3) * 4);
  const png = Buffer.from(shot.data, 'base64');
  need(png.toString('base64') === shot.data && png.length >= 33 && png.length <= PNG_LIMIT
    && (pngTotal += png.length) <= 12_000_000
    && png.subarray(0, 8).equals(Buffer.from([137,80,78,71,13,10,26,10]))
    && png.toString('ascii', 12, 16) === 'IHDR' && png.readUInt32BE(16) === p.width
    && png.readUInt32BE(20) === Math.ceil(content.height));
  const name = p.name + '-' + state + '.png';
  fs.writeFileSync(path.join(config.output, name), png, {flag: 'wx'});
  const domBytes = Buffer.from(JSON.stringify(observation, null, 2) + '\n');
  fs.writeFileSync(path.join(config.output, p.name + '-' + state + '.dom.json'), domBytes, {flag: 'wx'});
  return {screenshot: name, screenshotSha256: hash(png), screenshotBytes: png.length,
    dom: p.name + '-' + state + '.dom.json', domSha256: hash(domBytes),
    width: p.width, height: Math.ceil(content.height), noHorizontalOverflow: true,
    actualScreenshotCapture: true, humanVisualReviewComplete: false};
}

function admitStart(value) {
  need(process.platform === 'win32' && process.env.GITHUB_ACTIONS === 'true'
    && /^[0-9a-f]{64}$/.test(process.env.YOKE_BROWSER_WORKER_NONCE || ''));
  need(Object.keys(value).sort().join(',') === 'browser,dashboardSha256,document,draft,output,phase,profile,url,variant');
  const outputs = {native: 'ci-host-document-history-browser', serena: 'ci-host-document-history-browser-serena',
    'persistent-python': 'ci-host-document-history-browser-persistent-python', http: 'ci-host-document-history-browser-http'};
  need(typeof value.variant === 'string' && Object.hasOwn(outputs, value.variant)
    && value.variant === process.env.YOKE_BROWSER_WORKER_VARIANT && value.phase === 'start');
  const url = new URL(value.url);
  need(url.protocol === 'http:' && url.hostname === '127.0.0.1' && /^[0-9]+$/.test(url.port)
    && url.pathname === '/' && !url.search && !url.username && !url.password && /^#[a-f0-9]{64}$/.test(url.hash));
  const candidates = ['C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe'];
  need(candidates.some(p => p.toLowerCase() === value.browser.toLowerCase())
    && /^[a-f0-9]{64}$/.test(value.dashboardSha256) && value.document === 'new'
    && value.profile === path.join(process.cwd(), 'edge-profile')
    && path.isAbsolute(value.output) && value.output === path.join(__dirname, '..', 'validation', outputs[value.variant])
    && !fs.existsSync(value.profile));
  return url;
}

async function start(value) {
  const url = admitStart(value);
  config = value;
  fs.mkdirSync(config.profile);
  browser = spawn(config.browser, ['--headless=new', '--remote-debugging-pipe', '--no-first-run',
    '--no-default-browser-check', '--disable-background-networking', '--disable-component-update',
    '--disable-sync', '--disable-default-apps', '--metrics-recording-only',
    '--disk-cache-size=1048576', '--media-cache-size=1048576', '--disable-gpu-shader-disk-cache',
    '--proxy-server=' + url.origin, '--proxy-bypass-list=127.0.0.1',
    '--user-data-dir=' + config.profile, 'about:blank'],
    {cwd: process.cwd(), windowsHide: true, detached: false, stdio: ['ignore', 'pipe', 'pipe', 'pipe', 'pipe']});
  closeExit = new Promise((resolve, reject) => {
    browser.once('error', () => reject(Error('browser-launch')));
    browser.once('exit', (code, signal) => { if (!closing) fail(); resolve({code, signal}); });
  });
  closeExit.catch(fail);
  for (const stream of [browser.stdout, browser.stderr]) {
    stream.on('data', data => { browserLogs += data.length; if (browserLogs > 1_000_000) fail(); });
    stream.on('end', () => { browserOutputEOF++; });
    stream.on('error', fail);
  }
  cdp = new CDP(browser);
  const version = await cdp.send('Browser.getVersion');
  need(version.protocolVersion === '1.3' && /^(?:HeadlessChrome|Chrome|Edg)\/[0-9.]+$/.test(version.product));
  const reportedBrowser = version.userAgent.match(/(?:Edg|HeadlessChrome|Chrome)\/[0-9.]+/g);
  need(reportedBrowser && reportedBrowser.length <= 3);
  await cdp.send('Browser.setDownloadBehavior', {behavior: 'deny'});
  for (const [name, width, height] of [['desktop', 1440, 1000], ['mobile', 390, 844]]) {
    const created = await cdp.send('Target.createTarget', {url: 'about:blank'});
    const attached = await cdp.send('Target.attachToTarget', {targetId: created.targetId, flatten: true});
    const p = {name, width, height, session: attached.sessionId, requests: new Map(), receipts: []};
    pages.push(p);
    for (const method of ['Runtime.enable', 'Page.enable', 'Network.enable']) await cdp.send(method,
      method === 'Network.enable' ? {maxTotalBufferSize: 4_000_000, maxResourceBufferSize: 1_000_000,
        maxPostDataSize: 128000} : {}, p.session);
    await cdp.send('Network.setCacheDisabled', {cacheDisabled: true}, p.session);
    // Request admission only. Own traffic goes to the unmodified real server;
    // unsupported CDP is a hard failure, never an offline/mock fallback.
    await cdp.send('Fetch.enable', {patterns: [{urlPattern: '*', requestStage: 'Request'}]}, p.session);
    await cdp.send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: name === 'mobile'}, p.session);
    const navigation = await cdp.send('Page.navigate', {url: config.url}, p.session);
    need(!navigation.errorText);
    await until(p, `document.getElementById('connection')?.dataset.state==='live' && !document.getElementById('documents-tab').hidden`);
    await click(p, '#documents-tab', 'page-title');
    need(await evaluate(p, `document.getElementById('document-choice').value==='new'`));
  }
  return {variant: config.variant, engine: version.product, reportedBrowserVersions: reportedBrowser,
    protocolVersion: version.protocolVersion, nodeVersion: process.version,
    trustedBrowserPath: config.browser, headless: true, freshProfile: true, cdpTransport: 'inherited-pipes',
    visibleDesktopShellInteraction: false};
}

async function run(value) {
  need(!fatal && !finished && value && typeof value === 'object' && !Array.isArray(value));
  phase = value.phase;
  checkpoint = 'phase-entry'; observedViewport = 'none';
  if (phase === 'start') { need(!config); return start(value); }
  need(config && pages.length === 2);
  if (phase === 'before') {
    need(commands === 2);
    const states = [];
    for (const p of pages) {
      await load(p, false);
      await draftAndReview(p, config.draft);
      need(p.bootstrapSha256 === config.dashboardSha256);
      states.push(await snapshot(p, 'before'));
    }
    return {states, lastPlanId: pages[1].planId, servedDashboardSha256: config.dashboardSha256};
  }
  if (phase === 'pending') {
    need(commands === 3);
    const states = [];
    for (const p of pages) {
      observedViewport = p.name;
      checkpoint = 'pending-refresh';
      // Wait for an actual automatic state refresh, without reloading the page
      // or discarding the existing reviewed draft.
      const prior = await evaluate(p, `document.getElementById('updated').textContent`);
      await until(p, `document.getElementById('updated').textContent!==${JSON.stringify(prior)}`);
      const since = p.receipts.length;
      checkpoint = 'pending-click';
      await click(p, '#pending-documents');
      checkpoint = 'pending-receipt';
      const pending = await receipt(p, 'pending', since);
      checkpoint = 'pending-admission';
      need(pending.complete === false && pending.blockedByHistoryArchive === true
        && Array.isArray(pending.journals) && pending.journals.length === 0
        && pending.fileChanged === false && pending.metadataChanged === false && pending.sourceWritten === false);
      checkpoint = 'pending-disabled';
      await until(p, `document.getElementById('document-content').readOnly && document.getElementById('plan-document').disabled && document.getElementById('apply-document').disabled`);
      // manage() temporarily disables this clicked control, which can blur it
      // before the input acknowledgement. Verify focus through real keyboard
      // input after the reply, then exercise skipping the disabled actions.
      checkpoint = 'pending-keyboard-focus';
      await focusPendingControlWithKeyboard(p);
      // Keyboard navigation must skip disabled Review/Save controls.
      const count = p.receipts.length;
      checkpoint = 'pending-tab';
      await cdp.send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9}, p.session);
      await cdp.send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9}, p.session);
      need(!['plan-document', 'apply-document'].includes(await evaluate(p, `document.activeElement?.id`)));
      checkpoint = 'pending-repeat-click';
      await click(p, '#pending-documents');
      checkpoint = 'pending-repeat-receipt';
      await receipt(p, 'pending', count);
      checkpoint = 'pending-keyboard-focus';
      await focusPendingControlWithKeyboard(p);
      checkpoint = 'pending-repeat-admission';
      need(p.receipts.slice(since).length === 2 && p.receipts.slice(since).every(item => item.kind === 'pending'));
      checkpoint = 'pending-snapshot';
      states.push({...await snapshot(p, 'pending'), serverComplete: false, blockedByHistoryArchive: true,
        supportedRecoveryActions: 0, exactDraftRetained: true});
    }
    return {states, scriptErrors, consoleErrors};
  }
  if (phase === 'after') {
    need((commands === 4 && value.viewport === 'desktop') || (commands === 5 && value.viewport === 'mobile'));
    need(typeof value.draft === 'string' && Buffer.byteLength(value.draft) <= 64000);
    const p = pages.find(p => p.name === value.viewport);
    const since = p.receipts.length;
    await click(p, '#pending-documents');
    const pending = await receipt(p, 'pending', since);
    // The live global API omits complete when ordinary unblocked journals are
    // available; an absent field is never reinterpreted as proof of completeness.
    need(pending.blockedByHistoryArchive !== true && pending.complete !== false && Array.isArray(pending.journals));
    await load(p, true);
    const plan = await draftAndReview(p, value.draft);
    const prior = p.receipts.length;
    await click(p, '#apply-document');
    await confirm(p, 'Save reviewed plan ' + plan.id);
    await receipt(p, 'apply', prior);
    const read = await receipt(p, 'read', prior);
    need(read.document.content === value.draft && read.document.sha256 === hash(Buffer.from(value.draft)));
    await until(p, `document.getElementById('management-status').textContent.startsWith('Selected file saved.')`);
    need(!scriptErrors && !consoleErrors);
    return {planId: plan.id, ...await snapshot(p, 'after'), guardedSaveObserved: true,
      exactReadBackSha256: hash(Buffer.from(read.document.content)), scriptErrors, consoleErrors};
  }
  if (phase === 'close') {
    need(commands === 6 && !scriptErrors && !consoleErrors);
    closing = true;
    // Some Chromium versions close the pipe before acknowledging Browser.close.
    // A close request followed by raw exit 0 and pipe EOF is required either way.
    await cdp.close();
    const exit = await deadline(closeExit, 6000);
    need(exit.code === 0 && exit.signal === null);
    for (let n = 0; n < 50 && (!pipeEOF || browserOutputEOF !== 2); n++) await delay(20);
    need(pipeEOF && browserOutputEOF === 2);
    finished = true;
    return {browserCloseRequested: true, browserExitCode: exit.code, cdpEOF: pipeEOF, browserOutputEOF: true,
      scriptErrors, consoleErrors, screenshots: 6, visualReviewComplete: false};
  }
  throw Error('unknown-phase');
}

if (require.main === module) {
process.stdin.on('data', chunk => {
  try {
    need(chunk.length <= 64000 && input.length + chunk.length <= 64000);
    input = Buffer.concat([input, chunk]);
    let end;
    while ((end = input.indexOf(10)) >= 0) {
      const raw = input.subarray(0, end); input = input.subarray(end + 1);
      commandBytes += raw.length;
      need(++commands <= 6 && commandBytes <= 128000);
      const command = JSON.parse(decoder.decode(raw));
      serial = serial.then(async () => {
        const result = await deadline(run(command), 30000);
        emit({phase: command.phase, ok: true, ...result});
      }).catch(fail);
    }
  } catch { fail(); }
});
process.stdin.on('end', () => {
  serial.then(() => { if (!finished || input.length) fail(); }).catch(fail);
});
process.stdin.on('error', fail);
process.on('uncaughtException', fail);
process.on('unhandledRejection', fail);
} else {
  // Importing the protocol class starts no driver, browser or input listener.
  module.exports = {CDP, CdpPipeEOF, admitStart};
}
