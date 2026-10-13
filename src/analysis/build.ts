/** Pinned tsc solution builds using a virtual filesystem: generated output is an artifact, never a source-tree write. */
import ts from 'typescript';
import { dirname, relative, resolve } from 'node:path';
import { Workspace } from '../core/workspace.js';
import { Snapshot, Diagnostic } from '../core/types.js';
import { TypeScriptFileSystem, TypeScriptProjects } from './typescript-host.js';
import { assert, Deadline, hash, stable } from '../core/util.js';
export interface BuildReport {
    status: 'passed' | 'failed' | 'incomplete';
    snapshotId: string;
    compiler: string;
    diagnostics: Diagnostic[];
    outputs: {
        path: string;
        hash: string;
        bytes: number;
    }[];
    artifactId?: string;
    exitCode: number | null;
    sourceExecuted: false;
    sourceTreeWritten: false;
}
export function buildTypeScript(workspace: Workspace, snapshot: Snapshot, deadline = new Deadline(120000)): BuildReport {
    const contents = workspace.contents(snapshot), initial = new TypeScriptFileSystem(contents, workspace.root), configs = new TypeScriptProjects(initial, deadline);
    const entries = [...configs.projects.keys()].filter(n => n !== '[inferred]');
    const diagnostics: Diagnostic[] = [], outputs = new Map<string, string>();
    let fs = initial, dirty = false, bytes = 0;
    const current = () => { if (dirty) {
        fs = new TypeScriptFileSystem(new Map([...contents, ...outputs]), workspace.root);
        dirty = false;
    } return fs; };
    const local = (path: string) => { const p = relative(initial.root, resolve(path)).replaceAll('\\', '/'); assert(p && !p.startsWith('../') && p !== '..' && !/^[A-Za-z]:|^\//.test(p), 'BUILD_SCOPE_VIOLATION', 'Compiler output may not leave the immutable virtual root'); return p; };
    const convert = (d: ts.Diagnostic) => { deadline.check(); diagnostics.push(configs.diagnostic(d, '[solution-build]')); };
    if (!entries.length)
        return { status: 'incomplete', snapshotId: snapshot.id, compiler: ts.version, diagnostics: [], outputs: [], exitCode: null, sourceExecuted: false, sourceTreeWritten: false };
    const system: ts.System = { ...ts.sys,
        args: [], newLine: '\n', useCaseSensitiveFileNames: true, write: () => { }, getCurrentDirectory: () => initial.root,
        getExecutingFilePath: () => ts.sys.getExecutingFilePath(),
        readFile: path => { deadline.check(); return current().read(path); }, fileExists: path => current().fileExists(path),
        directoryExists: path => current().directoryExists(path), getDirectories: path => current().getDirectories(path), readDirectory: (...args) => current().readDirectory(...args),
        realpath: path => resolve(path).replaceAll('\\', '/'),
        createDirectory: path => { local(path); },
        writeFile: (path, content) => {
            deadline.check();
            const p = local(path);
            assert(!contents.has(p), 'BUILD_OUTPUT_CONFLICT', 'Compiler attempted to overwrite an immutable input');
            bytes -= Buffer.byteLength(outputs.get(p) ?? '');
            bytes += Buffer.byteLength(content);
            assert(bytes <= 64000000 && outputs.size < 20000, 'CAPACITY_EXCEEDED', 'Generated build output exceeded limits');
            outputs.set(p, content);
            dirty = true;
        },
        deleteFile: path => { outputs.delete(local(path)); dirty = true; },
        getModifiedTime: path => current().fileExists(path) ? new Date(outputs.has(relative(initial.root, path).replaceAll('\\', '/')) ? 2000 : 1000) : undefined,
        setModifiedTime: () => { }, exit: () => { throw new Error('Unexpected compiler process exit'); },
    };
    const host = ts.createSolutionBuilderHost(system, undefined, convert, () => { }, () => { });
    const result = ts.createSolutionBuilder(host, entries.map(initial.abs), { force: true }).build();
    deadline.check();
    const outputList = [...outputs].sort(([a], [b]) => a.localeCompare(b)).map(([path, content]) => ({ path, hash: hash(content), bytes: Buffer.byteLength(content) }));
    const report: BuildReport = { status: result === ts.ExitStatus.Success && !diagnostics.some(d => d.severity === 'error') ? 'passed' : 'failed', snapshotId: snapshot.id, compiler: ts.version, diagnostics, outputs: outputList, exitCode: result, sourceExecuted: false, sourceTreeWritten: false };
    report.artifactId = workspace.store.artifact(snapshot.id, { ...report, generatedFiles: Object.fromEntries(outputs) });
    workspace.store.setCache(`solution-build:${snapshot.id}`, report);
    return report;
}
