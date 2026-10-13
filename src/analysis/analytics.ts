import type { Entity, Observation, Snapshot } from '../core/types.js';
import { Workspace } from '../core/workspace.js';
import { Deadline } from '../core/util.js';
/** Deterministic projections; directed observations remain untouched in the store. */
export function analyzeGraph(workspace: Workspace, snapshot: Snapshot, deadline: Deadline): Record<string, unknown> {
    const entities = workspace.store.entities(snapshot.id);
    const edges = workspace.store.observations(snapshot.id);
    const outgoing = new Map<string, Set<string>>(), incoming = new Map<string, Set<string>>();
    for (const e of edges)
        if (e.target && e.relation !== 'contains') {
            const out = outgoing.get(e.source) ?? new Set();
            out.add(e.target);
            outgoing.set(e.source, out);
            const ins = incoming.get(e.target) ?? new Set();
            ins.add(e.source);
            incoming.set(e.target, ins);
        }
    const central = entities.map(e => ({ id: e.id, name: e.name, path: e.path, inDegree: incoming.get(e.id)?.size ?? 0, outDegree: outgoing.get(e.id)?.size ?? 0 })).sort((a, b) => (b.inDegree + b.outDegree) - (a.inDegree + a.outDegree)).slice(0, 30);
    // Weakly connected components are explicitly NOT advertised as Leiden communities.
    const unseen = new Set(entities.filter(e => e.kind !== 'file').map(e => e.id));
    const communities: {
        ids: string[];
        label: string;
    }[] = [];
    const byId = new Map(entities.map(e => [e.id, e]));
    while (unseen.size) {
        deadline.check();
        const first = unseen.values().next().value!;
        const queue = [first], component: string[] = [];
        unseen.delete(first);
        while (queue.length) {
            const id = queue.pop()!;
            component.push(id);
            for (const next of [...outgoing.get(id) ?? [], ...incoming.get(id) ?? []])
                if (unseen.delete(next))
                    queue.push(next);
        }
        communities.push({ ids: component, label: byId.get(first)?.path.split('/')[0] ?? 'component' });
    }
    const importEdges = edges.filter(e => e.relation === 'imports' && e.target);
    const graph = new Map<string, string[]>();
    for (const e of importEdges) {
        const list = graph.get(e.source) ?? [];
        list.push(e.target!);
        graph.set(e.source, list);
    }
    const cycles = stronglyConnected([...new Set(importEdges.flatMap(e => [e.source, e.target!]))], graph, deadline).filter(c => c.length > 1 || graph.get(c[0]!)?.includes(c[0]!));
    return { items: central, communities: communities.sort((a, b) => b.ids.length - a.ids.length).slice(0, 50), communityAlgorithm: 'weakly_connected_components_noncontainment_projection', importCycles: cycles.map(c => c.map(id => ({ id, path: byId.get(id)?.path }))), entityCount: entities.length, observationCount: edges.length, unresolved: edges.filter(e => !e.target).length };
}
function stronglyConnected(nodes: string[], edges: Map<string, string[]>, deadline: Deadline): string[][] {
    // Iterative Kosaraju avoids call-stack overflow on long repository chains.
    const seen = new Set<string>(), order: string[] = [];
    const rev = new Map<string, string[]>();
    for (const [a, bs] of edges)
        for (const b of bs) {
            const list = rev.get(b) ?? [];
            list.push(a);
            rev.set(b, list);
        }
    for (const root of nodes) {
        if (seen.has(root))
            continue;
        const stack: {
            id: string;
            exit: boolean;
        }[] = [{ id: root, exit: false }];
        while (stack.length) {
            deadline.check();
            const frame = stack.pop()!;
            if (frame.exit) {
                order.push(frame.id);
                continue;
            }
            if (seen.has(frame.id))
                continue;
            seen.add(frame.id);
            stack.push({ id: frame.id, exit: true });
            for (const next of edges.get(frame.id) ?? [])
                if (!seen.has(next))
                    stack.push({ id: next, exit: false });
        }
    }
    seen.clear();
    const result: string[][] = [];
    for (const root of order.reverse()) {
        if (seen.has(root))
            continue;
        const queue = [root], component: string[] = [];
        seen.add(root);
        while (queue.length) {
            const id = queue.pop()!;
            component.push(id);
            for (const next of rev.get(id) ?? [])
                if (!seen.has(next)) {
                    seen.add(next);
                    queue.push(next);
                }
        }
        result.push(component);
    }
    return result;
}
export function graphDiff(before: Entity[], after: Entity[], oldEdges: Observation[], newEdges: Observation[]): Record<string, unknown> {
    const a = new Map(before.map(e => [e.key, e])), b = new Map(after.map(e => [e.key, e]));
    return { added: [...b.keys()].filter(k => !a.has(k)), removed: [...a.keys()].filter(k => !b.has(k)), changed: [...b.keys()].filter(k => a.has(k) && JSON.stringify({ ...a.get(k), id: undefined, snapshotId: undefined }) !== JSON.stringify({ ...b.get(k), id: undefined, snapshotId: undefined })), observationsBefore: oldEdges.length, observationsAfter: newEdges.length, lineage: 'occurrence_keys_only_no_inferred_rename_identity' };
}
