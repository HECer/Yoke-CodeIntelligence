from pathlib import Path
import hashlib,json,shutil,zipfile,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/full-native-current-20261021-r1';assessment=w/'validation/integration-20261018/full-status-preflight/full-native-current-assessment-20261021-r1'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
r=read(assessment/'report.json');full=read(raw/'result.json');frozen=read(raw/'source-after.json')
assert full['allRegisteredGroupsAttempted'] and full['sourceUnchanged'] and full['cliUnchanged'] and full['toolsUnchanged']
assert not r['compileFailures'] and not r['unexplainedRuntimeFailures'] and not r['unexplainedCommandFailures']
assert r['allRuntimeFailuresMatchPriorOs1314Fixtures'] and len(r['actualJunctionSecurityCasesPassed'])==2
assert full['examplesBuild']['exitCode']==0 and r['registeredGroups']==320 and r['totals']['failed']==10
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()==r['sourceCommit']=='98ffa7824a696c53cfa8b4bdb9ecc49cf60bcb9a'
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
assert all(sha((w/n).read_bytes())==h for n,h in frozen.items())
ui=w/'validation/integration-20261018/explorer-current-audit';ui_cleanup=read(ui/'initial-r1/cleanup.json')
assert all(ui_cleanup[k] for k in ['primaryInputsUnchanged','fixtureSourcesUnchanged','cliUnchanged','htmlUnchanged','allNativeExportJobsEmpty','ownedServerPortClosed'])
for phase in ['headless-r1','headless-r2','headless-r3','headless-r4']:
    cleanup=read(ui/phase/'result.json')
    assert all(cleanup[k] for k in ['serverClosed','sourceInputsUnchanged','nativeCliUnchanged','browserUnchanged','htmlUnchanged','allOwnBrowserProcessesGone'])
visual=read(ui/'headless-r4/client-result.json');assert visual['completed'] and visual['embeddedGraphExact'] and visual['canonicalDownload']['fullGraphExact'] and not visual['exceptions'] and len(visual['images'])==8
for entry in visual['images']:
    data=(ui/'headless-r4'/(entry['name']+'.png')).read_bytes();assert len(data)==entry['bytes'] and sha(data)==entry['sha256']
