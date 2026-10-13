import { hostname } from 'node:os';
import { readdirSync, lstatSync, unlinkSync, existsSync, renameSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { Store } from './store.js';
import { assert, integer, stable } from '../core/util.js';
import type { Snapshot } from '../core/types.js';
export interface RetentionPolicy {
    keepSnapshots?: number;
    minimumAgeMs?: number;
    cacheAgeMs?: number;
    apply?: boolean;
}
/** Local maintenance only. Exclusive connection check and an IMMEDIATE transaction cover mark and sweep. */
export function collectGarbage(store: Store, policy: RetentionPolicy = {}): Record<string, unknown> {
    const keep = integer(policy.keepSnapshots ?? 20, 'keepSnapshots', 1, 100000), age = integer(policy.minimumAgeMs ?? 86400000, 'minimumAgeMs', 0, 365 * 86400000), cacheAge = integer(policy.cacheAgeMs ?? 30 * 86400000, 'cacheAgeMs', 0, 365 * 86400000);
    return store.transaction(() => {
        const clients = store.db.prepare('SELECT id,pid,host FROM clients').all() as {
            id: string;
            pid: number;
            host: string;
        }[];
        for (const c of clients) {
            if (c.id === store.clientId)
                continue;
            let alive = true;
            if (c.host === hostname()) {
                try {
                    process.kill(c.pid, 0);
                }
                catch (e) {
                    if ((e as NodeJS.ErrnoException).code === 'ESRCH')
                        alive = false;
                }
            }
            assert(!alive, 'WORKSPACE_IN_USE', 'Storage maintenance requires closing all other workspace connections');
            if (policy.apply)
                store.db.prepare('DELETE FROM clients WHERE id=?').run(c.id);
        }
        const now = Date.now(), all = store.snapshots(100000), byId = new Map(all.map(s => [s.id, s]));
        const pinned = new Set(all.slice(0, keep).map(s => s.id));
        const current = store.meta('current');
        if (current)
            pinned.add(current);
        for (const row of store.db.prepare('SELECT snapshot FROM pins').all() as {
            snapshot: string;
        }[])
            pinned.add(row.snapshot);
        const blobs = new Set<string>();
        const visit = (value: unknown) => {
            if (typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)) {
                if (byId.has(value))
                    pinned.add(value);
                else
                    blobs.add(value);
            }
            else if (Array.isArray(value))
                for (const v of value)
                    visit(v);
            else if (value && typeof value === 'object')
                for (const v of Object.values(value))
                    visit(v);
        };
        // Unexpired plans, audit receipts and explicit memories are retained, not guessed away.
        const plans = store.db.prepare('SELECT id,json FROM plans').all() as {
            id: string;
            json: string;
        }[];
        const referencedPlans = new Set((store.db.prepare('SELECT plan FROM receipts').all() as {
            plan: string;
        }[]).map(r => r.plan));
        for (const r of store.db.prepare('SELECT json FROM journals').all() as {
            json: string;
        }[]) {
            const v = JSON.parse(r.json);
            referencedPlans.add(v.planId);
            visit(v);
        }
        const expiredPlans: string[] = [];
        for (const p of plans) {
            const v = JSON.parse(p.json);
            if (referencedPlans.has(p.id) || Date.parse(v.expiresAt) > now)
                visit(v);
            else
                expiredPlans.push(p.id);
        }
        for (const table of ['memories', 'receipts'])
            for (const r of store.db.prepare(`SELECT json FROM ${table}`).all() as {
                json: string;
            }[])
                visit(JSON.parse(r.json));
        const hasEditor = store.db.prepare("SELECT name FROM sqlite_master WHERE name='editor_sessions'").get();
        if (hasEditor)
            for (const r of store.db.prepare('SELECT json FROM editor_sessions').all() as {
                json: string;
            }[])
                visit(JSON.parse(r.json));
        for (const r of store.db.prepare('SELECT json FROM parse_cache WHERE created>=?').all(new Date(now - cacheAge).toISOString()) as {
            json: string;
        }[])
            visit(JSON.parse(r.json));
        // Imported/signed manifests and provenance artifacts must remain readable.
        for (const r of store.db.prepare('SELECT snapshot,json FROM artifacts').all() as {
            snapshot: string | null;
            json: string;
        }[]) {
            if (r.snapshot)
                pinned.add(r.snapshot);
            visit(JSON.parse(r.json));
        }
        let grew = true;
        while (grew) {
            grew = false;
            for (const id of [...pinned]) {
                const p = byId.get(id)?.parentId;
                if (p && !pinned.has(p)) {
                    pinned.add(p);
                    grew = true;
                }
            }
        }
        const retired = all.filter(s => !pinned.has(s.id) && now - Date.parse(s.createdAt) >= age);
        const retiredIds = new Set(retired.map(s => s.id));
        // A rollback must still be able to restore every pre-transaction snapshot.
        const beforeTransactionHashes = new Set(all.flatMap(s => s.files.map(f => f.hash)));
        for (const s of all)
            if (!retiredIds.has(s.id))
                for (const f of s.files)
                    blobs.add(f.hash);
        const candidates: {
            path: string;
            hash: string;
            bytes: number;
        }[] = [];
        const blobRoot = join(store.directory, 'blobs');
        assert(!lstatSync(blobRoot).isSymbolicLink(), 'PATH_DENIED', 'Blob root is not safe');
        for (const prefix of readdirSync(blobRoot, { withFileTypes: true })) {
            if (!prefix.isDirectory() || prefix.isSymbolicLink() || !/^[a-f0-9]{2}$/.test(prefix.name))
                continue;
            for (const file of readdirSync(join(blobRoot, prefix.name), { withFileTypes: true })) {
                const digest = prefix.name + file.name;
                if (!file.isFile() || file.isSymbolicLink() || !/^[a-f0-9]{64}$/.test(digest) || blobs.has(digest))
                    continue;
                const path = store.blobPath(digest), stat = lstatSync(path);
                if (now - stat.mtimeMs >= age)
                    candidates.push({ path, hash: digest, bytes: stat.size });
            }
        }
        if (policy.apply) {
            for (const id of expiredPlans) {
                store.db.prepare('DELETE FROM approvals WHERE plan=?').run(id);
                store.db.prepare('DELETE FROM plans WHERE id=?').run(id);
            }
            for (const s of retired) {
                for (const table of ['observations', 'evidence', 'vectors', 'entity_fts', 'entities', 'graphs'])
                    store.db.prepare(`DELETE FROM ${table} WHERE ${table === 'graphs' ? 'snapshot' : 'snapshot'}=?`).run(s.id);
                store.db.prepare('DELETE FROM snapshots WHERE id=?').run(s.id);
            }
            store.db.prepare('DELETE FROM parse_cache WHERE created<?').run(new Date(now - cacheAge).toISOString());
            // Quarantine first. A crash/rollback can be recovered by Store.blob without a missing reference.
            const trashRoot = join(store.directory, 'trash');
            assert(!lstatSync(trashRoot).isSymbolicLink(), 'PATH_DENIED', 'Trash root is not safe');
            for (const c of candidates)
                if (existsSync(c.path)) {
                    const dest = join(trashRoot, c.hash);
                    if (existsSync(dest))
                        assert(!lstatSync(dest).isSymbolicLink(), 'PATH_DENIED', 'Unsafe quarantine entry');
                    renameSync(c.path, dest);
                    utimesSync(dest, new Date(), new Date());
                }
            // Only an earlier run's unreferenced quarantine entries may be destroyed.
            for (const file of readdirSync(trashRoot, { withFileTypes: true }))
                if (file.isFile() && !file.isSymbolicLink() && /^[a-f0-9]{64}$/.test(file.name) && !blobs.has(file.name) && !beforeTransactionHashes.has(file.name)) {
                    const p = join(trashRoot, file.name);
                    if (now - lstatSync(p).mtimeMs >= Math.max(age, 86400000))
                        unlinkSync(p);
                }
        }
        const report = { applied: !!policy.apply, retiredSnapshots: retired.map(s => s.id), retainedSnapshots: all.length - retired.length, pinnedSnapshots: pinned.size, expiredPlans: expiredPlans.length, unreferencedBlobs: candidates.length, reclaimableBytes: candidates.reduce((n, c) => n + c.bytes, 0), scope: 'exclusive_local_store', artifactsRetained: true, blobDisposal: 'quarantine_then_later_unreferenced_purge' };
        if (policy.apply)
            store.event('maintenance', report);
        return report;
    });
}
export function pinSnapshot(store: Store, snapshotId: string, id: string, reason: string): void { store.snapshot(snapshotId); assert(id.length > 0 && id.length <= 200 && reason.length <= 1000, 'INVALID_ARGUMENT', 'Invalid snapshot pin'); store.db.prepare('INSERT INTO pins VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot,reason=excluded.reason').run(id, snapshotId, reason); }
export function unpinSnapshot(store: Store, id: string): void { store.db.prepare('DELETE FROM pins WHERE id=?').run(id); }
