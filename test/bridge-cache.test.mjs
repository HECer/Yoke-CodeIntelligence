import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve, toNamespacedPath } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { fixture } from './helpers.mjs';
import { runBridge } from '../dist/adapters/bridges.js';
import { hash } from '../dist/core/util.js';
import { inside } from '../dist/core/paths.js';

test('Graphify spawn failure removes its separately owned cache and materialized tree', async t => {
    const f = fixture(t, { 'a.py': 'def owned():\n    return 1\n' }), s = f.snapshot();
    const parent = resolve(tmpdir()), prefix = `ci-gfy-${hash(f.ws.store.directory).slice(0, 16)}-`;
    const caches = () => readdirSync(parent).filter(name => name.startsWith(prefix)).sort();
    const changes = join(f.ws.store.directory, 'changes');
    const trees = () => existsSync(changes) ? readdirSync(changes).sort() : [];
    const beforeCaches = caches(), beforeTrees = trees();
    await assert.rejects(() => runBridge(f.engine, 'graphify', s.id, {
        trusted: true, python: join(f.root, 'missing-owned-provider.exe'), timeoutMs: 10000,
    }), error => error.code === 'PROCESS_UNAVAILABLE');
    assert.deepEqual(caches(), beforeCaches);
    assert.deepEqual(trees(), beforeTrees);
    assert.equal(f.read('a.py'), 'def owned():\n    return 1\n');
});

test('Graphify worker passes separate host cache and original root to its pinned extraction API', t => {
    const f = fixture(t, { 'a.py': 'def owned():\n    return 1\n' }), s = f.snapshot();
    const parent = resolve(tmpdir()), cache = mkdtempSync(join(parent, 'ci-gfy-contract-'));
    t.after(() => {
        assert.equal(dirname(resolve(cache)), parent);
        assert(basename(cache).startsWith('ci-gfy-contract-'));
        rmSync(cache, { recursive: true, force: true });
    });
    const program = `import importlib.metadata,types,sys,runpy,json
from pathlib import Path
version=importlib.metadata.version
importlib.metadata.version=lambda name: '0.9.87' if name=='graphifyy' else version(name)
package=types.ModuleType('graphify');module=types.ModuleType('graphify.extract')
def extract(paths,*,root,cache_root):
 assert root.is_absolute() and cache_root.is_absolute()
 assert root!=cache_root and not cache_root.is_relative_to(root)
 assert [p.relative_to(root).as_posix() for p in paths]==['a.py']
 assert paths[0].read_text(encoding='utf-8')=='def owned():\\n    return 1\\n'
 (cache_root/'visited').write_text('contract fixture',encoding='utf-8')
 return {'nodes':[],'links':[]}
module.extract=extract;sys.modules['graphify']=package;sys.modules['graphify.extract']=module
runpy.run_path(sys.argv[1],run_name='__main__')
`;
    const input = { root: f.root, cacheRoot: toNamespacedPath(cache), snapshotId: s.id, sourceHashes: { 'a.py': hash(f.read('a.py')) } };
    const r = spawnSync(process.platform === 'win32' ? 'python' : 'python3', ['-I', '-c', program, fileURLToPath(new URL('../workers/graphify_bridge.py', import.meta.url))], {
        input: JSON.stringify(input), encoding: 'utf8', timeout: 10000,
        env: { PATH: process.env.PATH ?? '', SystemRoot: process.env.SystemRoot ?? '', CI: '1', DO_NOT_TRACK: '1' },
    });
    assert.ifError(r.error);
    assert.equal(r.status, 0, r.stderr);
    const result = JSON.parse(r.stdout);
    assert.equal(result.snapshotId, s.id);
    assert.deepEqual(result.sourceHashes, input.sourceHashes);
    assert.equal(result.version, '0.9.87');
    assert.equal(result.rawExtraction, true);
    assert.equal(readFileSync(join(cache, 'visited'), 'utf8'), 'contract fixture');
    assert.equal(f.read('a.py'), 'def owned():\n    return 1\n');
    unlinkSync(join(cache, 'visited'));
    const refused = spawnSync(process.platform === 'win32' ? 'python' : 'python3', ['-I', '-c', program, fileURLToPath(new URL('../workers/graphify_bridge.py', import.meta.url))], {
        input: JSON.stringify({ ...input, cacheRoot: 'relative-cache' }), encoding: 'utf8', timeout: 10000,
        env: { PATH: process.env.PATH ?? '', SystemRoot: process.env.SystemRoot ?? '', CI: '1', DO_NOT_TRACK: '1' },
    });
    assert.notEqual(refused.status, 0);
    assert.match(refused.stderr, /Host cache directory must be existing and absolute/);
    assert(!existsSync(join(cache, 'visited')));
});

