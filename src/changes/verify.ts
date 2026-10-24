import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { Workspace } from '../core/workspace.js';
import { TypeScriptProject } from '../analysis/typescript.js';
import { extractPython } from '../analysis/python.js';
import { extractGo } from '../analysis/go.js';
import { Deadline, assert, hash, stable, redact } from '../core/util.js';
import { runSandboxed } from '../core/sandbox.js';
import { runProcess } from '../core/process.js';
import { ChangeManager } from './manager.js';
import type { Snapshot, Diagnostic } from '../core/types.js';
import { buildTypeScript } from '../analysis/build.js';
export interface VerificationReport {
    id: string;
    status: 'passed' | 'failed' | 'incomplete';
    resultSnapshotId: string;
    baseSnapshotId?: string;
    planId?: string;
    static: {
        beforeErrors: number;
        afterErrors: number;
        newErrors: Diagnostic[];
        resolvedErrors: Diagnostic[];
        limitations: string[];
    };
    jobs: Record<string, unknown>[];
    inputFilesUnchanged: boolean;
    artifactId?: string;
}
export class Verifier {
    constructor(readonly workspace: Workspace, readonly trustedProject = false) { }
    private async diagnostics(snapshot: Snapshot, deadline: Deadline): Promise<{
        items: Diagnostic[];
        limitations: string[];
    }> {
        const items: Diagnostic[] = [];
        const limitations: string[] = [];
        if (snapshot.files.some(f => ['typescript', 'javascript'].includes(f.language))) {
            const ts = new TypeScriptProject(snapshot, this.workspace.contents(snapshot), this.workspace.root, deadline);
            try {
                items.push(...ts.diagnostics(deadline));
                limitations.push(...ts.limitations());
            }
            finally {
                ts.close();
            }
        }
        if (snapshot.files.some(f => f.language === 'python')) {
            try {
                const py = await extractPython(this.workspace, snapshot, deadline);
                items.push(...py.diagnostics);
                limitations.push(...py.limitations);
            }
            catch (e) {
                limitations.push('Python verifier unavailable: ' + (e as Error).message);
            }
        }
        if (snapshot.files.some(f => f.language === 'go')) {
            try {
                const go = await extractGo(this.workspace, snapshot, deadline);
                items.push(...go.diagnostics);
                limitations.push(...go.limitations);
            }
            catch (e) {
                limitations.push('Go verifier unavailable: ' + (e as Error).message);
            }
        }
        const unsupported = snapshot.files.filter(f => /\.(?:rs|java|cs|cpp|cc|c|h|hpp|rb|php|swift|kt|kts|scala|dart|vue|svelte|astro)$/i.test(f.path));
        if (unsupported.length)
            limitations.push(`Static verifier unavailable for ${unsupported.length} source files outside native TypeScript/JavaScript, Python and Go coverage.`);
        return { items, limitations };
    }
    async verify(snapshot: Snapshot, options: {
        planId?: string;
        profiles?: string[];
        build?: boolean;
    }, deadline: Deadline): Promise<VerificationReport> {
        let base: Snapshot | undefined, target = snapshot;
        if (options.planId) {
            const plan = new ChangeManager(this.workspace).plan(options.planId);
            base = this.workspace.store.snapshot(plan.baseSnapshotId);
            target = this.workspace.store.snapshot(plan.resultSnapshotId);
        }
        const previous = base ? await this.diagnostics(base, deadline) : { items: [], limitations: [] };
        const current = await this.diagnostics(target, deadline);
        const errors = (items: Diagnostic[]) => items.filter(d => d.severity === 'error');
        const before = errors(previous.items), after = errors(current.items);
        const delta = diagnosticDelta(before, after);
        const report: VerificationReport = { id: randomUUID(), status: delta.added.length ? 'failed' : 'passed', resultSnapshotId: target.id, baseSnapshotId: base?.id, planId: options.planId,
            static: { beforeErrors: before.length, afterErrors: after.length, newErrors: delta.added, resolvedErrors: delta.removed, limitations: [...new Set([...previous.limitations, ...current.limitations])] }, jobs: [], inputFilesUnchanged: true };
        if (report.status === 'passed' && current.limitations.some(l => l.includes('unavailable') || l.includes('disabled')))
            report.status = 'incomplete';
        if (options.build) {
            const build = buildTypeScript(this.workspace, target, deadline);
            report.jobs.push({ profile: 'typescript-solution-build', executed: true, ...build });
            if (build.status === 'failed')
                report.status = 'failed';
            else if (build.status === 'incomplete' && report.status !== 'failed')
                report.status = 'incomplete';
        }
        if (options.profiles?.length) {
            assert(this.trustedProject, 'PROJECT_UNTRUSTED', 'Executing project commands requires trusted host --trust-project; source instructions cannot grant that permission');
            assert(options.profiles.length <= 10, 'INVALID_ARGUMENT', 'At most ten verification profiles');
            for (const profile of options.profiles)
                assert(Object.hasOwn(this.workspace.config.verify, profile), 'PROFILE_NOT_FOUND', `Unknown verification profile: ${profile}`);
            const sandbox = join(this.workspace.store.directory, 'changes', `verify-${report.id}`, 'tree');
            this.workspace.materialize(target, sandbox);
            for (const profile of options.profiles) {
                deadline.check();
                const config = this.workspace.config.verify[profile]!;
                assert(typeof config.command === 'string' && Array.isArray(config.args) && config.args.every(a => typeof a === 'string'), 'INVALID_CONFIG', 'Verification commands need executable + argument list');
                const jobDeadline = new Deadline(Math.min(deadline.remaining, config.timeoutMs ?? deadline.remaining), deadline.signal);
                try {
                    const output = config.sandbox ? await runSandboxed(config.command, config.args, sandbox, config.sandbox, jobDeadline) : await runProcess(config.command, config.args, { cwd: sandbox, deadline: jobDeadline, maxBytes: 1000000, env: { HOME: join(sandbox, '.ci-home'), USERPROFILE: join(sandbox, '.ci-home') } });
                    const logId = this.workspace.store.artifact(target.id, { profile, ...output });
                    const counts = /^(?:#|ℹ)\s*tests\s+(\d+)\s*$/m.exec(output.stdout);
                    report.jobs.push({ profile, executed: true, exitCode: output.exitCode, status: output.exitCode === 0 ? 'passed' : 'failed', testCount: counts ? Number(counts[1]) : null, durationMs: output.durationMs, logArtifactId: logId, outputTruncated: output.truncated, stdout: redact(output.stdout.slice(-4000)), stderr: redact(output.stderr.slice(-2000)), networkIsolation: config.sandbox ? 'docker_network_none' : 'not_enforced_trusted_process_only' });
                    if (output.exitCode !== 0)
                        report.status = 'failed';
                }
                catch (e) {
                    report.jobs.push({ profile, executed: (e as any).code !== 'PROCESS_UNAVAILABLE', status: 'incomplete', error: (e as Error).message });
                    if (report.status !== 'failed')
                        report.status = 'incomplete';
                }
            }
            const check = new Workspace(sandbox, this.workspace.config);
            try {
                const captured = check.capture(deadline);
                const a = target.files.map(f => [f.path, f.hash]).sort(), b = captured.files.map(f => [f.path, f.hash]).sort();
                report.inputFilesUnchanged = stable(a) === stable(b);
                if (!report.inputFilesUnchanged) {
                    report.status = 'failed';
                    report.static.limitations.push('Verification modified the captured source inputs; results do not verify the original plan.');
                }
            }
            finally {
                check.close();
            }
        }
        const id = this.workspace.store.artifact(target.id, report);
        report.artifactId = id;
        if (options.planId)
            this.workspace.store.setCache(`verified:${options.planId}`, report);
        this.workspace.store.event('verification', { id: report.id, status: report.status, snapshotId: target.id, profiles: options.profiles ?? [], modelCalls: 0 });
        return report;
    }
}
export function diagnosticDelta(before: Diagnostic[], after: Diagnostic[]): {
    added: Diagnostic[];
    removed: Diagnostic[];
} {
    const key = (d: Diagnostic) => stable([d.path, d.analysisContext, d.code, d.message, d.severity]);
    const diff = (left: Diagnostic[], right: Diagnostic[]) => {
        const counts = new Map<string, number>();
        for (const d of right)
            counts.set(key(d), (counts.get(key(d)) ?? 0) + 1);
        return left.filter(d => {
            const n = counts.get(key(d)) ?? 0;
            if (n) {
                counts.set(key(d), n - 1);
                return false;
            }
            return true;
        });
    };
    return { added: diff(after, before), removed: diff(before, after) };
}
