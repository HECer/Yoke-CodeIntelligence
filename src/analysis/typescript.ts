import ts from 'typescript';
import { resolve, dirname, relative, isAbsolute, basename } from 'node:path';
import { readFileSync } from 'node:fs';
import { TextIndex, assert, Deadline, hash } from '../core/util.js';
import type { Extraction, LocalEntity, LocalObservation, Diagnostic, Snapshot, FileEdit } from '../core/types.js';
const PRODUCER = `typescript-compiler@${ts.version}`;
const slash = (p: string) => p.replaceAll('\\', '/');
/** Compiler IO is limited to immutable source files and the installed standard library. */
export class TypeScriptProject {
    readonly program: ts.Program;
    readonly service: ts.LanguageService;
    readonly indexes = new Map<string, TextIndex>();
    private readonly virtual: string;
    private readonly absolute = new Map<string, string>();
    private readonly options: ts.CompilerOptions;
    private readonly configDiagnostics: ts.Diagnostic[] = [];
    constructor(readonly snapshot: Snapshot, readonly contents: Map<string, string>, root: string) {
        this.virtual = slash(resolve(root, '.code-intelligence', 'virtual'));
        for (const [path, source] of contents) {
            this.absolute.set(this.abs(path), source);
            this.indexes.set(path, new TextIndex(source));
        }
        const lib = slash(dirname(ts.getDefaultLibFilePath({ target: ts.ScriptTarget.ES2022 })));
        const read = (file: string): string | undefined => {
            const key = slash(resolve(file));
            const local = this.absolute.get(key);
            if (local !== undefined)
                return local;
            if (dirname(key) === lib && /^lib(?:\.[a-z0-9.-]+)?\.d\.ts$/i.test(basename(key))) {
                try {
                    return readFileSync(key, 'utf8');
                }
                catch {
                    return undefined;
                }
            }
            return undefined;
        };
        const fileExists = (file: string) => read(file) !== undefined;
        const directories = new Set<string>();
        for (const file of this.absolute.keys()) {
            let d = slash(dirname(file));
            while (d.startsWith(this.virtual)) {
                directories.add(d);
                const next = slash(dirname(d));
                if (next === d)
                    break;
                d = next;
            }
        }
        const base: ts.CompilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, jsx: ts.JsxEmit.Preserve, allowJs: true, checkJs: true, noEmit: true, skipLibCheck: true, strict: true, types: [] };
        const configName = contents.has('tsconfig.json') ? 'tsconfig.json' : contents.has('jsconfig.json') ? 'jsconfig.json' : undefined;
        if (configName) {
            const parsed = ts.parseConfigFileTextToJson(this.abs(configName), contents.get(configName)!);
            if (parsed.error)
                this.configDiagnostics.push(parsed.error);
            const result = ts.parseJsonConfigFileContent(parsed.config ?? {}, { useCaseSensitiveFileNames: true, readFile: read, fileExists, readDirectory: (_path, extensions) => [...this.absolute.keys()].filter(p => !extensions || extensions.some(e => p.endsWith(e))) }, this.virtual);
            this.configDiagnostics.push(...result.errors.filter(e => e.code !== 18003));
            this.options = { ...base, ...result.options, noEmit: true, skipLibCheck: true, types: [] };
        }
        else
            this.options = base;
        const roots = [...contents.keys()].filter(p => /\.[cm]?[jt]sx?$/.test(p)).map(p => this.abs(p));
        const host = ts.createCompilerHost(this.options, true);
        host.readFile = read;
        host.fileExists = fileExists;
        host.directoryExists = path => directories.has(slash(resolve(path))) || slash(resolve(path)) === lib;
        host.getDirectories = () => [];
        host.getCurrentDirectory = () => this.virtual;
        host.realpath = slash;
        host.writeFile = () => { throw new Error('Compiler is read-only'); };
        host.getSourceFile = (fileName, languageVersion) => { const value = read(fileName); return value === undefined ? undefined : ts.createSourceFile(fileName, value, languageVersion, true); };
        this.program = ts.createProgram(roots, this.options, host);
        const serviceHost: ts.LanguageServiceHost = {
            getCompilationSettings: () => this.options, getScriptFileNames: () => roots,
            getScriptVersion: name => hash(read(name) ?? ''), getScriptSnapshot: name => { const value = read(name); return value === undefined ? undefined : ts.ScriptSnapshot.fromString(value); },
            getCurrentDirectory: host.getCurrentDirectory, getDefaultLibFileName: options => ts.getDefaultLibFilePath(options),
            readFile: read, fileExists, directoryExists: host.directoryExists, getDirectories: host.getDirectories,
            useCaseSensitiveFileNames: () => true, realpath: slash,
        };
        this.service = ts.createLanguageService(serviceHost);
    }
    abs(path: string): string { return slash(resolve(this.virtual, path)); }
    rel(file: string): string | undefined { const path = slash(relative(this.virtual, file)); return path.startsWith('../') || isAbsolute(path) || !this.contents.has(path) ? undefined : path; }
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
        return { entities, observations, diagnostics: this.diagnostics(deadline), limitations: ['TypeScript semantic bindings are not runtime dispatch proofs.', 'External dependency source and @types packages are excluded unless explicitly indexed; missing dependencies reduce semantic coverage.', 'Project references and multiple conflicting tsconfig build variants are not merged into one authoritative configuration.'] };
    }
    diagnostics(deadline = new Deadline(60000)): Diagnostic[] {
        const diagnostics: ts.Diagnostic[] = [...this.configDiagnostics];
        for (const source of this.program.getSourceFiles())
            if (this.rel(source.fileName) !== undefined) {
                deadline.check();
                diagnostics.push(...this.program.getSyntacticDiagnostics(source), ...this.program.getSemanticDiagnostics(source));
            }
        return diagnostics.slice(0, 3000).map(d => {
            const path = d.file ? this.rel(d.file.fileName) ?? 'tsconfig.json' : 'tsconfig.json';
            return { path, code: `TS${d.code}`, message: ts.flattenDiagnosticMessageText(d.messageText, '\n'), severity: d.category === ts.DiagnosticCategory.Error ? 'error' : 'warning', startByte: d.start !== undefined ? this.indexes.get(path)?.byte(d.start) : undefined };
        });
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
