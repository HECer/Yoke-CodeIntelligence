/** Bounded, intraprocedural TypeScript dataflow. Findings are candidates, not proofs. */
import ts from 'typescript';
import { CodeIntelligence } from '../engine.js';
import { Snapshot } from '../core/types.js';
import { TypeScriptProject } from './typescript.js';
import { Deadline, TextIndex, assert, stable } from '../core/util.js';
export interface TaintPolicy {
    sources: string[];
    sinks: Record<string, number[]>;
    sanitizers: string[];
    maxIterations?: number;
}
export const DEFAULT_TAINT_POLICY: TaintPolicy = { sources: ['req.query', 'req.body', 'req.params', 'request.query', 'request.body', 'location.search'], sinks: { eval: [0], exec: [0], execSync: [0], query: [0] }, sanitizers: [], maxIterations: 32 };
type Env = Map<ts.Symbol, Set<string>>;
const merge = (a: Env, b: Env) => { const out = new Map([...a].map(([s, v]) => [s, new Set(v)])); for (const [s, v] of b) {
    const x = out.get(s) ?? new Set();
    for (const i of v)
        x.add(i);
    out.set(s, x);
} return out; };
export function analyzeTaint(engine: CodeIntelligence, snapshot: Snapshot, policy: TaintPolicy, deadline: Deadline): Record<string, unknown> {
    assert(policy.maxIterations === undefined || Number.isInteger(policy.maxIterations) && policy.maxIterations >= 1 && policy.maxIterations <= 64, 'INVALID_POLICY', 'maxIterations must be 1–64');
    assert(policy.sources.length <= 100 && Object.keys(policy.sinks).length <= 100 && policy.sanitizers.length <= 100, 'INVALID_POLICY', 'Bounded host taint policy required');
    assert([...policy.sources, ...Object.keys(policy.sinks), ...policy.sanitizers].every(x => typeof x === 'string' && x.length <= 200), 'INVALID_POLICY', 'Taint selectors must be bounded strings');
    assert(Object.values(policy.sinks).every(v => Array.isArray(v) && v.length <= 50 && v.every(n => Number.isInteger(n) && n >= 0 && n <= 50)), 'INVALID_POLICY', 'Sink arguments must be bounded indices');
    const project = new TypeScriptProject(snapshot, engine.workspace.contents(snapshot), engine.workspace.root, deadline), findings = new Map<string, unknown>(), sources = new Map<string, unknown>();
    let unknownCalls = 0, boundedOut = false;
    try {
        for (const unit of project.analysisUnits()) {
            const checker = unit.program.getTypeChecker();
            for (const sf of unit.program.getSourceFiles()) {
                const path = project.rel(sf.fileName);
                if (!path || /\.d\.[cm]?ts$/.test(path))
                    continue;
                deadline.check();
                const ti = new TextIndex(sf.text);
                const anchor = (n: ts.Node) => ({ path, contentHash: engine.workspace.file(snapshot, path).hash, ...ti.range(n.getStart(sf), n.end) });
                const symbol = (n: ts.Node) => checker.getSymbolAtLocation(n);
                const name = (n: ts.Node) => { const sym = symbol(ts.isPropertyAccessExpression(n) ? n.name : n); if (sym) {
                    try {
                        const resolved = sym.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(sym) : sym;
                        return resolved.name;
                    }
                    catch { }
                } return n.getText(sf); };
                const select = (n: ts.Node, patterns: string[]) => { const raw = n.getText(sf); return patterns.find(p => raw === p || raw.startsWith(p + '.') || raw.startsWith(p + '[')); };
                const evaluate = (node: ts.Node | undefined, env: Env): Set<string> => {
                    if (!node)
                        return new Set();
                    deadline.check();
                    const source = select(node, policy.sources);
                    if (source) {
                        const id = `${path}:${node.getStart(sf)}`;
                        sources.set(id, { id, selector: source, ...anchor(node) });
                        return new Set([id]);
                    }
                    if (ts.isFunctionLike(node))
                        return new Set();
                    if (ts.isVariableDeclaration(node)) {
                        const values = evaluate(node.initializer, env);
                        if (ts.isIdentifier(node.name)) {
                            const sym = symbol(node.name);
                            if (sym)
                                env.set(sym, values);
                        }
                        return values;
                    }
                    if (ts.isIdentifier(node)) {
                        const sym = symbol(node);
                        return new Set(sym ? env.get(sym) : []);
                    }
                    if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
                        const callee = name(node.expression), raw = node.expression.getText(sf), args = (node.arguments ?? []).map(a => evaluate(a, env)), sink = policy.sinks[raw] ?? policy.sinks[callee];
                        if (sink) {
                            const used = new Set(sink.flatMap(i => [...(args[i] ?? [])]));
                            if (used.size) {
                                const item = { rule: `taint:${raw}`, sink: raw, sourceIds: [...used].sort(), ...anchor(node), resolution: 'heuristic_dataflow', analysisContext: unit.id };
                                findings.set(stable([unit.id, path, node.pos, raw, [...used].sort()]), item);
                            }
                        }
                        if (policy.sanitizers.includes(raw) || policy.sanitizers.includes(callee))
                            return new Set();
                        unknownCalls++;
                        return new Set(args.flatMap(x => [...x]));
                    }
                    if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
                        const values = evaluate(node.right, env), target = ts.isIdentifier(node.left) ? symbol(node.left) : undefined;
                        if (target)
                            env.set(target, new Set(values));
                        return values;
                    }
                    const result = new Set<string>();
                    ts.forEachChild(node, n => { for (const v of evaluate(n, env))
                        result.add(v); });
                    return result;
                };
                const flow = (node: ts.Node | undefined, env: Env): Env => {
                    if (!node)
                        return env;
                    deadline.check();
                    if (ts.isBlock(node) || ts.isSourceFile(node)) {
                        for (const n of node.statements)
                            env = flow(n, env);
                        return env;
                    }
                    if (ts.isFunctionLike(node))
                        return env;
                    if (ts.isVariableStatement(node)) {
                        for (const d of node.declarationList.declarations) {
                            const v = evaluate(d.initializer, env);
                            if (ts.isIdentifier(d.name)) {
                                const sym = symbol(d.name);
                                if (sym)
                                    env.set(sym, v);
                            }
                            else {
                                const bind = (n: ts.Node) => { if (ts.isIdentifier(n)) {
                                    const sym = symbol(n);
                                    if (sym)
                                        env.set(sym, new Set(v));
                                }
                                else
                                    ts.forEachChild(n, bind); };
                                bind(d.name);
                            }
                        }
                        return env;
                    }
                    if (ts.isIfStatement(node)) {
                        evaluate(node.expression, env);
                        return merge(flow(node.thenStatement, merge(env, new Map())), flow(node.elseStatement, merge(env, new Map())));
                    }
                    if (ts.isWhileStatement(node) || ts.isDoStatement(node) || ts.isForStatement(node) || ts.isForOfStatement(node) || ts.isForInStatement(node)) {
                        let current = env, stableState = false;
                        const max = Math.min(Math.max(policy.maxIterations ?? 32, 1), 64);
                        let i = 0;
                        const fingerprint = (state: Env) => stable([...state].map(([sym, v]) => [sym.name, sym.declarations?.[0]?.pos, [...v].sort()]).sort());
                        if ('initializer' in node && node.initializer)
                            evaluate(node.initializer, current);
                        for (; i < max; i++) {
                            if ('expression' in node)
                                evaluate(node.expression, current);
                            const after = merge(current, flow(node.statement, merge(current, new Map())));
                            if ('incrementor' in node)
                                evaluate(node.incrementor, after);
                            if (fingerprint(after) === fingerprint(current)) {
                                stableState = true;
                                current = after;
                                break;
                            }
                            current = after;
                        }
                        if (!stableState)
                            boundedOut = true;
                        return current;
                    }
                    if (ts.isTryStatement(node)) {
                        let combined = flow(node.tryBlock, merge(env, new Map()));
                        if (node.catchClause)
                            combined = merge(combined, flow(node.catchClause.block, merge(env, new Map())));
                        return flow(node.finallyBlock, combined);
                    }
                    evaluate(node, env);
                    return env;
                };
                const top = flow(sf, new Map());
                const functions = (n: ts.Node) => { if (ts.isFunctionLike(n) && 'body' in n && n.body) {
                    flow(n.body as ts.Node, merge(top, new Map()));
                } ts.forEachChild(n, functions); };
                functions(sf);
            }
        }
        const report = { snapshotId: snapshot.id, provider: `ci-intraprocedural-taint@0.3.0/ts-${ts.version}`, findings: [...findings.values()], sources: [...sources.values()], unknownCalls, boundedOut, status: boundedOut ? 'incomplete' : 'analyzed', exhaustive: false, assumptions: policy, limitations: ['Intraprocedural flow over configured TS/JS programs. Heap aliasing, dynamic property names, interprocedural side effects, generated runtime behavior and reflective execution are not fully modeled.', 'Source/sink selectors and sanitizers are explicit host assumptions. A finding is not a proven vulnerability; zero findings is not a security certificate.'] };
        return { ...report, artifactId: engine.workspace.store.artifact(snapshot.id, { kind: 'taint_analysis', ...report }) };
    }
    finally {
        project.close();
    }
}
