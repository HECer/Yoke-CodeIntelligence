#!/usr/bin/env node
import { parseArgs } from 'node:util';
import { readFileSync, writeFileSync, watch as watchFs } from 'node:fs';
import { resolve } from 'node:path';
import { CodeIntelligence } from './engine.js';
import { serveMcp } from './protocol/mcp.js';
import { runBridge } from './adapters/bridges.js';
import { runLsp } from './adapters/lsp-command.js';
import { serveViewer } from './protocol/viewer.js';
import { Deadline, assert, object } from './core/util.js';
import { VERSION } from './core/types.js';
const HELP = `Code Intelligence ${VERSION} — local, evidence-first code intelligence

Usage: code-intelligence <command> [arguments] --root <project> [options]

  index                         Capture saved files and build the local index
  status                        Capabilities, coverage and stored snapshots
  find <query>                  Search symbols; --kind text|file|concept|all
  read <path>                   Exact source; --start-line / --end-line
  trace <entity-id>              Directed edges; --direction in|out|both
  context <question>            Compact source-first context
  analyze                       Graph components, centrality and import cycles
  tool <ci_tool> --json <json>   Invoke any of the eight MCP tools directly
  serve                         MCP stdio server (stdout is protocol only)
  view                          Read-only local explorer; prints authenticated URL
  watch                         Re-index on debounced source changes
  approve <plan-id>              Trusted, out-of-band workspace-write approval
  apply <plan-id>                Isolated copy by default; --mode workspace writes
  verify [plan-id]               Static verification; --profile <name> optional
  recover                       Recover an interrupted workspace transaction
  import <graft|graphify>        Import --input <JSON> bound to --snapshot
  bridge <graft|graphify>        Run separately installed, pinned upstream provider
  lsp <operation> <path>        External definition|references|implementation|rename
                               Requires --command, --byte and --trust-providers
  remember --input <JSON>        Store a source-bound memory (untrusted data)
  vectors --input <JSON>         Import precomputed vectors; no model bundled
  coverage --input <LCOV>        Import LCOV with --hashes <source-hashes.json>
  runtime --input <JSON>         Import source-hash-bound execution observations
  sarif --input <JSON>           Import SARIF with --hashes <source-hashes.json>
  export                        Write the canonical graph to --output <file>
  doctor                        Show environment and installed-provider diagnostics

Options:
  --root <path>                 Default: current directory
  --snapshot <id>               Reuse exact immutable source state
  --json <json> | --input <file> JSON arguments for tool/import/remember
  --max-bytes <n>               Result envelope limit, minimum 1024
  --timeout <milliseconds>      One shared request deadline
  --allow-writes                Host permission for workspace integration
  --trust-project               Permit configured verification commands
  --trust-providers             Permit separately installed upstream processes
  --python <executable>         Explicit host-selected Python interpreter
  --allow-test-changes          Explicit approval for a plan changing test files
  --allow-unverified            Explicit approval overriding static verification
  --key <key>                   Idempotency key for apply
  --command <executable>        Host-selected language server; no shell expansion
  --arg=<argument>              Repeatable server argument, e.g. --arg=--stdio
  --byte <offset>               Exact UTF-8 source offset for an LSP operation
  --new-name <name>             LSP rename prepares a plan; does not apply it
  --analysis-context <config>   Restrict trace to a config path; repeatable
  --port <port>                 Explorer port, default 0 (ephemeral)

No model/API key is required. Repository text is data, never permission.
`;
const numeric = (v: unknown, fallback: number): number => {
    if (v === undefined)
        return fallback;
    const n = Number(v);
    assert(Number.isSafeInteger(n) && n >= 0, 'INVALID_ARGUMENT', 'Expected a nonnegative integer');
    return n;
};
const print = (value: unknown) => process.stdout.write(JSON.stringify(value, null, 2) + '\n');
export async function main(argv = process.argv.slice(2)): Promise<void> {
    const { values: o, positionals: p } = parseArgs({ args: argv, allowPositionals: true, options: {
            command: { type: 'string' }, arg: { type: 'string', multiple: true }, byte: { type: 'string' }, 'new-name': { type: 'string' }, 'analysis-context': { type: 'string', multiple: true },
            python: { type: 'string' }, root: { type: 'string' }, snapshot: { type: 'string' }, json: { type: 'string' }, input: { type: 'string' },
            output: { type: 'string' }, hashes: { type: 'string' }, kind: { type: 'string' }, path: { type: 'string' },
            mode: { type: 'string' }, direction: { type: 'string' }, depth: { type: 'string' }, limit: { type: 'string' },
            'start-line': { type: 'string' }, 'end-line': { type: 'string' }, 'max-bytes': { type: 'string' },
            timeout: { type: 'string' }, key: { type: 'string' }, profile: { type: 'string', multiple: true }, port: { type: 'string' },
            'allow-writes': { type: 'boolean' }, 'trust-project': { type: 'boolean' }, 'trust-providers': { type: 'boolean' },
            'allow-test-changes': { type: 'boolean' }, 'allow-unverified': { type: 'boolean' },
            help: { type: 'boolean', short: 'h' }, version: { type: 'boolean', short: 'v' }
        } });
    if (o.version) {
        process.stdout.write(VERSION + '\n');
        return;
    }
    const cmd = p[0];
    if (o.help || !cmd || cmd === 'help') {
        process.stdout.write(HELP);
        return;
    }
    const engine = new CodeIntelligence(resolve(o.root ?? '.'), {
        allowWrites: o['allow-writes'], trustProject: o['trust-project'],
        config: { ...(o.python ? { python: o.python } : {}), ...(o.timeout ? { timeoutMs: numeric(o.timeout, 15000) } : {}) }
    });
    let persistent = false;
    const input = (): Record<string, unknown> => {
        assert(!(o.json && o.input), 'INVALID_ARGUMENT', 'Choose --json or --input, not both');
        const data = o.json ?? (o.input ? readFileSync(resolve(o.input), 'utf8') : '{}');
        assert(Buffer.byteLength(data) <= 32000000, 'CAPACITY_EXCEEDED', 'Input exceeds 32 MB');
        return object(JSON.parse(data));
    };
    const common = { ...(o.snapshot ? { snapshotId: o.snapshot } : {}),
        ...(o.timeout ? { timeoutMs: numeric(o.timeout, 15000) } : {}),
        ...(o['max-bytes'] ? { maxBytes: numeric(o['max-bytes'], 32000) } : {}) };
    const invoke = async (tool: string, args: Record<string, unknown> = {}) => {
        const response = await engine.invoke(tool, { ...common, ...args });
        print(response);
        if (response.status === 'error' || ['failed', 'incomplete', 'conflict', 'rolled_back', 'recovery_required'].includes(String(response.data.status)))
            process.exitCode = 1;
        return response;
    };
    const snapshot = () => engine.workspace.snapshot(o.snapshot, new Deadline(numeric(o.timeout, 120000)));
    const requireInput = () => { assert(o.input, 'INVALID_ARGUMENT', '--input is required'); return readFileSync(resolve(o.input), 'utf8'); };
    try {
        switch (cmd) {
            case 'index':
                await invoke('ci_status', { mode: 'index' });
                break;
            case 'status':
                await invoke('ci_status');
                break;
            case 'analyze':
                await invoke('ci_status', { mode: 'analyze' });
                break;
            case 'find':
                await invoke('ci_find', { query: p.slice(1).join(' '), ...(o.kind ? { kind: o.kind } : {}), ...(o.path ? { path: o.path } : {}), limit: numeric(o.limit, 20) });
                break;
            case 'read':
                assert(p[1], 'INVALID_ARGUMENT', 'Source path required');
                await invoke('ci_read', { path: p[1], ...(o['start-line'] ? { startLine: numeric(o['start-line'], 1) } : {}), ...(o['end-line'] ? { endLine: numeric(o['end-line'], 1) } : {}) });
                break;
            case 'trace':
                assert(p[1], 'INVALID_ARGUMENT', 'Entity ID required');
                await invoke('ci_trace', { entityIds: [p[1]], direction: o.direction ?? 'out', depth: numeric(o.depth, 2), ...(o['analysis-context'] ? { analysisContexts: o['analysis-context'] } : {}) });
                break;
            case 'context':
                await invoke('ci_context', { query: p.slice(1).join(' '), limit: numeric(o.limit, 8) });
                break;
            case 'tool':
                assert(p[1], 'INVALID_ARGUMENT', 'Tool name required');
                await invoke(p[1], input());
                break;
            case 'approve':
                assert(p[1], 'INVALID_ARGUMENT', 'Plan ID required');
                engine.changes.approve(p[1], !!o['allow-test-changes'], !!o['allow-unverified']);
                print({ approved: true, planId: p[1] });
                break;
            case 'apply':
                assert(p[1], 'INVALID_ARGUMENT', 'Plan ID required');
                await invoke('ci_change_apply', { planId: p[1], idempotencyKey: o.key ?? `cli-${p[1].slice(0, 24)}`, mode: o.mode ?? 'isolated' });
                break;
            case 'verify':
                await invoke('ci_verify', { ...(p[1] ? { planId: p[1] } : {}), ...(o.profile ? { profiles: o.profile } : {}) });
                break;
            case 'recover':
                print(engine.changes.recover());
                break;
            case 'import': {
                assert(p[1] === 'graft' || p[1] === 'graphify', 'INVALID_ARGUMENT', 'Expected graft or graphify');
                const s = snapshot();
                await engine.import(p[1], s.id, input());
                print({ imported: true, backend: p[1], snapshotId: s.id });
                break;
            }
            case 'bridge': {
                assert(p[1] === 'graft' || p[1] === 'graphify', 'INVALID_ARGUMENT', 'Expected graft or graphify');
                const s = snapshot();
                print(await runBridge(engine, p[1], s.id, { trusted: !!o['trust-providers'], timeoutMs: numeric(o.timeout, 120000) }));
                break;
            }
            case 'lsp': {
                assert(p[1] === 'definition' || p[1] === 'references' || p[1] === 'implementation' || p[1] === 'rename', 'INVALID_ARGUMENT', 'Expected definition, references, implementation or rename');
                assert(p[2] && p.length === 3 && o.command && o.byte !== undefined, 'INVALID_ARGUMENT', 'LSP requires a path, --command and --byte');
                print(await runLsp(engine, p[1], p[2], { ...common, trusted: !!o['trust-providers'], command: o.command, args: o.arg, byte: numeric(o.byte, 0), newName: o['new-name'] }));
                break;
            }
            case 'remember': {
                const s = snapshot();
                const data = input();
                assert(typeof data.text === 'string' && Array.isArray(data.paths) && data.paths.every(x => typeof x === 'string'), 'INVALID_ARGUMENT', 'Memory requires text and paths');
                print(engine.knowledge.remember(s, { kind: data.kind as any, text: data.text, paths: data.paths as string[], actor: typeof data.actor === 'string' ? data.actor : 'local-cli', ...(typeof data.supersedes === 'string' ? { supersedes: data.supersedes } : {}) }));
                break;
            }
            case 'vectors': {
                const s = snapshot();
                await engine.indexer.ensure(s, new Deadline(numeric(o.timeout, 120000)));
                print(engine.knowledge.importVectors(s, input()));
                break;
            }
            case 'runtime':
                print(engine.knowledge.importRuntime(snapshot(), input()));
                break;
            case 'coverage':
            case 'sarif': {
                assert(o.hashes, 'INVALID_ARGUMENT', '--hashes file is required');
                const hashes = object(JSON.parse(readFileSync(resolve(o.hashes), 'utf8'))) as Record<string, string>;
                print(cmd === 'coverage' ? engine.knowledge.importLcov(snapshot(), requireInput(), hashes) : engine.knowledge.importSarif(snapshot(), input(), hashes));
                break;
            }
            case 'export': {
                assert(o.output, 'INVALID_ARGUMENT', '--output is required');
                const s = snapshot();
                await engine.indexer.ensure(s, new Deadline(numeric(o.timeout, 120000)));
                const g = engine.workspace.store.graph(s.id);
                writeFileSync(resolve(o.output), JSON.stringify(g, null, 2));
                print({ snapshotId: s.id, output: resolve(o.output), nodes: g.entities.length, edges: g.observations.length });
                break;
            }
            case 'doctor':
                print({ node: process.version, platform: process.platform, architecture: process.arch, runtime: 'Node built-in SQLite (experimental warning on Node 22)', defaultNetworkCalls: false, providerBridges: 'opt-in, separately installed, not network-sandboxed', workspace: engine.workspace.root });
                await invoke('ci_status');
                break;
            case 'serve': {
                persistent = true;
                const stop = serveMcp(engine);
                const cleanup = () => { stop(); engine.close(); };
                process.once('SIGINT', () => { cleanup(); process.exit(0); });
                process.once('SIGTERM', () => { cleanup(); process.exit(0); });
                // Allow already accepted requests to complete before stdio EOF closes the process.
                process.stdin.once('end', () => { process.exitCode ??= 0; });
                break;
            }
            case 'view': {
                persistent = true;
                const server = await serveViewer(engine, numeric(o.port, 0));
                print({ url: server.url, readOnly: true, bind: '127.0.0.1' });
                const cleanup = () => { server.close(); engine.close(); process.exit(0); };
                process.once('SIGINT', cleanup);
                process.once('SIGTERM', cleanup);
                break;
            }
            case 'watch': {
                persistent = true;
                let timer: ReturnType<typeof setTimeout> | undefined, busy = false, again = false;
                const update = async () => {
                    if (busy) {
                        again = true;
                        return;
                    }
                    busy = true;
                    try {
                        await invoke('ci_status', { mode: 'index' });
                    }
                    finally {
                        busy = false;
                        if (again) {
                            again = false;
                            void update();
                        }
                    }
                };
                await update();
                const watcher = watchFs(engine.workspace.root, { recursive: true }, (_event, filename) => {
                    if (!filename || /(^|[\\/])(?:\.code-intelligence|node_modules|\.git|dist|__pycache__)([\\/]|$)/.test(filename.toString()))
                        return;
                    if (timer)
                        clearTimeout(timer);
                    timer = setTimeout(() => void update(), 300);
                });
                const cleanup = () => {
                    if (timer)
                        clearTimeout(timer);
                    watcher.close();
                    engine.close();
                    process.exit(0);
                };
                process.once('SIGINT', cleanup);
                process.once('SIGTERM', cleanup);
                break;
            }
            default: throw new Error(`Unknown command: ${cmd}. Use --help.`);
        }
    }
    finally {
        if (!persistent)
            engine.close();
    }
}
main().catch(error => { process.stderr.write(JSON.stringify({ error: { code: error.code ?? 'CLI_ERROR', message: error.message } }) + '\n'); process.exitCode = 1; });
