from pathlib import Path
import hashlib,json,re,shutil,subprocess,zipfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r3-community-policy'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
u=read(raw/'units-r1/result.json');c=read(raw/'contracts-r1/result.json');p=read(raw/'primary-r1/result.json');a=read(raw/'primary-cli-r1/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [u,c,p])
assert all(a[k] for k in ['legacyAndCategoryV1RawStdoutExact','newCommunityRepeatedRawStdoutExact','originalGraphAndSourceEvidenceExact','savedRootRetentionPlansPassed','sourceUnchanged'])
assert a['cliSha256']==p['cliSha256']
assert all(sha((w/n).read_bytes())==h for n,h in read(raw/'primary-r1/source-after.json').items())
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
totals=lambda r:{k:sum(c['totals'][k] for c in r['checks']) for k in ['passed','failed','ignored']}
assert totals(u)=={'passed':6,'failed':0,'ignored':0} and totals(c)=={'passed':97,'failed':0,'ignored':0} and totals(p)=={'passed':12,'failed':0,'ignored':0}
retained=(raw/'primary-r1/retained.log').read_text(encoding='utf-8');assert re.search(r'^(?:#|\u2139) pass 260$',retained,re.M) and re.search(r'^(?:#|\u2139) skipped 3$',retained,re.M) and re.search(r'^(?:#|\u2139) fail 0$',retained,re.M)
out=w/'docs/validation/integration-2026-10-21/community-policy';out.mkdir(parents=True,exist_ok=False)
entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for f in sorted(raw.rglob('*')):
        if not f.is_file():continue
        assert not f.is_symlink();name=f.relative_to(raw).as_posix();data=f.read_bytes();info=zipfile.ZipInfo(name,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16;archive.writestr(info,data);entries.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert archive.testzip() is None and set(archive.namelist())=={e['path'] for e in entries}
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
checkpoint={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'secondaryUnitTotals':totals(u),'secondaryIntegrationTotals':totals(c),'primaryRecheckTotals':totals(p),'countsOverlapNotAdditive':True,'formatAndAllTargetClippyPassedInBothCheckouts':True,'primaryTypecheckPassed':True,'retainedPassed':260,'retainedSkipped':3,'actualPrimaryCli':a,'secondaryActualCli':read(raw/'secondary-cli-r1/result.json'),'crossCheckout':read(raw/'cross-checkout-r1/result.json'),'baselineSourceCommit':'bcf442b32a6dd03bcd3c6c8e8794eaf3fd929079','baselineTotals':{'passed':2951,'failed':10,'ignored':53},'newModeWholeNativeSuiteExecuted':False,'newModeUpstreamParityCertified':False,'fullWindowsCertified':False,'fullStatusCertified':False,'runtimeSourceUnchangedDuringChecks':True}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n',encoding='utf-8',newline='\n');shutil.copyfile(__file__,out/'archive-and-docs.py')
(out/'README.md').write_text('''# Explicit source-stable community projection

The optional neighbor_majority_v1 policy has its own canonical and selected-View
producers. Original source bytes, directed/parallel observations, resolution and
evidence stay intact. The old default and source-category-only v1 paths remain
separate; actual CLI controls preserve both original raw stdout and artifact/
Markdown identities. With/without source categories, the new mode repeats exact
raw stdout on the same source and protects its saved dependencies in retention.

Numerical coverage includes unique-neighbor hub degree, parallel-weight-resistant
votes, stranded nodes, self-loops, true isolates, deterministic source-order ties
despite changed snapshot IDs, bounded oversized/sparse passes, an unsplittable
dense group, singleton cohesion, closed policy schemas and deadlines. Six units
and 97 integrations pass, including canonical/history and selected-View replay,
forged registrations, zero weights and unchanged legacy goldens. Formatting and
all-target Clippy with warnings denied pass.

A checkout comparison finds 95 line-ending-only differences, the two intentional
modified source files and byte-exact workers/fixtures. After integration, the
primary checkout separately repeats all twelve new numerical/integration cases,
format and all-target Clippy, then typecheck and TypeScript260/3 existing skips.
Those twelve overlap the earlier 103 tests and are not added. Its own actual CLI
control passes independently on its exact compiled executable and frozen inputs.
Documentation follows the code checks. No analyzed project source or model is
executed by these CLI controls.

The [checkpoint](checkpoint.json) retains exact commands/results and executable
identities. [raw-index.json](raw-index.json) indexes every byte-exact decompressed
member of [raw-logs.zip](raw-logs.zip). The unsupported-mode witness precedes the
new compiled CLI: red-interface-r1 incorrectly expected the field name in the
private static error; red-interface-r2 keeps a passing old-schema control and
checks the actual static refusal with unchanged source/Store files. That witness
is not a numerical test-before-implementation claim. Earlier format-only and
legacy-token audits retain their explicit uncompiled status at those phases.

The independent native algorithm uses pinned leiden-rs0.8.1. It does not promise
identical graspologic partitions, successful splitting or a hard maximum size.
Quality refers to the initial core, as disclosed in report diagnostics. Original
source evidence and inferred rankings remain different categories.

The complete default baseline at bcf442b remains separate:2951/10OS1314/53ignored.
It predates this new mode and is not a new-head full-suite certificate. Current
Windows symlink prerequisites, Google live export without an approved test file,
wider R1-R9/upstream/hosted/platform qualification, main and release stay open.
No system setting, privilege, deployment or approval authority is changed.
''',encoding='utf-8',newline='\n')
index=[{'path':f.name,'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.iterdir()) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')

def replace(name,old,new):
    f=w/name;s=f.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:70]);f.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
replace('native/docs/community-policy.md','Primary-checkout integration\nand retained TypeScript checks remain separate pending steps. This is not full\nupstream or platform acceptance.','The primary checkout separately repeats all twelve new cases, formatting and\nall-target Clippy, then passes typecheck and TypeScript260/3 existing skips. The\nrepeated cases overlap the earlier counts. Its own compiled CLI control also\npasses with frozen inputs. This is not full upstream or platform acceptance.')
replace('native/docs/graph-extensions.md','`graph_extensions::analyze(&Store, &Snapshot, &Graph, &AnalysisOptions, &Deadline)`','An independent optional [community projection policy](community-policy.md) adds\nunique-neighbor hub degree/voting, stranded-node holdout and two bounded split\nstages under separate immutable report producers. Omission keeps the defaults\nbelow and both existing report families unchanged.\n\n`graph_extensions::analyze(&Store, &Snapshot, &Graph, &AnalysisOptions, &Deadline)`')
replace('docs/ARCHITECTURE.md','Classic function-body inlining uses independent original-byte statement frames','Explicit neighbor-majority-v1 community analysis separately selects an\nundirected positive-weight projection, unique-neighbor hub degree/voting,\nstranded-node holdout and two bounded split stages. Source-order ties avoid\nsnapshot-hash churn. Its canonical/View producers and exact retention replay\nbind the selected options; no split or final maximum-size guarantee is invented.\nAbsent community options preserve both older report families. See the\n[community contract](../native/docs/community-policy.md).\n\nClassic function-body inlining uses independent original-byte statement frames')
replace('docs/REMAINING_WORK.md','Remaining partition/splitting/hub-reattachment policy differences, upstream analytics equivalence and acceptance.','An explicit neighbor-majority-v1 policy now implements unique-neighbor hub holdout/voting and two bounded split stages with separate producers, stable source ordering and exact retention. Broader partition/library differences, upstream analytics equivalence and acceptance remain open.')
replace('docs/STATUS.md','## Current integration work\n','''## Current integration work

An optional [community projection policy](../native/docs/community-policy.md)
now adds unique-neighbor hub degree/voting, stranded-node holdout and bounded
oversized/sparse split passes. Canonical and selected-View producers pin this
explicit mode. Source-order ties avoid snapshot-ID churn; original directed and
parallel observations, resolution labels, source bytes and evidence stay intact.
The old default and source-category-only v1 reports preserve exact raw CLI and
artifact/Markdown identities. No successful split or hard final size is claimed.

Six numerical units and 97 integrations pass with format/all-target Clippy.
The integrated primary checkout repeats all twelve new cases (overlapping),
format/Clippy, typecheck and TypeScript260/3 existing skips. Both actual compiled
CLI controls preserve old report families, repeat new mode bytes with/without
source categories and pass saved-root retention plans. All scoped inputs stay
frozen during checks; documentation follows. No model/analyzed-source execution
occurs in these controls. See the [indexed evidence](validation/integration-2026-10-21/community-policy/README.md).
The complete bcf442b baseline below predates the new mode; a new-head whole-native
run, full R3/R1-R9/upstream/hosted/platform/main/release acceptance remain open.
''')
print(json.dumps({'archive':str(out),'rawFiles':len(entries),'rawMembersByteExact':True,'nativeScopedTests':103,'primaryRepeatedNewCases':12,'countsNotAdditive':True,'primaryTypecheck':True,'retainedPassed':260,'retainedSkipped':3,'cliSha256':a['cliSha256'],'fullStatusCertified':False}))
