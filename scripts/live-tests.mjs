#!/usr/bin/env node
/** Separate optional live gates from deterministic/default regression totals. */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
const group = process.argv[2] ?? 'native';
const groups = { native: { files: ['test/live/semantics.mjs', 'test/live/media.mjs'], env: { CI_LIVE_SEMANTICS: '1', CI_LIVE_MEDIA: '1' } }, communities: { files: ['test/enrichment-taint-testmap.test.mjs'], env: { CI_LIVE_COMMUNITIES: '1' } }, upstreams: { files: ['test/live/upstreams.mjs'], env: { CI_LIVE_UPSTREAMS: '1' } } };
if (!groups[group])
    throw Error('Choose native, communities, or upstreams');
const g = groups[group], r = spawnSync(process.execPath, ['--test', '--test-concurrency=1', ...g.files], { stdio: 'inherit', env: { ...process.env, ...g.env } });
if (r.error)
    console.error(r.error.message);
process.exitCode = r.status ?? 1;
