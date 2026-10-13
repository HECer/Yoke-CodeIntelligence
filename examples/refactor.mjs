/** Runnable, non-destructive rename demo. The original example files stay unchanged. */
import { fileURLToPath } from 'node:url';
import { CodeIntelligence } from '../dist/index.js';
const root = fileURLToPath(new URL('./demo/', import.meta.url));
const ci = new CodeIntelligence(root, {config: {timeoutMs: 120_000, maxResultBytes: 64_000}});
async function invoke(name, args) {
  const response = await ci.invoke(name, args);
  if (response.status === 'error') throw new Error(`${response.error.code}: ${response.error.message}`);
  return response;
}
try {
  const indexed = await invoke('ci_status', {mode: 'index'});
  const snapshotId = indexed.snapshotId;
  const found = await invoke('ci_find', {snapshotId, query: 'invoiceTotal', limit: 10});
  const entity = found.data.items.find(item => item.kind === 'function' && item.name === 'invoiceTotal');
  if (!entity) throw new Error('Demo function was not found. Restore the example source first.');
  const original = await invoke('ci_read', {snapshotId, entityId: entity.id});
  const prepared = await invoke('ci_change_prepare', {snapshotId, rename: {entityId: entity.id, newName: 'calculateInvoiceTotal'}});
  const verified = await invoke('ci_verify', {planId: prepared.data.id});
  if (verified.data.status !== 'passed') throw new Error('Demo verification did not pass.');
  const applied = await invoke('ci_change_apply', {planId: prepared.data.id, idempotencyKey: `demo-${prepared.data.id.slice(0, 20)}`, mode: 'isolated'});
  console.log(JSON.stringify({sourceRead: original.data.path, originalWorkspaceChanged: false,
    diff: prepared.data.diff, verification: verified.data.status, isolatedResult: applied.data}, null, 2));
} finally { ci.close(); }