test('bridge contract fixture verifies ordinary and dependency bytes omitted by provider output', async t => {
    // This venv contains a labeled protocol fixture, not an installed Graphify
    // claim. Separate optional live gates exercise the actual pinned package.
    const parent = resolve(tmpdir()), environment = mkdtempSync(join(parent, 'ci-gfy-fixture-'));
    t.after(() => {
        assert.equal(dirname(resolve(environment)), parent);
        assert(basename(environment).startsWith('ci-gfy-fixture-'));
        rmSync(environment, { recursive: true, force: true });
    });
    const python = process.platform === 'win32' ? 'python' : 'python3';
    const created = spawnSync(python, ['-I', '-m', 'venv', '--without-pip', environment], { encoding: 'utf8', timeout: 15000 });
    assert.ifError(created.error);
    assert.equal(created.status, 0, created.stderr);
    const executable = join(environment, process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python');
    const location = spawnSync(executable, ['-I', '-c', 'import sysconfig;print(sysconfig.get_path("purelib"))'], { encoding: 'utf8', timeout: 10000 });
    assert.equal(location.status, 0, location.stderr);
    const site = location.stdout.trim();
    const canonicalEnvironment = realpathSync(environment), canonicalSite = realpathSync(site);
    assert(canonicalSite !== canonicalEnvironment && inside(canonicalEnvironment, canonicalSite));
    mkdirSync(join(site, 'graphify'), { recursive: true });
    mkdirSync(join(site, 'graphifyy-0.9.87.dist-info'), { recursive: true });
    writeFileSync(join(site, 'graphify/__init__.py'), '');
    writeFileSync(join(site, 'graphifyy-0.9.87.dist-info/METADATA'), 'Metadata-Version: 2.1\nName: graphifyy\nVersion: 0.9.87\n');
    for (const mutation of [null, 'omitted.py', 'node_modules/owned/index.d.ts']) {
        await t.test(mutation ? `refuses mutation of omitted ${mutation}` : 'imports ordinary source and admitted dependency declarations', async child => {
            const source = { 'a.py': 'def owned():\n    return 1\n', 'omitted.py': '# untouched original\n', 'node_modules/owned/index.d.ts': 'export declare function owned(): number;\n' };
            const f = fixture(child, source), s = f.snapshot();
            assert(s.files.some(file => file.path === 'node_modules/owned/index.d.ts' && file.readOnly));
            const prefix = `ci-gfy-${hash(f.ws.store.directory).slice(0, 16)}-`;
            const before = readdirSync(parent).filter(name => name.startsWith(prefix)).sort();
            writeFileSync(join(site, 'graphify/extract.py'), `from pathlib import Path
def extract(paths,*,root,cache_root):
 assert root!=cache_root and cache_root.is_dir()
 (cache_root/'owned-cache-file').write_text('fixture',encoding='utf-8')
 mutation=${mutation ? JSON.stringify(mutation) : 'None'}
 if mutation is not None:
  path=root/mutation
  path.write_text(path.read_text(encoding='utf-8')+'// provider mutation\\n',encoding='utf-8')
 return {'nodes':[{'id':'owned','source_file':'a.py','label':'owned','source_location':'L1','type':'function'}],'edges':[]}
`);
            const request = () => runBridge(f.engine, 'graphify', s.id, { trusted: true, python: executable, timeoutMs: 10000 });
            if (mutation) {
                await assert.rejects(request, error => error.code === 'SNAPSHOT_STALE');
                assert.throws(() => f.ws.store.graph(s.id));
            }
            else {
                const result = await request();
                assert.equal(result.imported, true);
                assert(f.ws.store.graph(s.id).entities.some(entity => entity.name === 'owned'));
            }
            assert.deepEqual(readdirSync(parent).filter(name => name.startsWith(prefix)).sort(), before);
            for (const [path, text] of Object.entries(source))
                assert.equal(f.read(path), text);
        });
    }
});
