import { posix } from 'node:path';
import { TextIndex } from '../core/util.js';
import { normalizePath } from '../core/paths.js';
import type { Extraction, LocalEntity, LocalObservation, Snapshot } from '../core/types.js';
const PRODUCER = 'ci-contracts@0.1.0';
/** Deterministic document/contracts extraction. All string-based links stay heuristic. */
export function extractDocuments(snapshot: Snapshot, contents: Map<string, string>, scopes: Record<string, string>): Extraction {
    const result: Extraction = { entities: [], observations: [], diagnostics: [], limitations: [] };
    const paths = new Set(contents.keys());
    const routes: LocalEntity[] = [], contracts: LocalEntity[] = [];
    const scope = (path: string) => Object.entries(scopes).sort(([a], [b]) => b.length - a.length).find(([prefix]) => path === prefix || path.startsWith(prefix + '/'))?.[1] ?? 'workspace';
    for (const file of snapshot.files) {
        const source = contents.get(file.path)!, index = new TextIndex(source), fileKey = `file:${file.path}`;
        const add = (name: string, kind: string, start: number, end: number, signature: string, metadata?: Record<string, unknown>): LocalEntity => {
            const entity: LocalEntity = { key: `${file.path}#${start}:${kind}:${name}`, path: file.path, name, qualifiedName: name, kind, language: file.language, signature: signature.slice(0, 700), producer: PRODUCER, parentKey: fileKey, metadata: { ...metadata, service: scope(file.path) }, ...index.range(start, end) };
            result.entities.push(entity);
            result.observations.push({ sourceKey: fileKey, targetKey: entity.key, relation: 'contains', resolution: 'syntactic', path: file.path, producer: PRODUCER, ...index.range(start, end) });
            return entity;
        };
        if (file.language === 'document') {
            const headings = [...source.matchAll(/^#{1,6}\s+(.+)$/gm)];
            for (let n = 0; n < headings.length; n++) {
                const h = headings[n]!;
                add(h[1]!, 'concept', h.index!, headings[n + 1]?.index ?? source.length, source.slice(h.index!, Math.min(headings[n + 1]?.index ?? source.length, h.index! + 700)));
            }
            for (const link of source.matchAll(/\[[^\]]+\]\(([^)\s]+)\)/g)) {
                const raw = link[1]!.split('#')[0]!;
                if (!raw || /^[a-z]+:|^[/\\]/i.test(raw))
                    continue;
                let target: string;
                try {
                    target = normalizePath(posix.normalize(posix.join(posix.dirname(file.path), decodeURIComponent(raw))));
                }
                catch {
                    continue;
                }
                if (paths.has(target))
                    result.observations.push({ sourceKey: fileKey, targetKey: `file:${target}`, relation: 'documents', resolution: 'authored_contract', path: file.path, producer: PRODUCER, ...index.range(link.index!, link.index! + link[0].length) });
            }
        }
        if (file.language === 'document' || /\.(ts|tsx|js|jsx|py|go|rs|java|cs)$/.test(file.path)) {
            for (const note of source.matchAll(/(?:\/\/|#)\s*(?:NOTE|WHY|ADR):\s*([^\n]+)/g))
                add(note[1]!.slice(0, 100), 'rationale', note.index!, note.index! + note[0].length, note[1]!);
            for (const env of source.matchAll(/\bprocess\.env\.([A-Za-z_][A-Za-z0-9_]*)|\b(?:getenv|environ\.get)\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]/g))
                add(env[1] ?? env[2]!, 'configuration_key', env.index!, env.index! + env[0].length, 'Environment variable name only; no value stored.');
            for (const route of source.matchAll(/(?:\b(?:app|router)\.(get|post|put|patch|delete|options|head)|@(?:app|router)\.(get|post|put|patch|delete|options|head))\(\s*['"]([^'"\n]+)['"]/g)) {
                const method = (route[1] ?? route[2]!).toUpperCase(), url = route[3]!;
                routes.push(add(`${method} ${url}`, 'route', route.index!, route.index! + route[0].length, route[0], { method, url, extraction: 'pattern' }));
            }
            for (const event of source.matchAll(/\.(emit|publish|subscribe|on)\(\s*['"]([^'"\n]+)['"]/g))
                add(event[2]!, /emit|publish/.test(event[1]!) ? 'event_publisher' : 'event_subscriber', event.index!, event.index! + event[0].length, event[0], { topic: event[2], extraction: 'pattern' });
        }
        if (file.language === 'sql') {
            for (const table of source.matchAll(/\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z_][A-Za-z0-9_.]*)/gi))
                add(table[1]!, 'database_table', table.index!, table.index! + table[0].length, table[0], { extraction: 'pattern' });
            for (const access of source.matchAll(/\b(FROM|JOIN|INTO|UPDATE|ALTER\s+TABLE)\s+([A-Za-z_][A-Za-z0-9_.]*)/gi))
                add(access[2]!, 'database_access', access.index!, access.index! + access[0].length, access[0], { action: access[1], extraction: 'pattern' });
        }
        if (/\.json$/.test(file.path)) {
            try {
                const json = JSON.parse(source) as Record<string, unknown>;
                if ((json.openapi || json.swagger) && json.paths && typeof json.paths === 'object') {
                    for (const [url, methods] of Object.entries(json.paths))
                        if (methods && typeof methods === 'object') {
                            for (const [method, detail] of Object.entries(methods))
                                if (/^(get|post|put|patch|delete|head|options)$/i.test(method)) {
                                    const at = source.indexOf(JSON.stringify(url));
                                    contracts.push(add(`${method.toUpperCase()} ${url}`, 'api_contract', Math.max(0, at), Math.max(0, at) + JSON.stringify(url).length, JSON.stringify(detail).slice(0, 700), { method: method.toUpperCase(), url, anchor: 'file', operation: detail }));
                                }
                        }
                }
                if (json.$schema && json.properties && typeof json.properties === 'object')
                    for (const name of Object.keys(json.properties)) {
                        const at = Math.max(0, source.indexOf(JSON.stringify(name)));
                        add(name, 'schema_field', at, at + JSON.stringify(name).length, JSON.stringify((json.properties as Record<string, unknown>)[name]).slice(0, 300));
                    }
            }
            catch { /* Invalid JSON remains searchable text. Compiler syntax checking is separate. */ }
        }
        if (/\.(ya?ml)$/.test(file.path) && /(?:openapi|swagger)\s*:/.test(source))
            result.limitations.push(`OpenAPI YAML ${file.path} is text-only in the built-in extractor; import a JSON OpenAPI artifact or use Graphify.`);
    }
    const connect = (source: LocalEntity, target: LocalEntity, relation: string): void => { result.observations.push({ sourceKey: source.key, targetKey: target.key, relation, resolution: 'heuristic', path: source.path, producer: PRODUCER, startByte: source.startByte, endByte: source.endByte, startLine: source.startLine, endLine: source.endLine }); };
    for (const route of routes) {
        const candidates = contracts.filter(c => c.name === route.name && c.metadata?.service === route.metadata?.service);
        if (candidates.length === 1)
            connect(route, candidates[0]!, 'implements_contract');
    }
    for (const item of result.entities) {
        if (item.kind === 'database_access')
            for (const target of result.entities.filter(t => t.kind === 'database_table' && t.name === item.name && t.metadata?.service === item.metadata?.service))
                connect(item, target, 'accesses_table');
        if (item.kind === 'event_publisher')
            for (const target of result.entities.filter(t => t.kind === 'event_subscriber' && t.name === item.name && t.metadata?.service === item.metadata?.service))
                connect(item, target, 'possible_event_delivery');
    }
    if (routes.length)
        result.limitations.push('Framework routes/events and SQL links use bounded pattern extraction; decorators, aliases, routers and schema resolution may be incomplete. Contract matches are candidates, not verified network calls.');
    return result;
}
