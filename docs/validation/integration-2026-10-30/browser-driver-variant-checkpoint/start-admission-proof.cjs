'use strict';
// Evaluate only admission guards with in-memory filesystem/environment doubles.
// Neither source slice can create a directory, process, browser or connection.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const cp = require('node:child_process'), assert = require('node:assert/strict');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '..', '..');
const normalize = text => text.replace(/\r\n/g, '\n');
const old = normalize(cp.execFileSync('git', ['show', 'd6b9da55:scripts/host_history_browser.cjs'], {cwd: root}).toString('utf8'));
const current = normalize(fs.readFileSync(path.join(root, 'scripts/host_history_browser.cjs'), 'utf8'));
const begin = old.indexOf('async function start(value) {');
const end = old.indexOf('  config = value;', begin);
assert.ok(begin >= 0 && end > begin);
const oldGuard = old.slice(begin, end) + '  return url;\n}\nthis.admit = start;';
assert.ok(!oldGuard.includes('spawn(') && !oldGuard.includes('mkdirSync('));
const freshBegin = current.indexOf('function admitStart(value) {');
const freshEnd = current.indexOf('async function start(value) {', freshBegin);
const freshGuard = current.slice(freshBegin, freshEnd) + '\nthis.admit = admitStart;';
assert.ok(!freshGuard.includes('spawn(') && !freshGuard.includes('mkdirSync('));
assert.equal(old.slice(0, begin), current.slice(0, freshBegin));
const oldTail = old.slice(end);
const freshTail = current.slice(current.indexOf('  config = value;', freshEnd))
  .replace('return {variant: config.variant, engine:', 'return {engine:')
  .replace('module.exports = {CDP, CdpPipeEOF, admitStart};', 'module.exports = {CDP, CdpPipeEOF};');
assert.equal(freshTail, oldTail);
function context(variant) {
  return vm.createContext({URL, path: path.win32, __dirname: 'C:\\owned-repository\\scripts',
    need: value => { if (!value) throw Error('admission-or-assertion'); }, fs: {existsSync: () => false},
    process: {platform: 'win32', cwd: () => 'C:\\owned-fixture', env: {GITHUB_ACTIONS: 'true',
      YOKE_BROWSER_WORKER_NONCE: 'a'.repeat(64), YOKE_BROWSER_WORKER_VARIANT: variant}}});
}
const names = {native: 'ci-host-document-history-browser', serena: 'ci-host-document-history-browser-serena',
  'persistent-python': 'ci-host-document-history-browser-persistent-python', http: 'ci-host-document-history-browser-http'};
(async () => {
  const observations = [];
  for (const [variant, name] of Object.entries(names)) {
    const value = {phase: 'start', browser: 'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
      profile: 'C:\\owned-fixture\\edge-profile', output: 'C:\\owned-repository\\validation\\' + name,
      url: 'http://127.0.0.1:1234/#' + 'b'.repeat(64), document: 'new', draft: '\ufeffπ 🦀 e\u0301\r\n',
      dashboardSha256: 'c'.repeat(64)};
    const before = context(variant), after = context(variant);
    vm.runInContext(oldGuard, before); vm.runInContext(freshGuard, after);
    let oldAccepted = false;
    try { await before.admit(value); oldAccepted = true; } catch (error) { assert.match(error.message, /admission-or-assertion/); }
    assert.equal(oldAccepted, variant === 'native');
    assert.equal(after.admit({...value, variant}).href, value.url);
    observations.push({variant, oldAccepted, correctedAccepted: true});
  }
  const receipt = {baseline: 'd6b9da55be23f3c7810292bc3412d67884b664cc', observations,
    noBrowserNativeWorkerOrNetworkExecuted: true,
    allDriverBehaviorAfterConfigurationUnchangedExceptVariantReceiptAndExport: true,
    driverSha256: crypto.createHash('sha256').update(fs.readFileSync(path.join(root, 'scripts/host_history_browser.cjs'))).digest('hex')};
  fs.writeFileSync(path.join(__dirname, 'start-admission-proof.json'), JSON.stringify(receipt, null, 2) + '\n', {flag: 'wx'});
  process.stdout.write(JSON.stringify(receipt) + '\n');
})().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
