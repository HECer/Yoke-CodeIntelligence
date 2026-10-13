import { readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
const files = readdirSync('test').filter(n => n.endsWith('.test.mjs')).sort().map(n => 'test/' + n);
const args = ['--test', '--test-concurrency=2', ...(process.argv.includes('--coverage') ? ['--experimental-test-coverage'] : []), ...files];
const result = spawnSync(process.execPath, args, { stdio: 'inherit' });
if (result.error)
    console.error(result.error.message);
process.exitCode = result.status ?? 1;
