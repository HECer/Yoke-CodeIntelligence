from pathlib import Path
import hashlib,json,shutil,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r3-policy-retention';out=w/'docs/validation/integration-2026-10-21/policy-retention-floats'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
g=read(raw/'green-r4/result.json');a=read(raw/'after-r1/result.json');assert g['allChecksPassed'] and g['sourceUnchanged'] and a['legacyAndCategoryV1RawStdoutExact'] and a['savedRootRetentionPlansPassed'] and a['cliSha256']==g['cliSha256']
assert all(c['exitCode']==0 and c['sourceUnchanged'] for c in g['checks']);totals={k:sum(c['totals'][k] for c in g['checks']) for k in ['passed','failed','ignored']};assert totals=={'passed':101,'failed':0,'ignored':0}
frozen=read(raw/'green-r4/source-after.json');assert all(sha((w/n).read_bytes())==h for n,h in frozen.items())
lock=(w/'Cargo.lock').read_bytes()
assert sha(lock)==read(raw/'float-red-r1/source-before.json')['Cargo.lock']
assert lock.replace(b'\r\n',b'\n')==subprocess.check_output(['git','show','HEAD:Cargo.lock'],cwd=w)
assert 'pass 260' in (raw/'green-r4/retained.log').read_text(encoding='utf-8')
out.mkdir(parents=True,exist_ok=False)
def copy(src,rel):
    dest=out/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest);assert src.read_bytes()==dest.read_bytes()
for phase in sorted(raw.iterdir()):
    if phase.is_dir():
        for f in sorted(phase.iterdir()):
            assert f.is_file();copy(f,Path(phase.name)/f.name)
for name in ['ci-r3-retention-check.py','ci-r3-retention-cli-after.py']:
    copy(Path.home()/'AppData/Local/Temp'/name,Path('controllers')/name)
copy(Path(__file__),Path('archive-and-docs.py'))
summary={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'nativeScopedTotals':totals,'selectedChecks':g['checks'],'actualCliControl':a,'cliSha256':g['cliSha256'],'runtimeSourceUnchangedDuringAndAfterChecks':True,'lockedDependencyVersionsUnchanged':True,'serdeJsonVersion':'1.0.154','serdeJsonFeature':'float_roundtrip','retainedPassed':260,'retainedSkipped':3,'fullNativeCurrentHeadRecheck':'pending','fullStatusCertified':False,'communityPolicyImplemented':False}
(out/'checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
(out/'README.md').write_text('''# Canonical policy retention and exact JSON floats

The [checkpoint](checkpoint.json) records the final green-r4 guard: 101 Rust
tests across fifteen targets, format, all-target Clippy with warnings denied,
typecheck and TypeScript 260 passed / 3 existing skips. All inputs remain frozen
during the guard. Documentation follows validation. Overlapping development runs
are not added together. The [index](index.json) hashes every archived file except
itself and preserves the original raw logs.

A valid canonical source-category report previously blocked retention with an
unknown-artifact-field error. Canonical policy reports now replay source scope,
original options, numerical analysis and historical comparisons read-only, then
compare exact native serialized bytes. Markdown and both historical source roots
retain their independent checks. Legacy report shape validation is preserved.

The new historical fixture additionally exposed JSON float parsing that changed
serialized values and caused a noncanonical-report refusal. The independent
float-red-r1 witness loses one IEEE bit for -1.1819158364087053e-285. Enabling
float_roundtrip on the already locked serde_json1.0.154 preserves finite boundary
values and finite members of a deterministic 4096-pattern sample, signed zero, Unicode and
canonical bytes. Duplicate-key, depth, byte-budget, overflow and deadline refusals
still pass. Cargo.lock and all dependency versions remain unchanged.

Canonical retention tests cover policy reports with/without a prior snapshot,
source and artifact roots, expired deadlines, and rehashed changes to cohesion,
audit, configuration, future fields and signed zero. The stronger adversarial
fixture keeps the authentic report registered and adds the forged one, avoiding
an unrelated orphan-artifact refusal. representation-red-r2 proves the zero-sign
mutation passed JSON Value equality; the final comparison requires exact bytes.

Other raw phases remain: red-r1 is a fixture compile error calling a private
Markdown helper; red-r2 reproduces the original retention failure; green-r1
encounters the fixture's deliberately expired Store deadline before resetting it
for a subsequent read; green-r2 exposes historical float corruption. green-r3
passes all tests before strengthening the signed-zero counterexample.
representation-red-r1's early fixture passes via an incidental orphan refusal;
representation-red-r2 retains both registrations and exposes the actual flaw.

The actual CLI repeats the same saved owned snapshot. Default AND source-category
v1 raw stdout, JSON/artifact/Markdown identities match the pre-change controls.
Actual retention and purge planning recognize all saved-root dependencies without
deleting any state. Original source bytes stay unchanged. No model or analyzed
project code is executed.

A complete native recheck for the shared parser feature remains pending here.
Existing Windows OS1314, ignored runtime gates, other R1-R9 work, new community
policy, current-head hosted/upstream qualification, main and release remain open.
''',encoding='utf-8',newline='\n')
index=[{'path':str(p.relative_to(out)).replace('\\','/'),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.rglob('*')) if p.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'copiedRawFilesByteExact':True},indent=2)+'\n',encoding='utf-8',newline='\n')
def edit(name,old,new):
    p=w/name;s=p.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:60]);p.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
