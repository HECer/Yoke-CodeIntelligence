/** Optional fail-closed Docker runner. No transparent fallback to an unsandboxed process. */
import { randomUUID } from 'node:crypto';
import { runProcess, type ProcessResult } from './process.js';
import { Deadline, assert, CIError } from './util.js';
export interface DockerPolicy {
    kind: 'docker';
    image: string;
    memoryMB?: number;
    pids?: number;
    cpus?: number;
}
export function dockerArguments(root: string, name: string, policy: DockerPolicy, command: string, args: string[]): string[] {
    assert(policy.kind === 'docker' && /^[a-zA-Z0-9][a-zA-Z0-9._:/-]*@sha256:[a-f0-9]{64}$/.test(policy.image), 'INVALID_SANDBOX', 'Use an explicit digest-pinned Docker image');
    const memory = policy.memoryMB ?? 1024, pids = policy.pids ?? 128, cpus = policy.cpus ?? 2;
    assert(Number.isInteger(memory) && memory >= 64 && memory <= 16384 && Number.isInteger(pids) && pids >= 8 && pids <= 512 && cpus > 0 && cpus <= 16, 'INVALID_SANDBOX', 'Invalid resource limits');
    assert(!root.includes(',') && !root.includes('\n'), 'INVALID_SANDBOX', 'Workspace path is not representable as a Docker bind mount');
    return ['run', '--name', name, '--pull=never', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges', '--pids-limit', String(pids), '--memory', `${memory}m`, '--cpus', String(cpus), '--tmpfs', '/tmp:rw,noexec,nosuid,size=128m', '--user', process.getuid ? `${process.getuid()}:${process.getgid!()}` : '65534:65534', '--mount', `type=bind,src=${root},dst=/workspace`, '--workdir', '/workspace', '--env', 'HOME=/tmp', '--entrypoint', command, policy.image, ...args];
}
export async function runSandboxed(command: string, args: string[], root: string, policy: DockerPolicy, deadline: Deadline): Promise<ProcessResult> {
    const name = `ci-${randomUUID()}`;
    let result: ProcessResult | undefined, failure: unknown;
    try {
        result = await runProcess('docker', dockerArguments(root, name, policy, command, args), { cwd: root, deadline, maxBytes: 1000000 });
    }
    catch (e) {
        failure = e;
    }
    // A container can outlive a killed Docker client. Cleanup has its own finite bound.
    try {
        const cleaned = await runProcess('docker', ['rm', '--force', name], { cwd: root, deadline: new Deadline(10000), maxBytes: 10000 });
        if (cleaned.exitCode !== 0 && !/No such container/i.test(cleaned.stderr))
            throw new Error('Container removal not confirmed');
    }
    catch (e) {
        throw new CIError('SANDBOX_CLEANUP_UNCONFIRMED', 'Docker unavailable or cleanup unconfirmed; verification must not pass');
    }
    if (failure)
        throw failure;
    assert(result, 'SANDBOX_FAILED', 'No sandbox result');
    return result;
}
