import { randomUUID } from 'node:crypto';
import { existsSync, readFileSync, mkdirSync, unlinkSync, openSync, closeSync, writeFileSync, lstatSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { hostname } from 'node:os';
import { Workspace } from '../core/workspace.js';
import type { Snapshot, FileEdit, PlannedFile, ChangePlan, ApplyReceipt } from '../core/types.js';
import { assert, hash, stable, TextIndex, Deadline, atomicWrite, isTestPath, CIError } from '../core/util.js';
import { normalizePath, safePath } from '../core/paths.js';
interface Journal {
    id: string;
    planId: string;
    receipt: ApplyReceipt;
    files: PlannedFile[];
    state: 'applying' | 'done' | 'rolled_back' | 'recovery_required';
    applied: string[];
}
export class ChangeManager {
    constructor(readonly workspace: Workspace, readonly options: {
        allowWrites?: boolean;
        afterWrite?: (index: number) => void;
    } = {}) { }
    prepare(base: Snapshot, edits: FileEdit[], deadline: Deadline, ttlMs = 3600000): ChangePlan {
        assert(edits.length > 0 && edits.length <= 2000, 'INVALID_EDIT', 'A plan needs 1–2000 file edits');
        const groups = new Map<string, FileEdit[]>();
        for (const edit of edits) {
            const path = normalizePath(edit.path);
            safePath(this.workspace.root, path);
            const group = groups.get(path) ?? [];
            group.push({ ...edit, path });
            groups.set(path, group);
        }
        const files: PlannedFile[] = [];
        const updates: {
            path: string;
            content: Buffer | null;
            mode?: number;
        }[] = [];
        const diffs: string[] = [];
        for (const [path, group] of groups) {
            deadline.check();
            const original = base.files.find(f => f.path === path);
            let next: Buffer | null;
            if (group.some(e => e.kind === 'create')) {
                assert(group.length === 1 && group[0]!.kind === 'create' && !original, 'WRITE_CONFLICT', 'Creation target exists or has conflicting edits');
                next = this.encode(group[0]!.text);
            }
            else {
                assert(original, 'FILE_NOT_FOUND', `Edit target is absent: ${path}`);
                const bytes = this.workspace.store.blob(original.hash);
                for (const e of group)
                    assert(e.kind !== 'create' && e.expectedHash === original.hash, 'SNAPSHOT_STALE', `Expected source hash differs: ${path}`);
                if (group.some(e => e.kind === 'delete')) {
                    assert(group.length === 1, 'INVALID_EDIT', 'Delete cannot be combined with replacements');
                    next = null;
                }
                else {
                    const ti = new TextIndex(bytes.toString('utf8'));
                    const unique = [...new Map(group.map(e => [stable(e), e])).values()] as Extract<FileEdit, {
                        kind: 'replace';
                    }>[];
                    unique.sort((a, b) => a.startByte - b.startByte || a.endByte - b.endByte);
                    for (let i = 0; i < unique.length; i++) {
                        const e = unique[i]!;
                        assert(ti.isBoundary(e.startByte) && ti.isBoundary(e.endByte) && e.startByte <= e.endByte, 'INVALID_RANGE', 'Edit range is not on valid UTF-8 boundaries');
                        const previous = unique[i - 1];
                        assert(!previous || previous.endByte <= e.startByte && (previous.startByte !== e.startByte || previous.endByte !== previous.startByte), 'OVERLAPPING_EDITS', 'Overlapping or ambiguous edits are rejected');
                    }
                    next = Buffer.from(bytes);
                    for (const e of unique.reverse())
                        next = Buffer.concat([next.subarray(0, e.startByte), this.encode(e.text), next.subarray(e.endByte)]);
                }
            }
            assert(next === null || next.length <= this.workspace.config.maxFileBytes, 'CAPACITY_EXCEEDED', 'Edited file exceeds maximum size');
            const afterHash = next === null ? null : this.workspace.store.putBlob(next);
            if (afterHash === (original?.hash ?? null))
                continue;
            const mode = original?.mode ?? 0o644;
            files.push({ path, beforeHash: original?.hash ?? null, afterHash, mode });
            updates.push({ path, content: next, mode });
            diffs.push(unifiedDiff(path, original ? this.workspace.store.blob(original.hash).toString('utf8') : null, next?.toString('utf8') ?? null));
        }
        assert(files.length > 0, 'NO_CHANGES', 'Plan does not change source');
        const result = this.workspace.derive(base, updates);
        const body: Omit<ChangePlan, 'id'> = { baseSnapshotId: base.id, resultSnapshotId: result.id, workspaceId: base.workspaceId, createdAt: new Date().toISOString(), expiresAt: new Date(Date.now() + ttlMs).toISOString(), files, diff: diffs.join('\n'), warnings: [], testFilesChanged: files.filter(f => isTestPath(f.path)).map(f => f.path), validation: 'not_run' };
        if (body.testFilesChanged.length)
            body.warnings.push('Tests are modified; independent review and explicit test-change approval are required for workspace integration.');
        if (files.some(f => f.path.startsWith('.github/') || f.path === 'code-intelligence.json' || f.path === 'package.json'))
            body.warnings.push('Execution/configuration metadata changed. Review before enabling or running project commands.');
        const plan: ChangePlan = { ...body, id: hash(stable(body)) };
        this.workspace.store.db.prepare('INSERT OR IGNORE INTO plans VALUES(?,?)').run(plan.id, stable(plan));
        this.workspace.store.event('change_prepared', { id: plan.id, base: base.id, result: result.id, files: files.length });
        return plan;
    }
    private encode(text: string): Buffer { assert(typeof text === 'string' && Buffer.from(text).toString('utf8') === text, 'INVALID_EDIT', 'Edit text must be valid Unicode without lone surrogates'); return Buffer.from(text); }
    plan(id: string): ChangePlan {
        const row = this.workspace.store.db.prepare('SELECT json FROM plans WHERE id=?').get(id) as {
            json: string;
        } | undefined;
        assert(row, 'PLAN_NOT_FOUND', 'Change plan not found');
        const plan = JSON.parse(row.json) as ChangePlan;
        const { id: _id, ...body } = plan;
        assert(plan.id === id && hash(stable(body)) === id, 'PLAN_TAMPERED', 'Plan content does not match its immutable ID');
        assert(plan.workspaceId === this.workspace.store.workspaceId, 'SCOPE_VIOLATION', 'Plan belongs to another workspace');
        return plan;
    }
    approve(id: string, allowTestChanges = false, allowUnverified = false): void {
        const plan = this.plan(id);
        assert(!plan.testFilesChanged.length || allowTestChanges, 'APPROVAL_REQUIRED', 'Plan changes tests; explicit test-change approval is required');
        this.workspace.store.db.prepare('INSERT OR REPLACE INTO approvals VALUES(?,?,?)').run(id, hash(stable(plan)), allowTestChanges ? 1 : 0);
        this.workspace.store.setMeta(`approval:${id}`, stable({ allowUnverified }));
        this.workspace.store.event('change_approved', { id, allowTestChanges, allowUnverified });
    }
    async apply(id: string, key: string, mode: 'isolated' | 'workspace', deadline: Deadline): Promise<ApplyReceipt> {
        assert(/^[A-Za-z0-9_.-]{1,150}$/.test(key), 'INVALID_ARGUMENT', 'Idempotency key must use 1–150 safe identifier characters');
        const old = this.workspace.store.db.prepare('SELECT plan,json FROM receipts WHERE key=?').get(key) as {
            plan: string;
            json: string;
        } | undefined;
        if (old) {
            assert(old.plan === id, 'IDEMPOTENCY_CONFLICT', 'Idempotency key is bound to another plan');
            const receipt = JSON.parse(old.json) as ApplyReceipt;
            assert(receipt.mode === mode, 'IDEMPOTENCY_CONFLICT', 'Idempotency mode differs');
            return receipt;
        }
        const plan = this.plan(id);
        assert(Date.parse(plan.expiresAt) > Date.now(), 'PLAN_EXPIRED', 'Prepare a new plan; the previous plan expired');
        const result = this.workspace.store.snapshot(plan.resultSnapshotId);
        if (mode === 'isolated') {
            const sandbox = join(this.workspace.store.directory, 'changes', randomUUID(), 'tree');
            this.workspace.materialize(result, sandbox);
            const receipt: ApplyReceipt = { id: randomUUID(), planId: id, idempotencyKey: key, mode, status: 'applied', snapshotId: result.id, changedPaths: plan.files.map(f => f.path), appliedPaths: plan.files.map(f => f.path), sandboxPath: sandbox };
            this.saveReceipt(receipt);
            return receipt;
        }
        assert(this.options.allowWrites, 'WRITE_DISABLED', 'Workspace writes require host --allow-writes; isolated plans are available without it');
        const approval = this.workspace.store.db.prepare('SELECT digest,allow_test FROM approvals WHERE plan=?').get(id) as {
            digest: string;
            allow_test: number;
        } | undefined;
        assert(approval?.digest === hash(stable(plan)), 'APPROVAL_REQUIRED', 'Approve this exact plan through the trusted CLI before integration');
        assert(!plan.testFilesChanged.length || approval.allow_test === 1, 'APPROVAL_REQUIRED', 'Test changes were not approved');
        const flags = JSON.parse(this.workspace.store.meta(`approval:${id}`) ?? '{}');
        const verified = this.workspace.store.cache<{
            status: string;
            resultSnapshotId: string;
        }>(`verified:${id}`);
        assert(flags.allowUnverified || verified?.status === 'passed' && verified.resultSnapshotId === result.id, 'VERIFICATION_REQUIRED', 'Run ci_verify for this plan before workspace integration');
        const release = this.acquire();
        const receipt: ApplyReceipt = { id: randomUUID(), planId: id, idempotencyKey: key, mode, status: 'applied', snapshotId: plan.baseSnapshotId, changedPaths: plan.files.map(f => f.path), appliedPaths: [] };
        const journal: Journal = { id: receipt.id, planId: id, receipt, files: plan.files, state: 'applying', applied: [] };
        try {
            const base = this.workspace.store.snapshot(plan.baseSnapshotId);
            this.workspace.assertCurrent(base, deadline);
            this.saveJournal(journal);
            for (let i = 0; i < plan.files.length; i++) {
                deadline.check();
                const file = plan.files[i]!;
                assert(this.diskHash(file.path) === file.beforeHash, 'WRITE_CONFLICT', `Source changed before integration: ${file.path}`);
                this.write(file.path, file.afterHash, file.mode);
                journal.applied.push(file.path);
                receipt.appliedPaths.push(file.path);
                this.saveJournal(journal);
                this.options.afterWrite?.(i);
            }
            const current = this.workspace.capture(deadline);
            for (const f of plan.files)
                assert((current.files.find(x => x.path === f.path)?.hash ?? null) === f.afterHash, 'WRITE_CONFLICT', 'Post-write verification differs from the plan');
            receipt.snapshotId = current.id;
            journal.state = 'done';
            this.saveJournal(journal);
            this.saveReceipt(receipt);
            this.workspace.store.event('change_applied', { id, receipt: receipt.id, snapshotId: current.id });
            return receipt;
        }
        catch (error) {
            const conflicts = this.rollbackJournal(journal);
            receipt.status = conflicts.length ? 'recovery_required' : 'rolled_back';
            receipt.error = (error as Error).message;
            journal.state = conflicts.length ? 'recovery_required' : 'rolled_back';
            this.saveJournal(journal);
            this.saveReceipt(receipt);
            if (!journal.applied.length && error instanceof CIError)
                throw error;
            return receipt;
        }
        finally {
            release();
        }
    }
    private diskHash(path: string): string | null {
        const abs = safePath(this.workspace.root, path);
        if (!existsSync(abs))
            return null;
        assert(lstatSync(abs).isFile(), 'WRITE_CONFLICT', 'Edit target is not a regular file');
        return hash(readFileSync(abs));
    }
    private write(path: string, digest: string | null, mode: number): void {
        const abs = safePath(this.workspace.root, path);
        if (digest === null) {
            if (existsSync(abs))
                unlinkSync(abs);
        }
        else {
            mkdirSync(dirname(abs), { recursive: true });
            atomicWrite(abs, this.workspace.store.blob(digest), mode);
        }
    }
    private rollbackJournal(journal: Journal): string[] {
        const conflicts: string[] = [];
        for (const path of [...journal.applied].reverse()) {
            const f = journal.files.find(f => f.path === path)!;
            try {
                const current = this.diskHash(path);
                if (current === f.beforeHash)
                    continue;
                if (current !== f.afterHash) {
                    conflicts.push(path);
                    continue;
                }
                this.write(path, f.beforeHash, f.mode);
            }
            catch {
                conflicts.push(path);
            }
        }
        return conflicts;
    }
    recover(): {
        recovered: string[];
        conflicts: string[];
    } {
        this.removeDeadLock();
        const release = this.acquire();
        const recovered: string[] = [], conflicts: string[] = [];
        try {
            for (const row of this.workspace.store.db.prepare('SELECT json FROM journals').all() as {
                json: string;
            }[]) {
                const j = JSON.parse(row.json) as Journal;
                if (!['applying', 'recovery_required'].includes(j.state))
                    continue;
                // A crash can occur after replacement but before the journal checkpoint.
                for (const file of j.files)
                    if (!j.applied.includes(file.path) && this.diskHash(file.path) === file.afterHash && file.beforeHash !== file.afterHash)
                        j.applied.push(file.path);
                const failed = this.rollbackJournal(j);
                j.state = failed.length ? 'recovery_required' : 'rolled_back';
                j.receipt.status = j.state;
                j.receipt.error = failed.length ? 'Recovery preserved concurrent edits' : 'Recovered interrupted integration';
                this.saveJournal(j);
                this.saveReceipt(j.receipt);
                if (failed.length)
                    conflicts.push(...failed);
                else
                    recovered.push(j.id);
            }
            return { recovered, conflicts };
        }
        finally {
            release();
        }
    }
    private saveReceipt(receipt: ApplyReceipt): void { this.workspace.store.db.prepare('INSERT INTO receipts VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET json=excluded.json').run(receipt.idempotencyKey, receipt.planId, stable(receipt)); }
    private saveJournal(journal: Journal): void { this.workspace.store.db.prepare('INSERT OR REPLACE INTO journals VALUES(?,?)').run(journal.id, stable(journal)); }
    private acquire(): () => void {
        const file = join(this.workspace.store.directory, 'write.lock'), nonce = randomUUID();
        let fd: number;
        try {
            fd = openSync(file, 'wx', 0o600);
        }
        catch {
            throw new CIError('WRITE_LOCKED', 'Another writer or interrupted transaction holds the lease; inspect/recover before retrying');
        }
        const fencing = Number(this.workspace.store.meta('fencing') ?? 0) + 1;
        this.workspace.store.setMeta('fencing', String(fencing));
        writeFileSync(fd, stable({ nonce, pid: process.pid, host: hostname(), fencing }));
        closeSync(fd);
        return () => {
            try {
                const lock = JSON.parse(readFileSync(file, 'utf8'));
                if (lock.nonce === nonce)
                    unlinkSync(file);
            }
            catch { /* never remove an unrelated lease */ }
        };
    }
    private removeDeadLock(): void {
        const file = join(this.workspace.store.directory, 'write.lock');
        if (!existsSync(file))
            return;
        const lock = JSON.parse(readFileSync(file, 'utf8'));
        assert(lock.host === hostname(), 'WRITE_LOCKED', 'Lease belongs to another host; manual inspection required');
        let alive = true;
        try {
            process.kill(lock.pid, 0);
        }
        catch (e) {
            alive = (e as NodeJS.ErrnoException).code !== 'ESRCH';
        }
        assert(!alive, 'WRITE_LOCKED', 'Writer process is still alive');
        unlinkSync(file);
    }
}
/** Valid whole-file unified diff; intentionally not a minimal diff algorithm. */
export function unifiedDiff(path: string, before: string | null, after: string | null): string {
    const lines = (s: string | null) => s === null || s === '' ? [] : s.endsWith('\n') ? s.slice(0, -1).split('\n') : s.split('\n');
    const a = lines(before), b = lines(after);
    const output = [`diff --git a/${path} b/${path}`, `--- ${before === null ? '/dev/null' : `a/${path}`}`, `+++ ${after === null ? '/dev/null' : `b/${path}`}`, `@@ -${a.length ? 1 : 0},${a.length} +${b.length ? 1 : 0},${b.length} @@`];
    for (const line of a)
        output.push('-' + line);
    if (before && !before.endsWith('\n'))
        output.push('\\ No newline at end of file');
    for (const line of b)
        output.push('+' + line);
    if (after && !after.endsWith('\n'))
        output.push('\\ No newline at end of file');
    return output.join('\n') + '\n';
}
