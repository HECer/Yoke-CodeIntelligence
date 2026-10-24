from pathlib import Path
import hashlib,json,re,shutil,subprocess,zipfile
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r6-session-hooks';read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
n=read(raw/'new-target-r3/result.json');c=read(raw/'contracts-r1/result.json');p=read(raw/'primary-r1/result.json');a=read(raw/'primary-cli-r1/result.json');s=read(raw/'secondary-cli-r1/result.json')
assert all(r['allChecksPassed'] and r['sourceUnchanged'] for r in [n,c,p])
assert all(r['completed'] and r['nativeContextObservedInActualClient'] and r['stalePolicyHookRefusalObservedInActualClient'] and r['stalePolicyContextNotEmitted'] and r['providerRequests']==[] and all(x['activeAfterCleanup']==0 and x['assignedBeforeResume'] for x in r['commands']) for r in [a,s])
assert all(sha((w/name).read_bytes())==h for name,h in read(raw/'primary-r1/source-after.json').items())
assert all(sha((w/name).read_bytes())==h for name,h in read(raw/'contracts-r1/source-after.json').items())
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
totals=lambda r:{k:sum(s['totals'][k] for s in r['checks']) for k in ['passed','failed','ignored']}
assert totals(n)=={'passed':6,'failed':0,'ignored':0} and totals(c)=={'passed':200,'failed':0,'ignored':1}
log=(raw/'primary-r1/retained.log').read_text(encoding='utf-8');assert all(re.search(r'^(?:#|\u2139) '+s+r'$',log,re.M) for s in ['pass 260','fail 0','skipped 3'])
build=next(s for s in p['checks'] if s['name']=='build');assert len(build['primaryPackageArtifacts'])==2 and all(not x['fresh'] for x in build['primaryPackageArtifacts'])
shutil.copyfile(Path(__file__).with_name('ci-r6-hook-interface.py'),raw/'interface-r1/controller.py')
out=w/'docs/validation/integration-2026-10-21/claude-session-hooks';out.mkdir(parents=True,exist_ok=False);entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for f in sorted(raw.rglob('*')):
        if not f.is_file():continue
        assert not f.is_symlink();name=f.relative_to(raw).as_posix();data=f.read_bytes();info=zipfile.ZipInfo(name,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16;archive.writestr(info,data);entries.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert archive.testzip() is None and set(archive.namelist())=={e['path'] for e in entries}
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
checkpoint={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'newIntegrationTotals':totals(n),'hostRegressionTotals':totals(c),'distinctRustCases':206,'newRustCasesIncluded':9,'frozenAuxiliaryInputs':p['validatedInputCount'],'allPrimaryInputsByteExact':True,'sourceBaselineCopiedInputs':read(raw/'baseline-bytecopy-r1/result.json')['copiedTrackedInputs'],'primaryBuildArtifacts':build['primaryPackageArtifacts'],'primaryFmtBuildTypecheckPassed':True,'auxiliaryFmtAllTargetClippyPassed':True,'retainedPassed':260,'retainedSkipped':3,'primaryActualCli':a,'secondaryActualCli':s,'rustTestsRepeatedInPrimary':False,'fullCurrentNativeSuiteExecuted':False,'fullR6Certified':False,'fullWindowsCertified':False,'fullStatusCertified':False}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n',encoding='utf-8',newline='\n');shutil.copyfile(__file__,out/'archive-and-docs.py')
(out/'README.md').write_text('''# Explicit Claude SessionStart callback checkpoint

The pinned Claude Code 2.1.306 interface probe executes owned Setup and
SessionStart fixtures with an absolute executable and argument vector. Spaces,
apostrophes and Unicode remain literal. It uses private settings/state and an
owned loopback mock provider, with no login or conversation and zero observed
provider requests. This is not an OS network-isolation attestation.

The additive claude-session-hooks-v1 installer owns one external
yoke-claude-hooks.json file. It does not patch foreign arrays or adopt an
existing file, even empty or byte-identical. Current physical source, executable,
policy, configuration and journal bindings plus an unchanged installed receipt
are required by the bounded read-only callback. Hook stdin grants no authority;
transcript paths and future fields are not read, executed or echoed. Its advisory
context explicitly says snapshot freshness has not been checked and grants no
permission or execution evidence. No source Store is created.

Six new integration functions and 200 host regression cases pass:206 distinct
Rust cases and one explicitly ignored child worker. The three new actual child
process interruption functions are already included in 200. They cover partial
exit/resume/uninstall, prepared kill/rollback to absence and replaced-source
refusals without journal writes. Existing generic/Codex/JSON host/configuration
cases and format/all-target Clippy pass. Unicode, literal args, duplicate keys,
unknown event kinds, foreign cwd, stale policy/executable/source/settings, exact
LF output capacities and a stalled stdin producer are exercised. Failed initial
compile/CLI-budget phases are retained and add no accepted case counts.

All validated isolated-worktree inputs match primary byte-exactly. Its fresh
library/CLI build records primary source paths and fresh:false; primary format,
typecheck and TypeScript260/3 existing skips pass. Identical Rust inputs are not
tested again solely because the checkout changed.

Both actual binaries are selected by independently owned native installation
fixtures. Actual Claude debug evidence shows the native context was received,
then shows the static hook refusal and no context after a policy change. The
client parent exits0 even for that refusal, so parent exit status alone is never
the acceptance criterion. Source, settings and host journal files stay unchanged
during both callbacks; uninstall removes only the managed file. Each client is
assigned to its own Windows job while suspended and every job is verified empty
after cleanup. No model turn, login, real user configuration or analyzed-source
execution is requested. Both live fixtures observe zero provider requests.

[checkpoint.json](checkpoint.json) pins source/binary/procedure results.
[raw-index.json](raw-index.json) indexes every exact decompressed member of
[raw-logs.zip](raw-logs.zip); all hashes and byte counts were verified. Raw line
endings and trailing whitespace are preserved. Documentation follows runtime
qualification and is reconciled separately.

The complete bcf442b default baseline remains historical:2951 passed,10 original
OS1314 symlink-fixture failures and53 ignored. No new-head whole-native suite,
interactive/model context consumption, Cursor/Desktop runtime, statusline,
all-version/platform/upstream, full R6/R1-R9, main or release acceptance is claimed.
No Developer Mode, privilege or system setting is changed. The user has no
approved Google test document; authenticated Google export stays unvalidated.
''',encoding='utf-8',newline='\n')
def replace(name,old,new):
    f=w/name;s=f.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:70]);f.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
