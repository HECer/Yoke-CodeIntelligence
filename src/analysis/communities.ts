import { fileURLToPath } from 'node:url';
import { CodeIntelligence } from '../engine.js';
import { Snapshot } from '../core/types.js';
import { Deadline, assert, object } from '../core/util.js';
import { runProcess } from '../core/process.js';
export async function detectCommunities(engine: CodeIntelligence, s: Snapshot, algorithm: 'leiden' | 'louvain', deadline: Deadline): Promise<Record<string, unknown>> {
    const graph = engine.workspace.store.graph(s.id);
    assert(graph, 'INDEX_REQUIRED', 'Build index first');
    const nodes = graph.entities.map(e => e.id), valid = new Set(nodes);
    const out = await runProcess(engine.workspace.config.python, ['-I', fileURLToPath(new URL('../../workers/community.py', import.meta.url))], { cwd: engine.workspace.root, deadline, maxBytes: 16000000, input: JSON.stringify({ nodes, edges: graph.observations.filter(e => e.target).map(e => [e.source, e.target]), algorithm, seed: 0 }) });
    assert(out.exitCode === 0 && !out.truncated, 'COMMUNITY_PROVIDER_UNAVAILABLE', out.stdout.slice(0, 1000) || out.stderr.slice(0, 1000));
    const result = object(JSON.parse(out.stdout));
    assert(result.algorithm === algorithm && Array.isArray(result.communities), 'ADAPTER_CONTRACT', 'Unexpected community result');
    const flat = result.communities.flat();
    assert(flat.length === nodes.length && new Set(flat).size === nodes.length && flat.every(x => valid.has(x)), 'ADAPTER_CONTRACT', 'Communities must partition all nodes exactly once');
    return { ...result, snapshotId: s.id, artifactId: engine.workspace.store.artifact(s.id, { kind: 'communities', snapshotId: s.id, ...result }) };
}
