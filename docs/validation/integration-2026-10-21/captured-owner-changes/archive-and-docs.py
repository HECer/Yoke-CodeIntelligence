from pathlib import Path
import hashlib,json,re,shutil,subprocess,zipfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r5-owner-changes';read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
n=read(raw/'new-target-r1/result.json');c=read(raw/'contracts-r1/result.json');p=read(raw/'primary-r1/result.json');a=read(raw/'primary-cli-r1/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [n,c,p])
assert all(a[k] for k in ['legacyOwnersRawStdoutExact','ownerChangesRepeatedRawStdoutExact','unchangedSourceDeclaredCandidatesChanged','originalFileHashPreserved','exactNewlineOutputBudgetPassed','allOwnedSourceStoreAndGitFilesUnchanged','sourceUnchanged'])
assert (raw/'primary-cli-r1/owner-changes.stdout').read_bytes()==(raw/'secondary-cli-r1/owner-changes.stdout').read_bytes()
assert all(sha((w/name).read_bytes())==h for name,h in read(raw/'primary-r1/source-after.json').items())
assert all(sha((w/name).read_bytes())==h for name,h in read(raw/'contracts-r1/source-after.json').items())
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
totals=lambda r:{k:sum(s['totals'][k] for s in r['checks']) for k in ['passed','failed','ignored']}
assert totals(n)=={'passed':6,'failed':0,'ignored':0} and totals(c)=={'passed':65,'failed':0,'ignored':0}
log=(raw/'primary-r1/retained.log').read_text(encoding='utf-8');assert all(re.search(r'^(?:#|\u2139) '+s+r'$',log,re.M) for s in ['pass 260','fail 0','skipped 3'])
build=next(s for s in p['checks'] if s['name']=='build');assert len(build['primaryPackageArtifacts'])==2 and all(not x['fresh'] for x in build['primaryPackageArtifacts'])
out=w/'docs/validation/integration-2026-10-21/captured-owner-changes';out.mkdir(parents=True,exist_ok=False);entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for f in sorted(raw.rglob('*')):
        if not f.is_file():continue
        assert not f.is_symlink();name=f.relative_to(raw).as_posix();data=f.read_bytes();info=zipfile.ZipInfo(name,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16;archive.writestr(info,data);entries.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert archive.testzip() is None and set(archive.namelist())=={e['path'] for e in entries}
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
checkpoint={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'newIntegrationTotals':totals(n),'existingRegressionTotals':totals(c),'distinctRustCases':71,'frozenAuxiliaryInputs':993,'all993PrimaryInputsByteExact':True,'sourceBaselineCopiedInputs':990,'primaryBuildArtifacts':build['primaryPackageArtifacts'],'primaryFmtBuildTypecheckPassed':True,'auxiliaryFmtAllTargetClippyPassed':True,'retainedPassed':260,'retainedSkipped':3,'primaryActualCli':a,'secondaryActualCli':read(raw/'secondary-cli-r1/result.json'),'crossCheckoutNewRawStdoutExact':True,'rustTestsRepeatedInPrimary':False,'fullCurrentNativeSuiteExecuted':False,'fullR5Certified':False,'fullWindowsCertified':False,'fullStatusCertified':False}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n',encoding='utf-8',newline='\n');shutil.copyfile(__file__,out/'archive-and-docs.py')
(out/'README.md').write_text('''# Captured owner changes across unchanged source files

The actual pre-change CLI fixture changes only CODEOWNERS from one authored
label to another. The existing owners query returns CODEOWNERS alone and omits
the unchanged Rust path. Its original raw result repeats exactly. The then-
unsupported owner-changes command refuses without changing any owned files.
This is an actual CLI availability/behavior witness, not a claim that numerical
tests ran against an earlier implementation.

The new separate query compares all paths in both captured manifests through the
same checked parent transaction and finite rule evaluator. It verifies every
captured body, retains full original parent and rule/token byte evidence, and
distinguishes candidate, winning-evidence, added/removed and indeterminate cases.
Missing/unsupported rules remain explicit; no identity, permission or actual
review requirement is established. GitHub review requests use base-branch rules.

Six new integration functions and 65 existing Git regressions pass: 71 distinct
Rust cases, formatting and all-target Clippy with warnings denied. Coverage
includes Unicode/CRLF, duplicate tokens, unchanged source bodies, relocated rule
anchors, explicit empty lists, unrelated container-hash changes, all zero quotas,
tampered bodies, caller transactions/deadlines and offline/private CLI admission.
The exact CLI output boundary includes its final LF; a one-byte-short budget
refuses without stdout. Both binaries preserve original owner output bytes,
repeat the new result exactly and leave every owned source/Store/Git file intact.

The isolated worktree starts from 990 byte-exact primary inputs. Its initial
CRLF copy exposed 851 stat/filter-cache differences; refreshing only the index
produces zero staged content changes without touching working bytes. After
integration all 993 validated inputs match byte-exactly. Primary Cargo build
then freshly compiles both library and CLI from the primary manifest/source
paths; the artifact records have fresh:false. Primary formatting, typecheck and
TypeScript260/3 existing skips pass. Rust tests are not repeated on identical
inputs; an independent actual primary CLI control pins its own executable hash.
The complete new raw output is also byte-exact across the two binaries.

[checkpoint.json](checkpoint.json) records commands/results, source and binary
proofs. [raw-index.json](raw-index.json) indexes each decompressed member of
[raw-logs.zip](raw-logs.zip), verified against its exact original bytes. No raw
line endings or trailing whitespace are rewritten. Documentation follows the
source checks; no runtime input is changed during qualification.

The complete default baseline at bcf442b predates both new community and owner
change modes:2951 passed/10 original OS1314 setup failures/53 ignored. It is not
a full run at this source commit. Wider R5/R1-R9, compiler override/dispatch,
upstream/parser/platform/hosted acceptance, actual review/remote truth, main and
release remain open. No model or analyzed-source execution occurs in these CLI
controls. No Developer Mode, privilege or system setting is changed.
''',encoding='utf-8',newline='\n')
def replace(name,old,new):
    f=w/name;s=f.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:70]);f.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