out=w/'docs/validation/integration-2026-10-21/native-baseline-current';out.mkdir(parents=True,exist_ok=False);entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for root,prefix in [(raw,'full-native'),(assessment,'assessment'),(ui,'supplemental-explorer-audit')]:
        for p in sorted(root.rglob('*')):
            if not p.is_file():continue
            assert not p.is_symlink();relative=prefix+'/'+p.relative_to(root).as_posix();data=p.read_bytes();info=zipfile.ZipInfo(relative,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16;archive.writestr(info,data);entries.append({'path':relative,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert set(archive.namelist())=={e['path'] for e in entries} and archive.testzip() is None
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
shutil.copyfile(assessment/'report.json',out/'checkpoint.json');shutil.copyfile(__file__,out/'archive-and-docs.py');t=r['totals']
(out/'README.md').write_text(f'''# Complete native default baseline at 98ffa78

All {r['registeredGroups']} Cargo-confirmed groups were attempted: {t['passed']} passed,
{t['failed']} failed and {t['ignored']} remained explicitly ignored. All three examples compile.
The ordinary locked profile runs one target and one test thread at a time.
No ignored test, fixture precondition or original timeout is changed. This run
includes the integrated community, owner-change, Claude hook and predicate modes;
their earlier scoped results are not added again to these totals.

All {r['frozenInputs']} frozen source/control inputs, selected tool binaries and the
existing primary native CLI remain unchanged. The prerequisite independently
reconciles the previous scoped source hashes with its five documentation-only
changes and pins the actual qualified primary CLI. Cargo metadata proves the
complete target inventory, including library, CLI and doctests. Examples build
but their code is not executed. Old held processes/files stay untouched. Cleanup
removes only exact newly-created completed test exe/PDB paths under the checked
build directory; preexisting artifacts and the native CLI are never removed.

Every runtime failure matches one of the ten original Windows symlink fixture
identities and actually reports OS1314. No compile failure, other unexplained
runtime failure or unexplained command failure remains. The failing
fixture source files also match the original baseline Git blobs byte-for-byte;
the checkpoint records each source hash separately. The intended symlink
security assertion is not reached in these cases, so this is a complete attempted
default suite, not passing full Windows acceptance. Both actual Junction security
cases pass again: decision import/export and directory-move provider refusal.
Junctions do not substitute for file-symlink coverage. No Developer Mode, privilege
or system setting changes. The earlier direct link-creation probe remains
historical evidence in the separate bcf442b baseline.

[checkpoint.json](checkpoint.json) lists every failed identity and the complete
scope. [raw-logs.zip](raw-logs.zip) preserves original compile JSONL, stderr,
test logs, per-group results, runner, tool/source manifests and assessment.
[raw-index.json](raw-index.json) records exact decompressed byte lengths and
SHA-256 hashes. Every member was independently verified; CRLF and trailing bytes
stay intact. Only STATUS and REMAINING_WORK documentation changes after the run;
[post-validation-docs.json](post-validation-docs.json) reconciles those hashes.

A supplemental read-only explorer audit uses only a newly created synthetic
TypeScript fixture and the unchanged qualified native CLI. Eight actual installed
Chrome156 viewport captures were visually inspected, including force transition
start/middle/settled, selection, filtered neighbors, radial layout and the scrolled
evidence panel. The complete desktop document fits that1440x900 viewport in every
capture. No test CSS is injected in this successful phase. The full embedded
graph and actual downloaded canonical JSON remain deeply equal to the original
ten nodes/sixteen observations; no browser JavaScript exception occurs.
Full source bytes and separate Code/Context/Outline tabs are absent; relations
have a select rather than verb chips. This export is intentionally offline; live
rebuild/reload needs a separate authorized workflow. Full-page snapshots failed
in the available primary automation runtime. A separately isolated browser's
captureBeyondViewport mode inflated the canvas and failed with Page is too large;
an independent no-screenshot sampling falsified normal layout growth (stable573
pixel canvas/900pixel document). Ordinary viewport capture preserves geometry.
This does not qualify other viewports/browsers, every feature, frame-rate or all
motion quality; missing tabs/live behavior remain open. The initial owned static
server required identity-checked termination after its
shutdown endpoint timed out. All1002 inputs/fixture bytes/CLI/HTML stayed exact,
its native child job ended empty and its port closed. All subsequent isolated
browser jobs end empty and their threaded servers close. No browser-skill session
was created because no extension was connected; its shared managed daemon is
retained. The helper reported an update-cache check, so this is not an OS network
isolation attestation. Original artifacts and limitations are indexed in the ZIP.

The suite result proves this exact source checkpoint, not semantic completion
of every upstream/R1-R9 contract, authenticated external services or deployment.
The user has no approved Google test document; live export remains unvalidated.
No CI polling, main merge or release occurs. The T3-owned PR watcher supplies
hosted check results separately; local test totals do not certify hosted CI.
''',encoding='utf-8',newline='\n')
def insert(name,marker,paragraph):
    p=w/name;s=p.read_text(encoding='utf-8-sig');assert s.count(marker)==1;p.write_text(s.replace(marker,marker+paragraph),encoding='utf-8',newline='\n')
paragraph=f'''
The complete native default run at `98ffa78` attempts all {r['registeredGroups']} registered
groups, including the new community, owner-change, Claude hook and predicate
code: **{t['passed']:,} passed, {t['failed']} failed and {t['ignored']} explicitly ignored**.
All three examples compile. All {r['frozenInputs']} source/control inputs, selected tools
and the existing primary CLI remain unchanged. Every failure is an original
symlink fixture with actual OS1314; there are no compile failures or additional
unexplained failures. Both actual Junction security cases pass. No test timeout,
fixture, ignore or system setting is changed. See the
[complete current-source evidence](validation/integration-2026-10-21/native-baseline-current/README.md).
This is a complete attempted default suite. Ten file-symlink prerequisites and
full Windows/R1-R9/hosted/upstream/main/release acceptance remain open. The older
bcf442b baseline and individual scoped counts below remain separately labeled.
The supplemental real-browser audit confirms the current explorer's header-only
source panel and missing Code/Context/Outline tabs, verb chips and live workflow.
It preserves legacy scope and does not certify those unavailable features.
'''
insert('docs/STATUS.md','## Current integration work\n',paragraph)
insert('docs/REMAINING_WORK.md','## Acceptance still required\n',paragraph)
changed=[]
for name,h in frozen.items():
    now=sha((w/name).read_bytes())
    if h!=now:changed.append({'path':name,'validatedSha256':h,'documentedSha256':now})
assert {x['path'] for x in changed}=={'docs/STATUS.md','docs/REMAINING_WORK.md'}
(out/'post-validation-docs.json').write_text(json.dumps({'postValidationChangesOnlyDocumentation':True,'allRuntimeInputsByteExact':True,'changedDocuments':changed},indent=2)+'\n',encoding='utf-8',newline='\n')
index=[{'path':p.name,'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.iterdir()) if p.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')
print(json.dumps({'archive':str(out),'members':len(entries),'rawBytes':sum(e['bytes'] for e in entries),'zipBytes':(out/'raw-logs.zip').stat().st_size,'totals':t,'sourceCommit':r['sourceCommit']}))