edit('docs/STATUS.md','## Current integration work\n','''## Current integration work

Canonical source-category reports now participate in retention through exact
read-only reconstruction of their saved sources, options and historical analysis.
Rehashed changes to scores/audit/policy/future fields and signed zero refuse;
report/Markdown and both historical source roots remain protected. The original
legacy shape checks remain. The already pinned JSON parser now uses exact float
roundtrips, fixing one-bit changes that could invalidate legitimate persisted
reports. Dependency versions and the lockfile stay unchanged.

The scoped guard passes 101 Rust tests across fifteen targets, format, all-target
Clippy, typecheck and TypeScript 260/3 existing skips. An actual same-snapshot CLI
control preserves both legacy and source-category v1 raw stdout/report/Markdown
identities, and actual retention/purge plans recognize its saved dependencies.
No deletion, model or analyzed-source execution occurs in this control. Failed
development phases and the stronger signed-zero counterexample remain archived.
The shared parser feature still needs a complete native recheck; broader R3
community policy, R1-R9/platform/hosted/upstream acceptance remain open. See the
[exact replay evidence](validation/integration-2026-10-21/policy-retention-floats/README.md).
''')
edit('docs/ARCHITECTURE.md','reports and checked View registrations use separate producers and exact replay;\nthe absent-policy branch preserves released options/artifact identities.', 'reports and checked View registrations use separate producers and exact replay.\nCanonical policy retention reconstructs source/options/history read-only and\ncompares exact serialized bytes; signed zero cannot pass by Value equality. The\npinned JSON parser preserves finite float roundtrips. The absent-policy branch\npreserves released options/artifact identities.')
edit('docs/REMAINING_WORK.md','with separate policy producers and exact legacy report preservation.', 'with separate policy producers, exact legacy report preservation and checked canonical/View retention replay.')
p=w/'native/docs/source-category-policy.md';s=p.read_text(encoding='utf-8');p.write_text(s+'''
Canonical policy reports also participate in retention. Their source scope,
options, prior snapshot comparison and full native report are reconstructed
read-only and compared as exact serialized bytes before retaining report,
Markdown and source roots. Rehashing modified scores, audit, policy or unknown
fields does not establish validity. Signed-zero differences are rejected even
though JSON Value equality treats those floats as equal. The pinned JSON parser
uses float_roundtrip to preserve finite IEEE values through saved JSON; duplicate,
depth/node/byte, nonfinite and deadline boundaries remain in force. The legacy
report shape validator and published v1 producers remain separate. Future
analysis behavior needs its own explicit policy/producer so old reports can replay.
''',encoding='utf-8',newline='\n')
for n,h in frozen.items():
    if sha((w/n).read_bytes())!=h:assert n=='native/docs/source-category-policy.md',n
fixture=w/'native/tests/fixtures/postgres-catalog-response.jsonl';assert len(fixture.read_bytes())==2779 and sha(fixture.read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
print(json.dumps({'archivedFiles':len(index)+1,'archivedBytes':sum(x['bytes'] for x in index),'nativeScopedTotals':totals,'runtimeSourceExact':True,'docsUpdated':True}))
