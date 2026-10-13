import ts from 'typescript';
import { TextIndex, assert, Deadline, hash, stable } from '../core/util.js';
import type { Extraction, LocalEntity, LocalObservation, Diagnostic, Snapshot, FileEdit, AnalysisContext } from '../core/types.js';
import { TypeScriptFileSystem, TypeScriptProjects, type ConfiguredProject } from './typescript-host.js';
const PRODUCER = `typescript-compiler@${ts.version}`;
class TypeScriptUnit {
    readonly service: ts.LanguageService;
    readonly program: ts.Program;
    readonly indexes: Map<string, TextIndex>;
    readonly contents: Map<string, string>;
    readonly abs: (path: string) => string;
    readonly rel: (path: string) => string | undefined;
    constructor(readonly snapshot: Snapshot, readonly fs: TypeScriptFileSystem, readonly configs: TypeScriptProjects, readonly config: ConfiguredProject) {
        this.indexes = fs.indexes;
        this.contents = fs.contents;
        this.abs = fs.abs;
        this.rel = fs.rel;
        const options: ts.CompilerOptions = { ...config.parsed.options, noEmit: true, plugins: [] };
        const host: ts.LanguageServiceHost & {
            useSourceOfProjectReferenceRedirect(): boolean;
        } = {
            getCompilationSettings: () => options,
            getScriptFileNames: () => config.parsed.fileNames,
            getScriptVersion: name => hash(fs.read(name) ?? ''),
            getScriptSnapshot: name => { const value = fs.read(name); return value === undefined ? undefined : ts.ScriptSnapshot.fromString(value); },
            getCurrentDirectory: () => fs.root,
            getDefaultLibFileName: options => ts.getDefaultLibFilePath(options),
            getProjectReferences: () => config.parsed.projectReferences,
            // Pinned compiler host hook: analyze captured referenced source, never emit build files.
            useSourceOfProjectReferenceRedirect: () => true,
            getParsedCommandLine: configs.parsed,
            readFile: fs.read, fileExists: fs.fileExists, readDirectory: fs.readDirectory,
            directoryExists: fs.directoryExists, getDirectories: fs.getDirectories,
            useCaseSensitiveFileNames: () => true,
            realpath: path => path.replaceAll('\\', '/'),
            writeFile: () => { throw new Error('Compiler is read-only'); },
        };
        this.service = ts.createLanguageService(host);
        try {
            const program = this.service.getProgram();
            assert(program, 'COMPILER_UNAVAILABLE', 'TypeScript did not create a source program');
            this.program = program;
        }
        catch (e) {
            this.service.dispose();
            throw e;
        }
    }
    extract(deadline: Deadline): Extraction {
        const checker = this.program.getTypeChecker();
        const entities: LocalEntity[] = [];
        const observations: LocalObservation[] = [];
        const declarations = new Map<ts.Node, LocalEntity>();
        const files = this.program.getSourceFiles().filter(f => this.rel(f.fileName) !== undefined);
        const candidate = (node: ts.Node): {
            name: ts.Node;
            kind: string;
        } | undefined => {
            if (ts.isFunctionDeclaration(node) && node.name)
                return { name: node.name, kind: 'function' };
            if (ts.isClassDeclaration(node) && node.name)
                return { name: node.name, kind: 'class' };
            if (ts.isInterfaceDeclaration(node))
                return { name: node.name, kind: 'interface' };
            if (ts.isMethodDeclaration(node) || ts.isMethodSignature(node))
                return { name: node.name, kind: 'method' };
            if (ts.isConstructorDeclaration(node))
                return { name: node, kind: 'constructor' };
            if (ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node))
                return { name: node.name, kind: 'accessor' };
            if (ts.isTypeAliasDeclaration(node))
                return { name: node.name, kind: 'type' };
            if (ts.isEnumDeclaration(node))
                return { name: node.name, kind: 'enum' };
            if (ts.isEnumMember(node))
                return { name: node.name, kind: 'enum_member' };
            if (ts.isModuleDeclaration(node))
                return { name: node.name, kind: 'namespace' };
            if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name))
                return { name: node.name, kind: node.initializer && (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer)) ? 'function' : 'variable' };
            if ((ts.isPropertyDeclaration(node) || ts.isPropertySignature(node)) && node.name)
                return { name: node.name, kind: 'property' };
            if ((ts.isParameter(node) || ts.isBindingElement(node)) && ts.isIdentifier(node.name))
                return { name: node.name, kind: 'binding' };
            return undefined;
        };
        for (const sf of files) {
            deadline.check();
            const path = this.rel(sf.fileName)!, index = this.indexes.get(path)!;
            const visit = (node: ts.Node, parent?: LocalEntity) => {
                const item = candidate(node);
                let owner = parent;
                if (item) {
                    const start = node.getStart(sf), end = node.getEnd();
                    const name = item.kind === 'constructor' ? 'constructor' : item.name.getText(sf);
                    const nameStart = item.name.getStart(sf), nameEnd = item.kind === 'constructor' ? nameStart + 11 : item.name.getEnd();
                    const entry: LocalEntity = { key: `${path}#${start}:${item.kind}:${name}`, path, name, qualifiedName: parent ? `${parent.qualifiedName}.${name}` : name, kind: item.kind, language: /\.[cm]?jsx?$/.test(path) ? 'javascript' : 'typescript', signature: node.getText(sf).split('\n')[0]!.slice(0, 400), producer: PRODUCER, parentKey: parent?.key ?? `file:${path}`, ...index.range(start, end), nameStartByte: index.byte(nameStart), nameEndByte: index.byte(nameEnd), exported: (ts.getCombinedModifierFlags(node as ts.Declaration) & ts.ModifierFlags.Export) !== 0 };
                    entities.push(entry);
                    declarations.set(node, entry);
                    owner = entry;
                }
                ts.forEachChild(node, child => visit(child, owner));
            };
            visit(sf);
        }
        const resolved = (node: ts.Node): LocalEntity | undefined => {
            let symbol = checker.getSymbolAtLocation(node);
            if (symbol && (symbol.flags & ts.SymbolFlags.Alias)) {
                try {
                    symbol = checker.getAliasedSymbol(symbol);
                }
                catch {
                    return undefined;
                }
            }
            const defs = symbol?.declarations?.map(d => declarations.get(d)).filter((v): v is LocalEntity => !!v) ?? [];
            // Overloads remain distinct occurrences; resolve a call by its signature below.
            return defs.length === 1 ? defs[0] : defs.find(d => d.kind === 'function' || d.kind === 'method');
        };
        for (const sf of files) {
            deadline.check();
            const path = this.rel(sf.fileName)!, index = this.indexes.get(path)!;
            const edge = (source: string, target: LocalEntity | undefined, relation: string, node: ts.Node, label: string, resolution: LocalObservation['resolution'] = target ? 'exact_binding' : 'unresolved', targetKey?: string) => observations.push({ sourceKey: source, targetKey: targetKey ?? target?.key, targetLabel: target || targetKey ? undefined : label, relation, resolution, path, producer: PRODUCER, ...index.range(node.getStart(sf), node.getEnd()) });
            const visit = (node: ts.Node, owner = `file:${path}`): void => {
                const entry = declarations.get(node);
                if (entry) {
                    edge(owner, entry, 'contains', node, entry.name, 'syntactic');
                    owner = entry.key;
                }
                if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
                    const symbol = checker.getSymbolAtLocation(node.moduleSpecifier);
                    const sfTarget = symbol?.declarations?.find(ts.isSourceFile);
                    const targetPath = sfTarget ? this.rel(sfTarget.fileName) : undefined;
                    edge(`file:${path}`, undefined, 'imports', node.moduleSpecifier, node.moduleSpecifier.text, targetPath ? 'exact_binding' : 'unresolved', targetPath ? `file:${targetPath}` : undefined);
                }
                if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
                    const signature = checker.getResolvedSignature(node);
                    let target = signature?.declaration ? declarations.get(signature.declaration) : undefined;
                    const expr = ts.isPropertyAccessExpression(node.expression) ? node.expression.name : node.expression;
                    target ??= resolved(expr);
                    const dispatch = signature?.declaration && (ts.isMethodSignature(signature.declaration) || ts.isCallSignatureDeclaration(signature.declaration));
                    edge(owner, target, ts.isNewExpression(node) ? 'constructs' : 'calls', node.expression, node.expression.getText(sf), dispatch ? 'possible_dispatch' : target ? 'exact_binding' : 'unresolved');
                }
                if (ts.isIdentifier(node) && !(node.parent && declarations.has(node.parent) && (node.parent as ts.NamedDeclaration).name === node)) {
                    const target = resolved(node);
                    if (target)
                        edge(owner, target, 'references', node, node.text);
                }
                if (ts.isHeritageClause(node))
                    for (const t of node.types)
                        edge(owner, resolved(t.expression), node.token === ts.SyntaxKind.ImplementsKeyword ? 'implements' : 'inherits', t.expression, t.expression.getText(sf));
                ts.forEachChild(node, child => visit(child, owner));
            };
            visit(sf);
        }
        for (const edge of observations)
            edge.analysisContext = this.config.name;
        return { entities, observations, diagnostics: this.diagnostics(deadline), limitations: ['TypeScript semantic bindings are not runtime dispatch proofs.', 'Only captured installed declaration/configuration files are available. Missing packages and symlinked dependency stores remain explicit omissions.', 'Bindings are scoped to the reported TypeScript configuration; alternative configurations are retained separately.'] };
    }
    diagnostics(deadline = new Deadline(60000)): Diagnostic[] {
        const diagnostics: ts.Diagnostic[] = [...this.program.getOptionsDiagnostics(), ...this.program.getGlobalDiagnostics()];
        for (const source of this.program.getSourceFiles())
            if (this.rel(source.fileName) !== undefined) {
                deadline.check();
                diagnostics.push(...this.program.getSyntacticDiagnostics(source), ...this.program.getSemanticDiagnostics(source));
            }
        return diagnostics.slice(0, 3000).map(d => this.configs.diagnostic(d, this.config.name));
    }
    rename(path: string, byte: number, name: string): FileEdit[] {
        assert(/^[$_\p{L}][$_\p{L}\p{N}]*$/u.test(name), 'INVALID_RENAME', 'New name must be a JavaScript identifier');
        const scanner = ts.createScanner(ts.ScriptTarget.Latest, false, ts.LanguageVariant.Standard, name);
        assert(scanner.scan() === ts.SyntaxKind.Identifier && scanner.scan() === ts.SyntaxKind.EndOfFileToken, 'INVALID_RENAME', 'Reserved keywords are not valid rename targets');
        const index = this.indexes.get(path);
        assert(index, 'FILE_NOT_FOUND', 'Rename file is missing');
        const position = index.utf16(byte);
        const info = this.service.getRenameInfo(this.abs(path), position, { allowRenameOfImportPath: false });
        assert(info.canRename, 'RENAME_UNAVAILABLE', info.canRename ? '' : info.localizedErrorMessage);
        const locations = this.service.findRenameLocations(this.abs(path), position, false, false, true);
        assert(locations?.length, 'RENAME_UNAVAILABLE', 'Compiler returned no rename locations');
        return locations.map(loc => {
            const target = this.rel(loc.fileName);
            assert(target, 'SCOPE_VIOLATION', 'Rename targets a file outside the snapshot');
            const ti = this.indexes.get(target)!;
            const file = this.snapshot.files.find(f => f.path === target)!;
            return { kind: 'replace', path: target, expectedHash: file.hash, startByte: ti.byte(loc.textSpan.start), endByte: ti.byte(loc.textSpan.start + loc.textSpan.length), text: (loc.prefixText ?? '') + name + (loc.suffixText ?? '') };
        });
    }
    close(): void { this.service.dispose(); }
}
/** Snapshot-wide facade over independently configured TypeScript programs.
 * Entities keep source identity; observations and diagnostics retain context.
 */