replace('native/docs/git-owner-changes.md','Primary integration\nand required retained TypeScript checks remain pending.','All 993 validated inputs match the integrated primary checkout byte-exactly.\nPrimary Cargo freshly builds its library and CLI; formatting, typecheck and\nTypeScript260/3 existing skips pass. The independent actual primary CLI also\npasses, with the complete new stdout exact across both binaries.')
replace('native/docs/git-owner-changes.md','Both manifest, file, source-byte and rule-byte reservations precede new CAS reads.','The shared rule parser also retains its existing per-file 1,000,000-byte,\nper-line 4,096-byte, pattern 1,024-byte, token 256-byte and 50-token-per-rule bounds.\n\nBoth manifest, file, source-byte and rule-byte reservations precede new CAS reads.')
replace('native/docs/git-owners.md','# Captured per-path owner candidates\n','# Captured per-path owner candidates\n\nThe separate [owner-changes query](git-owner-changes.md) also compares unchanged\ncaptured files when rule edits alter their authored candidates. This existing\nimpact-path query and its v1 options/result identities stay unchanged.\n')
replace('docs/ARCHITECTURE.md','Classic function-body inlining uses independent original-byte statement frames','Captured owner changes separately compare the full saved base/head path union,\nincluding unchanged file bodies, through the checked impact/area transaction and\nshared finite rule evaluator. Exact original source/token evidence and the parent\nremain unchanged. Candidate changes and winning-line evidence changes differ from\nsource-body changes; missing rules stay indeterminate. New body/match/output\nbounds admit complete results without writes, Git, network or model execution.\nThis grants no identity, permission or actual review authority. See the\n[owner-change contract](../native/docs/git-owner-changes.md).\n\nClassic function-body inlining uses independent original-byte statement frames')
replace('docs/REMAINING_WORK.md','finite CODEOWNERS interpretation with original byte evidence and visible unknowns.','finite CODEOWNERS interpretation with original byte evidence and visible unknowns; explicit captured owner changes over all saved base/head paths, including unchanged bodies, with original parent/rule evidence and bounded body replay.')
replace('docs/STATUS.md','## Current integration work\n','''## Current integration work

The separate [captured owner-change query](../native/docs/git-owner-changes.md)
now reports authored candidate changes for unchanged files when only CODEOWNERS
changes. It compares the full saved path union, verifies captured bodies, and
retains original parent, rules, duplicate tokens and Unicode/CRLF byte evidence.
Missing/unsupported rules stay indeterminate; candidate changes, moved winning
evidence, added/removed paths and source hashes remain distinct. It grants no
authenticated ownership, permission or actual review authority. The old owners
query preserves exact raw result identities.

Six new integration functions and 65 existing Git regressions pass (71 Rust
cases), with format/all-target Clippy. All 993 validated inputs match primary
byte-exactly; its library/CLI freshly build and format/typecheck/TypeScript260/3
existing skips pass. Both actual binaries preserve old owner bytes, repeat the
same new result, enforce exact newline budgets and leave every owned source,
Store and Git file unchanged. No Git/network/model or analyzed-source execution
occurs in these query controls. See the [indexed evidence](validation/integration-2026-10-21/captured-owner-changes/README.md).
The new mode has no complete native-suite or full R5/R1-R9/upstream/hosted/
platform/main/release acceptance; the earlier complete baseline stays separate.
''')
frozen=read(raw/'primary-r1/source-after.json');changed=[]
for name,h in frozen.items():
    now=sha((w/name).read_bytes())
    if now!=h:changed.append({'path':name,'validatedSha256':h,'documentedSha256':now})
assert {x['path'] for x in changed}=={'native/docs/git-owner-changes.md','native/docs/git-owners.md','docs/ARCHITECTURE.md','docs/REMAINING_WORK.md','docs/STATUS.md'}
(out/'post-validation-docs.json').write_text(json.dumps({'postValidationChangesOnlyDocumentation':True,'allRuntimeInputsByteExact':True,'changedDocuments':changed},indent=2)+'\n',encoding='utf-8',newline='\n')
index=[{'path':f.name,'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.iterdir()) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'archive':str(out),'rawFiles':len(entries),'rustScopedPassed':71,'retainedPassed':260,'retainedSkipped':3,'all993InputsMatched':True,'primaryBuildFresh':True,'crossCheckoutRawStdoutExact':True,'cliSha256':a['cliSha256'],'fullStatusCertified':False}))
