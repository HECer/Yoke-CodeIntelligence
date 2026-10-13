/** Additional evidence-preserving contract extraction. Cross-language links remain candidates. */
import ts from 'typescript';
import { TextIndex, Deadline, assert } from '../core/util.js';
import type { Snapshot, Extraction, LocalEntity, LocalObservation } from '../core/types.js';
const PRODUCER = 'ci-contract-plugins@0.3.0';
export function extractContracts(snapshot: Snapshot, contents: Map<string, string>, scopes: Record<string, string>, deadline: Deadline): Extraction {
    const out: Extraction = { entities: [], observations: [], diagnostics: [], limitations: ['Prisma/SQL/OpenAPI/GraphQL/Protobuf and literal HTTP-call extractors cover explicit declarations. Generated schemas, dynamic SQL, middleware rewrites and service discovery are not inferred as facts.'] };
    const tables: LocalEntity[] = [], accesses: LocalEntity[] = [], http: LocalEntity[] = [], apis: LocalEntity[] = [];
    const scope = (path: string) => Object.entries(scopes).sort(([a], [b]) => b.length - a.length).find(([prefix]) => path === prefix || path.startsWith(prefix + '/'))?.[1] ?? 'workspace';
    for (const file of snapshot.files) {
        deadline.check();
        const source = contents.get(file.path)!, ti = new TextIndex(source);
        const add = (name: string, kind: string, start: number, end: number, metadata: Record<string, unknown> = {}): LocalEntity => { const item: LocalEntity = { key: `contract:${file.path}:${start}:${kind}:${name}`, path: file.path, name, qualifiedName: name, kind, language: file.language, signature: source.slice(start, Math.min(end, start + 800)), producer: PRODUCER, ...ti.range(start, end), metadata: { service: scope(file.path), ...metadata } }; out.entities.push(item); out.observations.push({ sourceKey: `file:${file.path}`, targetKey: item.key, relation: 'declares_contract', resolution: 'syntactic', path: file.path, producer: PRODUCER, ...ti.range(start, end) }); return item; };
        const edge = (from: LocalEntity, to: LocalEntity, relation: string, at: LocalEntity, resolution: LocalObservation['resolution'] = 'heuristic') => out.observations.push({ sourceKey: from.key, targetKey: to.key, relation, resolution, path: at.path, producer: PRODUCER, startByte: at.startByte, endByte: at.endByte, startLine: at.startLine, endLine: at.endLine });
        if (file.path.endsWith('.prisma')) {
            const clean = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, m => m.replace(/[^\n]/g, ' '));
            for (const m of clean.matchAll(/\b(model|enum|type)\s+([A-Za-z_]\w*)\s*\{([^}]*)\}/g)) {
                const start = m.index!, bodyStart = start + m[0].indexOf('{') + 1, body = m[3]!, mapped = /@@map\(\s*"([^"]+)"\s*\)/.exec(body)?.[1], model = add(m[2]!, m[1] === 'model' ? 'orm_model' : 'schema_type', start, start + m[0].length, { name: m[2], databaseName: mapped ?? m[2], format: 'prisma' });
                if (m[1] === 'model')
                    tables.push(model);
                for (const f of body.matchAll(/^\s*([A-Za-z_]\w*)\s+([A-Za-z_]\w*(?:\[\])?\??)([^\n]*)/gm)) {
                    const field = add(`${m[2]}.${f[1]}`, 'schema_field', bodyStart + f.index!, bodyStart + f.index! + f[0].length, { owner: m[2], field: f[1], type: f[2], nullable: f[2]!.endsWith('?'), format: 'prisma', databaseName: /@map\(\s*"([^"]+)"\s*\)/.exec(f[3]!)?.[1] ?? f[1] });
                    edge(model, field, 'has_field', field, 'syntactic');
                    const relation = /@relation\(\s*(?:name:\s*)?"([^"]+)"/.exec(f[3]!);
                    if (relation)
                        field.metadata!.relationName = relation[1];
                }
            }
        }
        if (file.language === 'sql') {
            // Keep text positions while hiding comments so commented migrations do not create tables.
            const clean = source.replace(/--[^\n]*|\/\*[\s\S]*?\*\//g, m => m.replace(/[^\n]/g, ' '));
            for (const m of clean.matchAll(/\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:"([^"\n]+)"|`([^`\n]+)`|([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?))/gi))
                tables.push(add(m[1] ?? m[2] ?? m[3]!, 'migration_table', m.index!, m.index! + m[0].length, { databaseName: m[1] ?? m[2] ?? m[3], action: 'create', format: 'sql' }));
            for (const m of clean.matchAll(/\b(ALTER\s+TABLE|DROP\s+TABLE|FROM|JOIN|INTO|UPDATE)\s+(?:"([^"\n]+)"|`([^`\n]+)`|([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?))/gi))
                accesses.push(add(m[2] ?? m[3] ?? m[4]!, 'sql_operation', m.index!, m.index! + m[0].length, { databaseName: m[2] ?? m[3] ?? m[4], action: m[1]!.toUpperCase(), format: 'sql' }));
        }
        if (['typescript', 'javascript'].includes(file.language) && !file.readOnly) {
            const sf = ts.createSourceFile(file.path, source, ts.ScriptTarget.Latest, true, file.path.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
            const walk = (n: ts.Node) => {
                if (ts.isCallExpression(n)) {
                    const callee = n.expression.getText(sf), first = n.arguments[0];
                    if (first && ts.isStringLiteralLike(first) && (/^(fetch|axios\.(get|post|put|patch|delete|head))$/.test(callee))) {
                        let method = callee.startsWith('axios.') ? callee.split('.')[1]!.toUpperCase() : 'GET';
                        const opts = n.arguments[1];
                        if (opts && ts.isObjectLiteralExpression(opts))
                            for (const prop of opts.properties)
                                if (ts.isPropertyAssignment(prop) && prop.name.getText(sf) === 'method' && ts.isStringLiteralLike(prop.initializer))
                                    method = prop.initializer.text.toUpperCase();
                        http.push(add(`${method} ${first.text}`, 'http_client_call', n.getStart(sf), n.end, { method, url: first.text, format: 'literal-http-call' }));
                    }
                    if (ts.isPropertyAccessExpression(n.expression)) {
                        const raw = n.expression.getText(sf), m = /^(?:prisma|db)\.([A-Za-z_]\w*)\.(findMany|findUnique|findFirst|create|update|delete|upsert|count|aggregate)$/.exec(raw);
                        if (m)
                            accesses.push(add(m[1]!, 'orm_operation', n.getStart(sf), n.end, { databaseName: m[1], action: m[2], format: 'prisma-call', binding: 'syntactic_name_pattern' }));
                    }
                }
                ts.forEachChild(n, walk);
            };
            walk(sf);
        }
        if (/\.(graphql|gql)$/.test(file.path))
            for (const m of source.matchAll(/\b(type|input|enum|interface)\s+([A-Za-z_]\w*)[^{}]*\{([^}]*)\}/g)) {
                const type = add(m[2]!, 'schema_type', m.index!, m.index! + m[0].length, { format: 'graphql' });
                for (const f of m[3]!.matchAll(/\b([A-Za-z_]\w*)\s*(?:\([^)]*\))?\s*:\s*([\[\]!\w]+)/g)) {
                    const start = m.index! + m[0].indexOf('{') + 1 + f.index!, field = add(`${m[2]}.${f[1]}`, 'schema_field', start, start + f[0].length, { field: f[1], type: f[2], format: 'graphql' });
                    edge(type, field, 'has_field', field, 'syntactic');
                }
            }
        if (file.path.endsWith('.proto'))
            for (const m of source.matchAll(/\b(message|service)\s+([A-Za-z_]\w*)\s*\{/g))
                add(m[2]!, 'schema_type', m.index!, m.index! + m[0].length, { format: 'protobuf', extraction: 'top-level-header' });
        if (file.path.endsWith('.json')) {
            try {
                const json = JSON.parse(source);
                if (json.openapi && json.paths) {
                    for (const [url, raw] of Object.entries(json.paths)) {
                        if (!raw || typeof raw !== 'object')
                            continue;
                        for (const [method, body] of Object.entries(raw))
                            if (/^(get|post|put|patch|delete|head|options)$/.test(method)) {
                                const at = source.indexOf(JSON.stringify(url));
                                apis.push(add(`${method.toUpperCase()} ${url}`, 'openapi_endpoint', Math.max(0, at), Math.max(0, at) + JSON.stringify(url).length, { method: method.toUpperCase(), url, operationId: body?.operationId, format: 'openapi', anchor: 'path_key_enclosing_operation' }));
                            }
                    }
                }
            }
            catch { /* invalid JSON already visible through source/config diagnostics */ }
        }
    }
    const relate = (from: LocalEntity, to: LocalEntity, relation: string) => out.observations.push({ sourceKey: from.key, targetKey: to.key, relation, resolution: 'heuristic', path: from.path, producer: PRODUCER, startByte: from.startByte, endByte: from.endByte, startLine: from.startLine, endLine: from.endLine });
    for (const a of accesses)
        for (const table of tables)
            if (a.metadata!.service === table.metadata!.service && (String(a.metadata!.databaseName).toLowerCase() === String(table.metadata!.databaseName).toLowerCase() || a.kind === 'orm_operation' && table.kind === 'orm_model' && String(a.metadata!.databaseName).toLowerCase() === table.name.toLowerCase()))
                relate(a, table, 'possibly_accesses_table');
    for (const model of tables.filter(t => t.kind === 'orm_model'))
        for (const migration of tables.filter(t => t.kind === 'migration_table'))
            if (model.metadata!.service === migration.metadata!.service && model.metadata!.databaseName === migration.metadata!.databaseName)
                relate(model, migration, 'possibly_implemented_by_migration');
    for (const call of http)
        for (const api of apis)
            if (call.metadata!.method === api.metadata!.method && call.metadata!.url === api.metadata!.url && call.metadata!.service === api.metadata!.service)
                relate(call, api, 'possibly_consumes_contract');
    return out;
}
/** Breaking-change candidates for explicitly given OpenAPI JSON, not a universal compatibility proof. */
export function diffOpenApi(before: unknown, after: unknown): Record<string, unknown> {
    assert(before && after && typeof before === 'object' && typeof after === 'object', 'INVALID_CONTRACT', 'OpenAPI documents required');
    const a = before as any, b = after as any, breaking: unknown[] = [];
    assert(a.openapi && b.openapi && a.paths && b.paths, 'INVALID_CONTRACT', 'OpenAPI paths missing');
    for (const [path, methods] of Object.entries(a.paths)) {
        for (const [method, op] of Object.entries(methods as any)) {
            if (!/^(get|post|put|patch|delete|head|options)$/.test(method))
                continue;
            const next = b.paths[path]?.[method];
            if (!next) {
                breaking.push({ kind: 'operation_removed', path, method });
                continue;
            }
            const prior = op as any, oldParams = new Set((prior.parameters ?? []).filter((p: any) => p.required).map((p: any) => `${p.in}:${p.name}`));
            for (const p of next.parameters ?? [])
                if (p.required && !oldParams.has(`${p.in}:${p.name}`))
                    breaking.push({ kind: 'new_required_parameter', path, method, name: p.name, in: p.in });
        }
    }
    for (const [name, raw] of Object.entries(a.components?.schemas ?? {})) {
        const old = raw as any, next = b.components?.schemas?.[name];
        if (!next) {
            breaking.push({ kind: 'schema_removed', name });
            continue;
        }
        for (const field of Object.keys(old.properties ?? {}))
            if (!Object.hasOwn(next.properties ?? {}, field))
                breaking.push({ kind: 'field_removed', schema: name, field });
        for (const field of next.required ?? [])
            if (!(old.required ?? []).includes(field))
                breaking.push({ kind: 'field_newly_required', schema: name, field });
    }
    return { breakingCandidates: breaking, exhaustive: false, limitations: ['External refs, directional request/response compatibility, union narrowing and semantic constraints require additional contract tests.'] };
}
