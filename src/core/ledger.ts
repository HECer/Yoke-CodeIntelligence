import { randomUUID } from 'node:crypto';
import { Store } from '../store/store.js';
import { assert, stable } from './util.js';
export interface Usage {
    inputTokens: number;
    outputTokens: number;
    cachedInputTokens: number;
}
/** Reservations survive unknown billing and process failure; unknown is never $0. */
export class Ledger {
    constructor(readonly store: Store) { }
    reserve(task: string, maxCost: number, taskLimit: number, provider: string): string {
        assert(task.length > 0 && task.length < 200 && Number.isFinite(maxCost) && maxCost >= 0 && Number.isFinite(taskLimit) && taskLimit >= 0, 'INVALID_BUDGET', 'Invalid task cost reservation');
        return this.store.transaction(() => {
            const committed = this.total(task);
            assert(committed.encumbered + maxCost <= taskLimit + 1e-12, 'COST_LIMIT', 'Task cost budget exhausted');
            const id = randomUUID();
            this.store.db.prepare('INSERT INTO attempts VALUES(?,?,?,?,?,?)').run(id, task, 'reserved', maxCost, null, stable({ provider, usage: null, events: [], createdAt: new Date().toISOString() }));
            return id;
        });
    }
    markSent(id: string): void {
        const row = this.row(id);
        assert(row.state === 'reserved' || row.state === 'sent', 'INVALID_SETTLEMENT', 'Cannot send a completed attempt');
        this.store.db.prepare("UPDATE attempts SET state='sent' WHERE id=?").run(id);
    }
    usage(id: string, eventId: string, usage: Usage): void {
        for (const v of Object.values(usage))
            assert(Number.isSafeInteger(v) && v >= 0, 'INVALID_USAGE', 'Usage must be nonnegative integers');
        assert(usage.cachedInputTokens <= usage.inputTokens, 'INVALID_USAGE', 'Cached input cannot exceed total input');
        this.store.transaction(() => {
            const row = this.row(id);
            assert(row.state !== 'not_sent', 'INVALID_USAGE', 'A cancelled unsent attempt cannot receive usage');
            const data = JSON.parse(row.json);
            if (data.events.includes(eventId))
                return;
            const old: Usage = data.usage ?? { inputTokens: 0, outputTokens: 0, cachedInputTokens: 0 };
            data.usage = { inputTokens: Math.max(old.inputTokens, usage.inputTokens), outputTokens: Math.max(old.outputTokens, usage.outputTokens), cachedInputTokens: Math.max(old.cachedInputTokens, usage.cachedInputTokens) };
            data.events.push(eventId);
            this.store.db.prepare('UPDATE attempts SET json=? WHERE id=?').run(stable(data), id);
        });
    }
    settle(id: string, actualCost: number | null): void {
        assert(actualCost === null || Number.isFinite(actualCost) && actualCost >= 0, 'INVALID_USAGE', 'Invalid settlement cost');
        const row = this.row(id);
        if (row.state === 'settled') {
            assert(row.actual === actualCost, 'IDEMPOTENCY_CONFLICT', 'Attempt already settled with different cost');
            return;
        }
        this.store.db.prepare('UPDATE attempts SET state=?,actual=? WHERE id=?').run(actualCost === null ? 'unknown' : 'settled', actualCost, id);
    }
    cancelBeforeSend(id: string): void { const row = this.row(id); assert(row.state === 'reserved' && JSON.parse(row.json).usage === null, 'INVALID_SETTLEMENT', 'Only a never-sent reservation can be cancelled'); this.store.db.prepare("UPDATE attempts SET state='not_sent',actual=0 WHERE id=?").run(id); }
    total(task: string): {
        actual: number;
        reserved: number;
        unknown: number;
        encumbered: number;
        attempts: number;
    } {
        const rows = this.store.db.prepare('SELECT state,reserved,actual FROM attempts WHERE task=?').all(task) as {
            state: string;
            reserved: number;
            actual: number | null;
        }[];
        let actual = 0, reserved = 0, unknown = 0;
        for (const row of rows) {
            if (row.actual !== null)
                actual += row.actual;
            else {
                reserved += row.reserved;
                if (row.state === 'unknown')
                    unknown++;
            }
        }
        return { actual, reserved, unknown, encumbered: actual + reserved, attempts: rows.length };
    }
    private row(id: string): {
        state: string;
        actual: number | null;
        reserved: number;
        json: string;
    } { const row = this.store.db.prepare('SELECT state,actual,reserved,json FROM attempts WHERE id=?').get(id) as any; assert(row, 'ATTEMPT_NOT_FOUND', 'Unknown provider attempt'); return row; }
}
