import { createHash, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { mkdirSync, writeFileSync, renameSync, existsSync, unlinkSync, openSync, closeSync, fsyncSync } from 'node:fs';
import { dirname } from 'node:path';
export class CIError extends Error {
    constructor(public readonly code: string, message: string) { super(message); this.name = 'CIError'; }
}
export function assert(condition: unknown, code: string, message: string): asserts condition {
    if (!condition)
        throw new CIError(code, message);
}
export function hash(value: string | Uint8Array): string { return createHash('sha256').update(value).digest('hex'); }
export function stable(value: unknown): string {
    if (value === undefined)
        return 'null';
    if (Array.isArray(value))
        return '[' + value.map(stable).join(',') + ']';
    if (value && typeof value === 'object')
        return '{' + Object.entries(value).filter(([, v]) => v !== undefined).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([k, v]) => JSON.stringify(k) + ':' + stable(v)).join(',') + '}';
    return JSON.stringify(value);
}
export function object(value: unknown, name = 'input'): Record<string, unknown> {
    assert(value !== null && typeof value === 'object' && !Array.isArray(value), 'INVALID_ARGUMENT', `${name} must be an object`);
    return value as Record<string, unknown>;
}
export function text(value: unknown, name: string, max = 200000): string {
    assert(typeof value === 'string' && value.length <= max, 'INVALID_ARGUMENT', `${name} must be a string of at most ${max} characters`);
    return value;
}
export function integer(value: unknown, name: string, min: number, max: number, fallback?: number): number {
    if (value === undefined && fallback !== undefined)
        return fallback;
    assert(typeof value === 'number' && Number.isSafeInteger(value) && value >= min && value <= max, 'INVALID_ARGUMENT', `${name} must be an integer between ${min} and ${max}`);
    return value;
}
export function atomicWrite(file: string, value: string | Uint8Array, mode = 0o600): void {
    mkdirSync(dirname(file), { recursive: true, mode: 0o700 });
    const temp = `${file}.${randomBytes(8).toString('hex')}.tmp`;
    try {
        const fd = openSync(temp, 'wx', mode);
        try {
            writeFileSync(fd, value);
            fsyncSync(fd);
        }
        finally {
            closeSync(fd);
        }
        renameSync(temp, file);
        // Persist the directory entry where the OS supports directory fsync.
        if (process.platform !== 'win32') {
            const parent = openSync(dirname(file), 'r');
            try {
                fsyncSync(parent);
            }
            finally {
                closeSync(parent);
            }
        }
    }
    finally {
        if (existsSync(temp))
            unlinkSync(temp);
    }
}
export class Deadline {
    readonly started = performance.now();
    readonly until: number;
    constructor(timeoutMs = 10000, readonly signal?: AbortSignal) { this.until = this.started + timeoutMs; }
    check(): void { assert(!this.signal?.aborted, 'CANCELLED', 'Request cancelled'); assert(performance.now() < this.until, 'DEADLINE_EXCEEDED', 'Shared request deadline exceeded'); }
    get remaining(): number { this.check(); return Math.max(1, Math.floor(this.until - performance.now())); }
    get elapsed(): number { return Math.round(performance.now() - this.started); }
}
export class TextIndex {
    readonly bytes: Buffer;
    readonly byteOffsets: Uint32Array;
    readonly lineStarts: number[] = [0];
    constructor(readonly value: string) {
        this.bytes = Buffer.from(value);
        this.byteOffsets = new Uint32Array(value.length + 1);
        let b = 0;
        for (let i = 0; i < value.length;) {
            const cp = value.codePointAt(i)!;
            const width = cp > 0xffff ? 2 : 1;
            this.byteOffsets[i] = b;
            if (width === 2)
                this.byteOffsets[i + 1] = b; // Mid-surrogate positions are not edit boundaries.
            b += Buffer.byteLength(String.fromCodePoint(cp));
            if (value[i] === '\n' || value[i] === '\r' && value[i + 1] !== '\n')
                this.lineStarts.push(i + 1);
            i += width;
            this.byteOffsets[i] = b;
        }
    }
    byte(offset: number): number { return this.byteOffsets[Math.max(0, Math.min(this.value.length, offset))]!; }
    utf16(byte: number): number {
        assert(this.isBoundary(byte), 'INVALID_RANGE', 'Offset is not a UTF-8 code-point boundary');
        let low = 0, high = this.byteOffsets.length - 1;
        while (low < high) {
            const mid = Math.floor((low + high) / 2);
            if (this.byteOffsets[mid]! < byte)
                low = mid + 1;
            else
                high = mid;
        }
        return low;
    }
    isBoundary(byte: number): boolean { return Number.isInteger(byte) && byte >= 0 && byte <= this.bytes.length && (byte === this.bytes.length || (this.bytes[byte]! & 0xc0) !== 0x80); }
    line(offset: number): number {
        let low = 0, high = this.lineStarts.length;
        while (low + 1 < high) {
            const mid = (low + high) >> 1;
            if (this.lineStarts[mid]! <= offset)
                low = mid;
            else
                high = mid;
        }
        return low + 1;
    }
    range(start: number, end: number) { return { startByte: this.byte(start), endByte: this.byte(end), startLine: this.line(start), endLine: this.line(end) }; }
}
export class CursorCodec {
    constructor(private readonly secret: Buffer) { }
    encode(value: Record<string, unknown>): string { const data = Buffer.from(stable(value)).toString('base64url'); return data + '.' + createHmac('sha256', this.secret).update(data).digest('base64url'); }
    decode(value: string): Record<string, unknown> {
        assert(value.length <= 4096, 'INVALID_CURSOR', 'Cursor too large');
        const [data, signature, extra] = value.split('.');
        assert(data && signature && !extra, 'INVALID_CURSOR', 'Malformed cursor');
        const expected = createHmac('sha256', this.secret).update(data).digest();
        const actual = Buffer.from(signature, 'base64url');
        assert(actual.length === expected.length && timingSafeEqual(actual, expected), 'INVALID_CURSOR', 'Cursor signature mismatch');
        try {
            return object(JSON.parse(Buffer.from(data, 'base64url').toString('utf8')), 'cursor');
        }
        catch {
            throw new CIError('INVALID_CURSOR', 'Invalid cursor payload');
        }
    }
}
export function unique<T>(values: T[]): T[] { return [...new Set(values)]; }
export function isTestPath(path: string): boolean { return /(^|\/)(test|tests|__tests__|spec)(\/|$)|(?:\.test|\.spec|_test)\.|(?:^|\/)test_[^/]+\.py$/.test(path); }
export function tokens(query: string): string[] { return unique(query.replace(/([a-z])([A-Z])/g, '$1 $2').toLocaleLowerCase('en').match(/[\p{L}\p{N}_]+/gu) ?? []).slice(0, 32); }
export function redact(value: string): string { return value.replace(/\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,})\b/g, '[REDACTED]'); }
