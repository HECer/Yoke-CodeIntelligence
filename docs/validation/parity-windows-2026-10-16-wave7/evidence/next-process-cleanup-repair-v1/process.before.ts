import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { Deadline, CIError, assert } from './util.js';
export interface ProcessResult {
    exitCode: number | null;
    stdout: string;
    stderr: string;
    truncated: boolean;
    durationMs: number;
}
export function runProcess(command: string, args: string[], options: {
    cwd: string;
    input?: string;
    deadline: Deadline;
    maxBytes?: number;
    env?: Record<string, string>;
    trusted?: boolean;
}): Promise<ProcessResult> {
    const { deadline } = options;
    deadline.check();
    const start = Date.now();
    const max = options.maxBytes ?? 2000000;
    assert(command.length > 0 && !command.includes('\0') && args.every(a => typeof a === 'string' && !a.includes('\0')), 'INVALID_COMMAND', 'Invalid process command');
    return new Promise((resolve, reject) => {
        const child = spawn(command, args, { cwd: options.cwd, shell: false, detached: process.platform !== 'win32', env: { PATH: process.env.PATH ?? '', SystemRoot: process.env.SystemRoot ?? '', PYTHONIOENCODING: 'utf-8', PYTHONDONTWRITEBYTECODE: '1', PYTHONNOUSERSITE: '1', CI: '1', NO_COLOR: '1', ...options.env }, stdio: ['pipe', 'pipe', 'pipe'] });
        const outputs = { stdout: [] as Buffer[], stderr: [] as Buffer[] };
        let bytes = 0, truncated = false, done = false;
        let stdinClosed = false;
        let failure: Error | undefined;
        let timer: ReturnType<typeof setTimeout> | undefined;
        const stop = (error: Error) => {
            if (done || failure)
                return;
            failure = error;
            child.stdin.destroy();
            killTree(child);
        };
        const abort = () => stop(new CIError('CANCELLED', 'Process cancelled'));
        const cleanup = () => { clearTimeout(timer); deadline.signal?.removeEventListener('abort', abort); };
        for (const stream of ['stdout', 'stderr'] as const)
            child[stream].on('data', (data: Buffer) => {
                if (bytes >= max) {
                    truncated = true;
                    return;
                }
                const kept = data.subarray(0, max - bytes);
                outputs[stream].push(kept);
                bytes += kept.length;
                if (kept.length < data.length)
                    truncated = true;
            });
        child.on('error', error => {
            stop(new CIError('PROCESS_UNAVAILABLE', error.message));
        });
        child.on('close', code => {
            if (done)
                return;
            done = true;
            cleanup();
            if (failure) {
                if (stdinClosed)
                    reject(failure);
                else
                    child.stdin.once('close', () => reject(failure));
            }
            else
                resolve({ exitCode: code, stdout: Buffer.concat(outputs.stdout).toString('utf8'), stderr: Buffer.concat(outputs.stderr).toString('utf8'), truncated, durationMs: Date.now() - start });
        });
        child.stdin.on('error', () => { });
        child.stdin.once('close', () => { stdinClosed = true; });
        try {
            deadline.signal?.addEventListener('abort', abort, { once: true });
            timer = setTimeout(() => stop(new CIError('DEADLINE_EXCEEDED', 'Process exceeded shared deadline')), deadline.remaining);
            if (!failure)
                child.stdin.end(options.input ?? '');
        }
        catch (error) {
            stop(error instanceof Error ? error : new Error(String(error)));
        }
    });
}
export function killTree(child: Pick<ChildProcessWithoutNullStreams, 'pid' | 'kill'>): void {
    if (!child.pid)
        return;
    if (process.platform === 'win32') {
        const killer = spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
        killer.on('error', () => child.kill('SIGKILL'));
    }
    else {
        try {
            process.kill(-child.pid, 'SIGKILL');
        }
        catch {
            child.kill('SIGKILL');
        }
    }
}
