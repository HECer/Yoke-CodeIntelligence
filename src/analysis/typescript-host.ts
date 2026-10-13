import ts from 'typescript';
import { basename, dirname, isAbsolute, relative, resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import { assert, Deadline, TextIndex } from '../core/util.js';
import type { AnalysisContext, Diagnostic } from '../core/types.js';
const slash = (p: string) => p.replaceAll('\\', '/');
const isSource = (p: string) => /\.[cm]?[jt]sx?$/i.test(p);
/** Uses the pinned compiler's own file matcher, backed ONLY by immutable entries.
 * matchFiles is a compiler export, not a stable public API: retain the version
 * pin and the include/exclude/extends contract tests when changing TypeScript.
 */
type MatchFiles = (path: string, extensions: readonly string[] | undefined, excludes: readonly string[] | undefined, includes: readonly string[] | undefined, caseSensitive: boolean, cwd: string, depth: number | undefined, entries: (dir: string) => {
    files: string[];
    directories: string[];
}, realpath: (path: string) => string) => string[];
export interface ConfiguredProject {
    name: string;
    parsed: ts.ParsedCommandLine;
    problems: Diagnostic[];
    limitations: string[];
}
/** A compiler filesystem that cannot fall through to the live repository. */
export class TypeScriptFileSystem {
    readonly root: string;
    readonly lib: string;
    readonly indexes = new Map<string, TextIndex>();
    private readonly source = new Map<string, string>();
    private readonly directories = new Map<string, {
        files: Set<string>;
        directories: Set<string>;
    }>();
    private readonly library = new Map<string, string | undefined>();
    constructor(readonly contents: Map<string, string>, workspaceRoot: string) {
        this.root = slash(resolve(workspaceRoot, '.code-intelligence', 'virtual'));
        this.lib = slash(dirname(ts.getDefaultLibFilePath({ target: ts.ScriptTarget.ES2022 })));
        this.directories.set(this.root, { files: new Set(), directories: new Set() });
        for (const [path, content] of contents) {
            const abs = this.abs(path);
            this.source.set(abs, content);
            this.indexes.set(path, new TextIndex(content));
            let child = abs, directory = slash(dirname(child)), file = true;
            while (directory === this.root || directory.startsWith(this.root + '/')) {
                const entry = this.directories.get(directory) ?? { files: new Set<string>(), directories: new Set<string>() };
                (file ? entry.files : entry.directories).add(basename(child));
                this.directories.set(directory, entry);
                if (directory === this.root)
                    break;
                child = directory;
                directory = slash(dirname(child));
                file = false;
            }
        }
    }
    abs = (path: string): string => slash(resolve(this.root, path));
    rel = (path: string): string | undefined => {
        const rel = slash(relative(this.root, path));
        return rel === '..' || rel.startsWith('../') || isAbsolute(rel) || !this.contents.has(rel) ? undefined : rel;
    };
    read = (path: string): string | undefined => {
        const abs = slash(resolve(path));
        if (this.source.has(abs))
            return this.source.get(abs);
        // This allowlist is the sole disk read: the trusted installed compiler's libs.
        if (slash(dirname(abs)) !== this.lib || !/^lib(?:\.[a-z0-9.-]+)?\.d\.ts$/i.test(basename(abs)))
            return undefined;
        if (!this.library.has(abs)) {
            try {
                this.library.set(abs, readFileSync(abs, 'utf8'));
            }
            catch {
                this.library.set(abs, undefined);
            }
        }
        return this.library.get(abs);
    };
    fileExists = (path: string): boolean => this.read(path) !== undefined;
    directoryExists = (path: string): boolean => this.directories.has(slash(resolve(path))) || slash(resolve(path)) === this.lib;
    getDirectories = (path: string): string[] => [...(this.directories.get(slash(resolve(path)))?.directories ?? [])].sort();
    readDirectory = (path: string, extensions?: readonly string[], excludes?: readonly string[], includes?: readonly string[], depth?: number): string[] => {
        const match = (ts as unknown as {
            matchFiles: MatchFiles;
        }).matchFiles;
        assert(typeof match === 'function', 'COMPILER_CONTRACT', 'Pinned TypeScript file matcher is unavailable');
        return match(slash(resolve(path)), extensions, excludes, includes, true, this.root, depth, dir => {
            const entry = this.directories.get(slash(resolve(dir)));
            return { files: [...(entry?.files ?? [])].sort(), directories: [...(entry?.directories ?? [])].sort() };
        }, p => slash(resolve(p)));
    };
}
/** Discover default configs and explicitly referenced arbitrary config names.
 * An extends-only base is NOT independently activated as a build project.
 */
export class TypeScriptProjects {
    readonly projects = new Map<string, ConfiguredProject>();
    readonly limitations: string[] = [];
    constructor(readonly fs: TypeScriptFileSystem, deadline: Deadline) {
        const names = [...fs.contents.keys()].filter(p => /(?:^|\/)(?:tsconfig|jsconfig)\.json$/.test(p))
            .filter(p => !p.endsWith('jsconfig.json') || !fs.contents.has(p.slice(0, -'jsconfig.json'.length) + 'tsconfig.json')).sort();
        const queue = [...names];
        for (let i = 0; i < queue.length; i++) {
            deadline.check();
            const name = queue[i]!;
            if (this.projects.has(name))
                continue;
            assert(this.projects.size < 128, 'CAPACITY_EXCEEDED', 'More than 128 TypeScript projects; narrow the workspace');
            const errors: ts.Diagnostic[] = [];
            const parsed = ts.getParsedCommandLineOfConfigFile(fs.abs(name), {}, {
                useCaseSensitiveFileNames: true, getCurrentDirectory: () => fs.root,
                fileExists: fs.fileExists, readFile: fs.read, readDirectory: fs.readDirectory,
                onUnRecoverableConfigFileDiagnostic: d => errors.push(d),
            }) ?? { fileNames: [], options: {}, errors: [] };
            errors.push(...parsed.errors);
            const problems = errors.map(d => this.diagnostic(d, name));
            const limits: string[] = [];
            if (Array.isArray(parsed.options.plugins) && parsed.options.plugins.length)
                limits.push(`${name}: compiler plugins are not loaded; configuration is data, not execution permission.`);
            const project: ConfiguredProject = { name, parsed, problems, limitations: limits };
            this.projects.set(name, project);
            const references: ts.ProjectReference[] = [];
            for (const ref of parsed.projectReferences ?? []) {
                const configFile = /\.json$/i.test(ref.path) ? ref.path : resolve(ref.path, 'tsconfig.json');
                const target = fs.rel(configFile);
                if (!target) {
                    problems.push({ path: name, analysisContext: name, code: 'CI_TS_REFERENCE_UNAVAILABLE', severity: 'error', message: 'Referenced config is outside the immutable snapshot or missing: ' + slash(relative(fs.root, configFile)) });
                    limits.push(`${name}: referenced project unavailable in the captured source.`);
                    continue;
                }
                references.push({ ...ref, path: fs.abs(target) });
                queue.push(target);
            }
            parsed.projectReferences = references;
        }
        // Remove only back-edges; report cycles rather than overflowing compiler recursion.
        const active = new Set<string>(), done = new Set<string>();
        const visit = (name: string) => {
            deadline.check();
            if (done.has(name))
                return;
            active.add(name);
            const project = this.projects.get(name)!;
            project.parsed.projectReferences = (project.parsed.projectReferences ?? []).filter(ref => {
                const target = fs.rel(ref.path)!;
                if (active.has(target)) {
                    project.problems.push({ path: name, analysisContext: name, code: 'CI_TS_REFERENCE_CYCLE', severity: 'error', message: `Project reference cycle includes ${target}` });
                    project.limitations.push(`${name}: circular reference omitted from analysis.`);
                    return false;
                }
                visit(target);
                return true;
            });
            active.delete(name);
            done.add(name);
        };
        for (const name of this.projects.keys())
            visit(name);
        if (!this.projects.size) {
            this.projects.set('[inferred]', { name: '[inferred]', parsed: { fileNames: [...fs.contents.keys()].filter(isSource).map(fs.abs), options: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, jsx: ts.JsxEmit.Preserve, allowJs: true, checkJs: true, strict: true, skipLibCheck: true, types: [] }, errors: [] }, problems: [], limitations: ['No tsconfig/jsconfig was found; an explicitly inferred TypeScript configuration is used.'] });
        }
        if (names.length)
            this.limitations.push('Configured project roots honor files/include/exclude. Captured files outside all programs remain available for file/text search, not implied semantic analysis.');
        this.limitations.push('TypeScript source-reference navigation is editor-style analysis, not a tsc --build result or generated declaration validation.');
    }
    parsed = (fileName: string): ts.ParsedCommandLine | undefined => {
        const path = this.fs.rel(fileName);
        return path ? this.projects.get(path)?.parsed : undefined;
    };
    diagnostic(d: ts.Diagnostic, context: string): Diagnostic {
        const path = d.file ? this.fs.rel(d.file.fileName) ?? context : context;
        return { path, analysisContext: context, code: `TS${d.code}`, message: ts.flattenDiagnosticMessageText(d.messageText, '\n'), severity: d.category === ts.DiagnosticCategory.Error ? 'error' : 'warning', startByte: d.start !== undefined ? this.fs.indexes.get(path)?.byte(d.start) : undefined };
    }
    context(project: ConfiguredProject, analyzedFiles: string[]): AnalysisContext {
        return { id: project.name, provider: `typescript-compiler@${ts.version}`, configPath: project.name === '[inferred]' ? null : project.name,
            rootFiles: project.parsed.fileNames.map(p => this.fs.rel(p)).filter((p): p is string => p !== undefined),
            analyzedFiles: [...new Set(analyzedFiles)].sort(), references: (project.parsed.projectReferences ?? []).map(r => this.fs.rel(r.path)!).filter(Boolean),
            limitations: project.limitations };
    }
}
