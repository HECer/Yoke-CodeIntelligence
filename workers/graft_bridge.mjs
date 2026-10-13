/** Optional, pinned upstream bridge. The input root must be an isolated snapshot. */
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, relative, isAbsolute } from 'node:path';
const input = JSON.parse(readFileSync(0, 'utf8'));
const require = createRequire(import.meta.url);
const meta = require('@nanonets/graft/package.json');
if (meta.version !== '0.21.1')
    throw new Error('Expected @nanonets/graft@0.21.1; validate a new version before changing the pin.');
const { Graft } = await import('@nanonets/graft');
const root = resolve(input.root);
const engine = new Graft({ contextDir: '.ci-graft-output' });
console.log = (...parts) => process.stderr.write(parts.map(String).join(' ') + '\n');
const result = await engine.graph(root, { llm: false, lsp: false, reuse: true });
if (result.errors?.length)
    throw new Error('Graft reported extraction errors: ' + result.errors.join('; '));
const path = resolve(root, result.graphPath);
const rel = relative(root, path);
if (isAbsolute(rel) || rel === '..' || rel.startsWith('../'))
    throw new Error('Graft graph output escaped sandbox');
const graph = JSON.parse(readFileSync(path, 'utf8'));
process.stdout.write(JSON.stringify({ snapshotId: input.snapshotId, sourceHashes: input.sourceHashes, version: meta.version, graph }));
