/** Host-facing commands kept separate from the eight compact agent tools. */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { CodeIntelligence } from './engine.js';
import { Deadline, assert, object, text } from './core/util.js';
import { serveEditor } from './protocol/editor-server.js';
import { buildTypeScript } from './analysis/build.js';
import { runSemantic } from './adapters/semantic.js';
import { runSerena } from './adapters/serena-command.js';
import { collectGarbage, pinSnapshot, unpinSnapshot } from './store/maintenance.js';
import { exportSnapshot, importSnapshot, type SyncIdentity } from './federation/snapshot-sync.js';
import { serveFederation, federationRequest, type FederationProject, type FederationGrant } from './federation/server.js';
import { generateEmbeddings, generateLocalVectors, semanticSearch, searchLocalVectors, rerank, summarizeSources, type ModelProvider } from './analysis/enrichment.js';
import { detectCommunities } from './analysis/communities.js';
import { analyzeTaint, type TaintPolicy } from './analysis/taint.js';
import { collectNodeTestCoverage, importTestCoverage, selectTests } from './analysis/test-map.js';
import { ingestMedia } from './analysis/media.js';
import { diffOpenApi } from './analysis/contracts.js';
import type { Snapshot } from './core/types.js';
export const ADVANCED_COMMANDS = new Set(['editor', 'build', 'semantic', 'serena', 'gc', 'pin', 'unpin', 'federation', 'sync', 'embed', 'semantic-search', 'rerank', 'summarize', 'communities', 'taint', 'coverage-run', 'coverage-tests', 'test-select', 'media', 'contract-diff']);
type Options = Record<string, string | string[] | boolean | undefined>;
const n = (v: unknown, otherwise: number) => { const value = v === undefined ? otherwise : Number(v); assert(Number.isSafeInteger(value) && value >= 0, 'INVALID_ARGUMENT', 'Expected a nonnegative integer'); return value; };
const s = (v: unknown, name: string, max = 4096) => text(v, name, max);
const jsonFile = (path: unknown) => { const data = readFileSync(resolve(s(path, 'host-config')), 'utf8'); assert(Buffer.byteLength(data) <= 64000000, 'CAPACITY_EXCEEDED', 'Configuration exceeds 64 MB'); return object(JSON.parse(data)); };
const secretEnv = (name: unknown) => { const key = s(name, 'environment variable', 200); assert(/^[A-Za-z_][A-Za-z0-9_]*$/.test(key), 'INVALID_ARGUMENT', 'Invalid environment variable name'); return s(process.env[key], `environment variable ${key}`, 64000); };
function strings(value: unknown, name: string): string[] { assert(Array.isArray(value) && value.every(v => typeof v === 'string'), 'INVALID_ARGUMENT', `${name} must be strings`); return value; }
export async function advancedCommand(cmd: string, p: string[], o: Options, engine: CodeIntelligence, input: () => Record<string, unknown>, print: (value: unknown) => void): Promise<{
    persistent: boolean;
}> {
    const deadline = new Deadline(n(o.timeout, 120000));
    const snapshot = () => engine.workspace.snapshot(typeof o.snapshot === 'string' ? o.snapshot : undefined, deadline);
    const indexed = async () => { const snap = snapshot(); await engine.indexer.ensure(snap, deadline); return snap; };
    const model = (): ModelProvider => { assert(o['trust-providers'] === true, 'PROVIDER_UNTRUSTED', '--trust-providers is required for model processes'); const h = jsonFile(o['host-config']); const env: Record<string, string> = {}; if (h.env)
        for (const [k, v] of Object.entries(object(h.env)))
            env[k] = s(v, `env.${k}`, 16000); if (h.envFrom)
        for (const [k, v] of Object.entries(object(h.envFrom)))
            env[k] = secretEnv(v); return { trusted: true, command: s(h.command, 'command'), args: strings(h.args ?? [], 'args'), modelId: s(h.modelId, 'modelId', 300), maxCost: Number(h.maxCost), taskLimit: Number(h.taskLimit), taskId: s(h.taskId, 'taskId', 300), env }; };
    const stopOnSignal = (close: () => void) => { const cleanup = () => { close(); engine.close(); process.exit(0); }; process.once('SIGINT', cleanup); process.once('SIGTERM', cleanup); };
    switch (cmd) {
        case 'editor': {
            const server = await serveEditor(engine, n(o.port, 0));
            print({ endpoint: server.endpoint, token: server.token, sourceWrites: false });
            stopOnSignal(server.close);
            return { persistent: true };
        }
        case 'build': {
            const result = buildTypeScript(engine.workspace, snapshot(), deadline);
            print(result);
            if (result.status !== 'passed')
                process.exitCode = 1;
            break;
        }
        case 'semantic': {
            assert(p[1] === 'python' || p[1] === 'go', 'INVALID_ARGUMENT', 'Expected python or go');
            assert(['definition', 'references', 'rename'].includes(p[2] ?? ''), 'INVALID_ARGUMENT', 'Expected definition, references or rename');
            assert(o.byte !== undefined, 'INVALID_ARGUMENT', '--byte is required');
            print(await runSemantic(engine, p[1], p[2] as any, s(p[3], 'path'), { trusted: o['trust-providers'] === true, byte: n(o.byte, 0), newName: typeof o['new-name'] === 'string' ? o['new-name'] : undefined, snapshotId: snapshot().id, timeoutMs: deadline.remaining }));
            break;
        }
        case 'serena': {
            assert(['capabilities', 'read', 'prepare'].includes(p[1] ?? ''), 'INVALID_ARGUMENT', 'Expected capabilities, read or prepare');
            const data = input();
            print(await runSerena(engine, p[1] as any, typeof data.tool === 'string' ? data.tool : undefined, object(data.arguments ?? {}), { trusted: o['trust-providers'] === true, command: s(o.command, 'command'), args: strings(o.arg ?? [], 'arg'), snapshotId: snapshot().id, timeoutMs: deadline.remaining, allowArbitraryCode: o['allow-runtime'] === true }));
            break;
        }
        case 'gc': {
            const data = input();
            assert(o.mode === undefined || o.mode === 'dry-run' || o.mode === 'apply', 'INVALID_ARGUMENT', 'GC --mode must be dry-run or apply');
            print(collectGarbage(engine.workspace.store, { keepSnapshots: n(data.keepSnapshots, 5), minimumAgeMs: n(data.minimumAgeMs, 86400000), cacheAgeMs: n(data.cacheAgeMs, 30 * 86400000), apply: o.mode === 'apply' }));
            break;
        }
        case 'pin': {
            const snap = snapshot();
            pinSnapshot(engine.workspace.store, snap.id, s(o.key, 'key', 200), p.slice(1).join(' '));
            print({ pinned: true, snapshotId: snap.id, key: o.key });
            break;
        }
        case 'unpin':
            unpinSnapshot(engine.workspace.store, s(o.key, 'key', 200));
            print({ unpinned: true, key: o.key });
            break;
        case 'federation': {
            const h = jsonFile(o['host-config']), engines: CodeIntelligence[] = [];
            try {
                assert(Array.isArray(h.projects) && Array.isArray(h.grants), 'INVALID_ARGUMENT', 'Host config requires projects and grants');
                const projects: FederationProject[] = h.projects.map(v => { const item = object(v); assert(typeof item.root === 'string', 'INVALID_ARGUMENT', 'Project root required'); const child = new CodeIntelligence(resolve(item.root), { allowWrites: false, trustProject: false }); engines.push(child); const scope = item.sync ? object(item.sync) : null; return { engine: child, tenantId: s(item.tenantId, 'tenantId'), projectId: s(item.projectId, 'projectId'), ...(scope ? { sync: { keyId: s(scope.keyId, 'keyId'), ...(scope.publicKeyFile ? { publicKey: readFileSync(resolve(s(scope.publicKeyFile, 'publicKeyFile')), 'utf8') } : {}), ...(scope.privateKeyFile ? { privateKey: readFileSync(resolve(s(scope.privateKeyFile, 'privateKeyFile')), 'utf8') } : {}) } } : {}) }; });
                const grants: FederationGrant[] = h.grants.map(v => { const grant = object(v); return { token: secretEnv(grant.tokenEnv), tenantId: s(grant.tenantId, 'tenantId'), projects: strings(grant.projects, 'projects'), permissions: strings(grant.permissions, 'permissions') as FederationGrant['permissions'] }; });
                const tls = h.tls ? object(h.tls) : null;
                const server = await serveFederation({ projects, grants, host: typeof h.host === 'string' ? h.host : undefined, port: n(o.port ?? h.port, 0), allowedHosts: h.allowedHosts ? strings(h.allowedHosts, 'allowedHosts') : undefined, ...(tls ? { tls: { cert: readFileSync(resolve(s(tls.certFile, 'certFile')), 'utf8'), key: readFileSync(resolve(s(tls.keyFile, 'keyFile')), 'utf8') } } : {}) });
                print({ endpoint: server.endpoint, sourceWrites: false, projects: projects.map(v => ({ tenantId: v.tenantId, projectId: v.projectId })) });
                stopOnSignal(() => { server.close(); for (const child of engines)
                    child.close(); });
                return { persistent: true };
            }
            catch (e) {
                for (const child of engines)
                    child.close();
                throw e;
            }
        }
        case 'sync': {
            const h = jsonFile(o['host-config']);
            if (p[1] === 'request') {
                print(await federationRequest(s(h.endpoint, 'endpoint'), secretEnv(h.tokenEnv), input(), deadline.remaining));
                break;
            }
            const scope: SyncIdentity = { tenantId: s(h.tenantId, 'tenantId'), projectId: s(h.projectId, 'projectId'), keyId: s(h.keyId, 'keyId') };
            if (p[1] === 'export') {
                assert(o.output, 'INVALID_ARGUMENT', '--output required');
                const data = exportSnapshot(engine.workspace, snapshot(), scope, readFileSync(resolve(s(h.privateKeyFile, 'privateKeyFile')), 'utf8'));
                writeFileSync(resolve(s(o.output, 'output')), JSON.stringify(data), { flag: 'wx', mode: 0o600 });
                print({ exported: true, output: resolve(s(o.output, 'output')), containsSource: true });
            }
            else {
                assert(p[1] === 'import', 'INVALID_ARGUMENT', 'Expected sync export, import or request');
                print(importSnapshot(engine.workspace, input(), scope, readFileSync(resolve(s(h.publicKeyFile, 'publicKeyFile')), 'utf8')));
            }
            break;
        }
        case 'embed': {
            const snap = await indexed();
            print(o['host-config'] ? await generateEmbeddings(engine, snap, model(), deadline) : generateLocalVectors(engine, snap, n(o.dimensions, 512)));
            break;
        }
        case 'semantic-search': {
            const snap = await indexed(), query = p.slice(1).join(' ');
            print(o['host-config'] ? await semanticSearch(engine, snap, query, model(), deadline, n(o.limit, 20)) : searchLocalVectors(engine, snap, s(o.model, 'model'), query, deadline, n(o.limit, 20)));
            break;
        }
        case 'rerank':
        case 'summarize': {
            const snap = await indexed(), data = input(), ids = strings(data.entityIds, 'entityIds');
            print(cmd === 'rerank' ? await rerank(engine, snap, s(data.query, 'query'), ids, model(), deadline) : await summarizeSources(engine, snap, ids, model(), deadline));
            break;
        }
        case 'communities': {
            assert(o.algorithm === 'leiden' || o.algorithm === 'louvain', 'INVALID_ARGUMENT', 'Select --algorithm leiden or louvain');
            print(await detectCommunities(engine, await indexed(), o.algorithm, deadline));
            break;
        }
        case 'taint': {
            const data = input();
            const policy: TaintPolicy = { sources: strings(data.sources ?? ['req.query', 'req.body', 'req.params'], 'sources'), sinks: object(data.sinks ?? { eval: [0], exec: [0], query: [0] }) as Record<string, number[]>, sanitizers: strings(data.sanitizers ?? [], 'sanitizers'), maxIterations: n(data.maxIterations, 8) };
            print(analyzeTaint(engine, snapshot(), policy, deadline));
            break;
        }
        case 'coverage-run': {
            const data = input();
            assert(Array.isArray(data.tests), 'INVALID_ARGUMENT', 'tests array required');
            print(await collectNodeTestCoverage(engine, snapshot(), data.tests as any, o['trust-project'] === true, deadline));
            break;
        }
        case 'coverage-tests':
            print(importTestCoverage(engine, snapshot(), input()));
            break;
        case 'test-select': {
            const data = input();
            print(selectTests(engine, engine.workspace.store.snapshot(s(o.base, 'base')), snapshot(), strings(data.tests, 'tests')));
            break;
        }
        case 'media': {
            const data = input();
            print(await ingestMedia(engine, snapshot(), s(p[1], 'media path'), { ...(typeof data.transcript === 'string' ? { transcript: data.transcript } : {}), ...(o['host-config'] ? { provider: model() } : {}), ...(typeof o['expected-hash'] === 'string' ? { expectedHash: o['expected-hash'] } : {}) }, deadline));
            break;
        }
        case 'contract-diff': {
            const data = input();
            print(diffOpenApi(object(data.before), object(data.after)));
            break;
        }
        default: throw new Error(`Unknown command: ${cmd}`);
    }
    return { persistent: false };
}