export class TypeScriptProject {
    readonly fs: TypeScriptFileSystem;
    readonly configs: TypeScriptProjects;
    private readonly units: TypeScriptUnit[] = [];
    constructor(readonly snapshot: Snapshot, readonly contents: Map<string, string>, root: string, deadline = new Deadline(120000)) {
        this.fs = new TypeScriptFileSystem(contents, root);
        this.configs = new TypeScriptProjects(this.fs, deadline);
        try {
            for (const config of this.configs.projects.values()) {
                deadline.check();
                if (config.parsed.fileNames.length)
                    this.units.push(new TypeScriptUnit(snapshot, this.fs, this.configs, config));
            }
        }
        catch (e) {
            this.close();
            throw e;
        }
    }
    get indexes(): Map<string, TextIndex> { return this.fs.indexes; }
    /** Legacy single-program access. Use contexts() for multi-project analysis. */
    get program(): ts.Program { assert(this.units[0], 'CAPABILITY_UNAVAILABLE', 'No configured TypeScript source program'); return this.units[0].program; }
    get service(): ts.LanguageService { assert(this.units[0], 'CAPABILITY_UNAVAILABLE', 'No configured TypeScript source program'); return this.units[0].service; }
    abs(path: string): string { return this.fs.abs(path); }
    rel(path: string): string | undefined { return this.fs.rel(path); }
    analysisUnits(): {
        id: string;
        program: ts.Program;
    }[] { return this.units.map(u => ({ id: u.config.name, program: u.program })); }
    contexts(): AnalysisContext[] {
        return [...this.configs.projects.values()].map(config => {
            const unit = this.units.find(u => u.config === config);
            return this.configs.context(config, unit?.program.getSourceFiles().map(f => this.rel(f.fileName)).filter((p): p is string => p !== undefined) ?? []);
        });
    }
    limitations(): string[] {
        const contexts = this.contexts(), known = new Set(contexts.flatMap(c => c.analyzedFiles));
        const missing = this.snapshot.files.filter(f => ['typescript', 'javascript'].includes(f.language) && !known.has(f.path));
        return [...this.configs.limitations, ...contexts.flatMap(c => c.limitations),
            ...(missing.length ? [`TypeScript semantic analysis unavailable for ${missing.length} captured files outside configured programs; file/text search still works.`] : []),
            'Only immutable source/dependency captures and trusted TypeScript standard libraries are loaded. Package code and compiler plugins are not executed.'];
    }
    cacheStats = { hits: 0, misses: 0 };
    extract(deadline: Deadline, cache?: {
        get(key: string): Extraction | undefined;
        put(key: string, value: Extraction): void;
    }): Extraction {
        this.cacheStats = { hits: 0, misses: 0 };
        const entities = new Map<string, LocalEntity>(), memberships = new Map<string, Set<string>>();
        const observations: LocalObservation[] = [], diagnostics: Diagnostic[] = [...this.configs.projects.values()].flatMap(c => c.problems);
        const limitations = new Set(this.limitations());
        for (const unit of this.units) {
            deadline.check();
            const sourceFiles = unit.program.getSourceFiles().map(f => this.rel(f.fileName)).filter((p): p is string => p !== undefined);
            const metadata = this.snapshot.files.filter(f => /(?:tsconfig|jsconfig|package|lock).*\.json$/.test(f.path)).map(f => [f.path, f.hash]);
            const key = 'ts-unit:' + hash(stable({ compiler: ts.version, producer: 'ci-ts-unit-v1', context: unit.config.name, options: { ...unit.config.parsed.options, configFile: undefined }, roots: unit.config.parsed.fileNames, references: unit.config.parsed.projectReferences, metadata, availablePaths: this.snapshot.files.map(f => f.path), sources: sourceFiles.map(path => [path, hash(this.contents.get(path)!)]) }));
            let result = cache?.get(key);
            if (result)
                this.cacheStats.hits++;
            else {
                result = unit.extract(deadline);
                this.cacheStats.misses++;
                cache?.put(key, result);
            }
            for (const entity of result.entities) {
                const prior = entities.get(entity.key);
                assert(!prior || stable(prior) === stable(entity), 'IDENTITY_COLLISION', 'TypeScript source entity differs across configuration contexts');
                entities.set(entity.key, entity);
                const contexts = memberships.get(entity.key) ?? new Set<string>();
                contexts.add(unit.config.name);
                memberships.set(entity.key, contexts);
            }
            observations.push(...result.observations);
            diagnostics.push(...result.diagnostics);
            for (const limit of result.limitations)
                limitations.add(limit);
        }
        for (const [key, entity] of entities)
            entity.metadata = { ...entity.metadata, analysisContexts: [...memberships.get(key)!].sort() };
        return { entities: [...entities.values()], observations, diagnostics, limitations: [...limitations], analysisContexts: this.contexts() };
    }
    diagnostics(deadline = new Deadline(60000)): Diagnostic[] {
        const diagnostics = [...this.configs.projects.values()].flatMap(c => c.problems);
        for (const unit of this.units) {
            deadline.check();
            diagnostics.push(...unit.diagnostics(deadline));
        }
        return diagnostics;
    }
    rename(path: string, byte: number, name: string, deadline = new Deadline(60000)): FileEdit[] {
        const edits = new Map<string, FileEdit>(), replacements = new Map<string, string>();
        const perContext = new Map<TypeScriptUnit, Set<string>>();
        for (const unit of this.units) {
            deadline.check();
            if (!unit.program.getSourceFile(this.abs(path)))
                continue;
            const contextEdits = new Set<string>();
            perContext.set(unit, contextEdits);
            // A failure in any participating context must not silently produce a partial rename.
            for (const edit of unit.rename(path, byte, name)) {
                assert(edit.kind === 'replace', 'COMPILER_CONTRACT', 'Compiler rename returned a resource operation');
                const key = stable([edit.path, edit.startByte, edit.endByte]);
                const old = replacements.get(key);
                assert(old === undefined || old === edit.text, 'RENAME_CONTEXT_CONFLICT', 'Configurations disagree on a rename replacement; use separate reviewed plans');
                replacements.set(key, edit.text);
                edits.set(stable(edit), edit);
                contextEdits.add(stable(edit));
            }
        }
        deadline.check();
        assert(edits.size, 'RENAME_UNAVAILABLE', 'No configured program contains the rename target');
        for (const [key, edit] of edits)
            for (const unit of this.units) {
                deadline.check();
                // A shared consumer may bind the same spelling to a DIFFERENT target
                // under another build variant. A rename must agree in every context
                // that analyzes an edited file, not merely contexts holding its definition.
                assert(!unit.program.getSourceFile(this.abs(edit.path)) || perContext.get(unit)?.has(key), 'RENAME_CONTEXT_CONFLICT', `Rename would change ${edit.path} inconsistently in ${unit.config.name}; prepare a context-specific explicit patch instead`);
            }
        return [...edits.values()].sort((a, b) => a.path.localeCompare(b.path) || (a.kind === 'replace' && b.kind === 'replace' ? a.startByte - b.startByte : 0));
    }
    close(): void {
        for (const unit of this.units)
            unit.close();
    }
}