replace('native/docs/claude-session-hooks.md','Actual managed native\ncallback validation is recorded separately; client parsing alone is not callback\ndelivery or context consumption by a model.','Both actual managed native callback fixtures now pass: client debug evidence\nrecords the received context and, after a policy change, a static hook refusal\nwith no context. Source/settings/journal bytes stay unchanged during callbacks,\nall process jobs end empty and no provider request is observed. Parent exit0 alone\nis not an acceptance criterion. The scoped Rust guard passes206 cases/1 explicit\nchild-worker ignore; format/all-target Clippy, fresh primary build, typecheck and\nTypeScript260/3 existing skips pass. See the\n[indexed evidence](../../docs/validation/integration-2026-10-21/claude-session-hooks/README.md).\nThis does not establish context consumption by a model.')
replace('docs/native-json-hosts.md','# Managed JSON MCP clients (native 0.5.0 development)\n','# Managed JSON MCP clients (native 0.5.0 development)\n\nThe separate [Claude SessionStart settings](../native/docs/claude-session-hooks.md)\nprofile owns an entire external hook file; it does not modify these MCP entries.\n')
replace('docs/ARCHITECTURE.md','## Native Rust ownership\n','''## Native Rust ownership

The optional Claude SessionStart adapter owns an entire external settings file,
leaving foreign hook arrays alone. Literal native argv carries the original
physical bindings. A bounded unique-key stdin callback checks the current
installed receipt and source/policy/executable identities before emitting only
advisory source-scope metadata, with freshness explicitly not checked. No Store,
transcript read, model, permission decision or initial user message is created.
Existing guarded apply/recovery and old profile identities remain intact. See
the [hook contract](../native/docs/claude-session-hooks.md).
''')
replace('docs/REMAINING_WORK.md','actual isolated Claude2.1.306 managed-file health and native session semantic planning.','actual isolated Claude2.1.306 managed-file health, explicit whole-file SessionStart settings with read-only physically bound context and observed actual init-only success/stale-policy refusal, and native session semantic planning.')
replace('docs/REMAINING_WORK.md','further branded schemas, hook/statusline adapters and remaining platform qualification.','further branded schemas, broader hook events, interactive context consumption, statusline adapters and remaining platform qualification.')
replace('docs/STATUS.md','## Current integration work\n','''## Current integration work

The optional [Claude SessionStart settings](../native/docs/claude-session-hooks.md)
profile now owns one external JSON file through the guarded installer. It does
not adopt existing files or edit foreign hook arrays. Literal executable/args,
original physical bindings and current unchanged receipts govern the bounded
read-only callback. Hook input never grants authority or selects transcripts;
the advisory explicitly marks snapshot freshness as not checked. No source Store,
model, Git, permission decision or initial user message is created.

The scoped guard passes206 Rust cases/1 explicit child-worker ignore, including
six new integrations and three new actual process-interruption functions. Format,
all-target Clippy, fresh primary library/CLI build, typecheck and TypeScript260/3
existing skips pass; every validated input matches primary byte-exactly. Both
actual Claude2.1.306 init-only fixtures receive the native context and observe a
static refusal with no context after a policy change. Parent exit0 alone is not
accepted. Callback source/settings/journal bytes remain unchanged, process jobs
end empty and zero provider requests are observed; no login/conversation occurs.
See the [exact evidence](validation/integration-2026-10-21/claude-session-hooks/README.md).
Interactive/model context use, other versions/runtimes, statuslines and complete
R6/R1-R9/platform/upstream/hosted/main/release acceptance remain open. The full
bcf442b native baseline predates this addition and remains separately labeled.
''')
frozen=read(raw/'primary-r1/source-after.json');changed=[]
for name,h in frozen.items():
    now=sha((w/name).read_bytes())
    if now!=h:changed.append({'path':name,'validatedSha256':h,'documentedSha256':now})
assert {x['path'] for x in changed}=={'native/docs/claude-session-hooks.md','docs/native-json-hosts.md','docs/ARCHITECTURE.md','docs/REMAINING_WORK.md','docs/STATUS.md'}
(out/'post-validation-docs.json').write_text(json.dumps({'postValidationChangesOnlyDocumentation':True,'allRuntimeInputsByteExact':True,'changedDocuments':changed},indent=2)+'\n',encoding='utf-8',newline='\n')
index=[{'path':f.name,'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.iterdir()) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'archive':str(out),'rawFiles':len(entries),'rustScopedPassed':206,'rustIgnored':1,'retainedPassed':260,'retainedSkipped':3,'allInputsMatched':p['validatedInputCount'],'primaryBuildFresh':True,'cliSha256':a['cliSha256'],'fullStatusCertified':False}))
