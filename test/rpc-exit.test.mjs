import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { fixture } from './helpers.mjs';
import { RpcPeer } from '../dist/adapters/rpc.js';
import { Deadline } from '../dist/core/util.js';

const script = fileURLToPath(new URL('./fixtures/adversarial-provider.mjs', import.meta.url));
async function running(t) {
    const f = fixture(t, { 'main.ts': 'nameText\n' });
    const peer = new RpcPeer('lsp');
    peer.start(process.execPath, [script, '{}'], f.root);
    f.cleanup(async () => {
        const child = peer.child;
        const stopped = child && child.exitCode === null && child.signalCode === null
            ? once(child, 'close') : Promise.resolve();
        peer.close();
        await stopped;
    });
    await peer.request('opened', null, new Deadline(30000));
    return peer;
}

test('RPC exit wait observes actual process closure and supports repeated waits', async t => {
    const peer = await running(t);
    // Observe the real OS child's event, independently of RPC's early "closed".
    let actualClosed = false;
    peer.child.once('close', () => { actualClosed = true; });
    peer.close();
    await peer.waitForExit(new Deadline(30000));
    assert(actualClosed, 'Exit wait returned before the provider process closed');
    await peer.waitForExit(new Deadline(30000));
});

test('RPC exit wait obeys an expired deadline without terminating a live peer', async t => {
    const peer = await running(t);
    await assert.rejects(() => peer.waitForExit(new Deadline(0)), error => error.code === 'DEADLINE_EXCEEDED');
    assert.deepEqual(await peer.request('opened', null, new Deadline(30000)), []);
});

test('RPC exit wait observes shared cancellation without terminating a live peer', async t => {
    const peer = await running(t);
    const controller = new AbortController();
    const waiting = peer.waitForExit(new Deadline(30000, controller.signal));
    controller.abort();
    await assert.rejects(waiting, error => error.code === 'CANCELLED');
    assert.deepEqual(await peer.request('opened', null, new Deadline(30000)), []);
});

test('RPC exit wait times out while the provider remains live', async t => {
    const peer = await running(t);
    await assert.rejects(() => peer.waitForExit(new Deadline(25)), error => error.code === 'DEADLINE_EXCEEDED');
    assert.deepEqual(await peer.request('opened', null, new Deadline(30000)), []);
});
