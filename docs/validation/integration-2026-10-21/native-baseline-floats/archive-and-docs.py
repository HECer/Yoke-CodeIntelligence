from pathlib import Path
import hashlib,json,shutil,zipfile,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
raw=w/'validation/integration-20261018/full-native-float-20261021-r1'
assessment=w/'validation/integration-20261018/full-status-preflight/full-native-float-assessment-20261021-r1'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
sha=lambda b:hashlib.sha256(b).hexdigest()
r=read(assessment/'report.json');full=read(raw/'result.json')
assert full['allRegisteredGroupsAttempted'] and full['sourceUnchanged'] and full['cliUnchanged'] and full['toolsUnchanged']
assert not r['compileFailures'] and not r['unexplainedRuntimeFailures'] and not r['unexplainedCommandFailures']
assert r['allRuntimeFailuresMatchPriorOs1314Fixtures'] and len(r['actualJunctionSecurityCasesPassed'])==2
assert full['examplesBuild']['exitCode']==0
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip()==r['sourceCommit']
assert not subprocess.check_output(['git','status','--porcelain'],cwd=w).strip()
assert all(sha((w/n).read_bytes())==h for n,h in read(raw/'source-after.json').items())
out=w/'docs/validation/integration-2026-10-21/native-baseline-floats';out.mkdir(parents=True,exist_ok=False)
entries=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
    for root,prefix in [(raw,'full-native'),(assessment,'assessment')]:
        for p in sorted(root.rglob('*')):
            if not p.is_file():continue
            assert not p.is_symlink()
            relative=prefix+'/'+p.relative_to(root).as_posix();data=p.read_bytes()
            info=zipfile.ZipInfo(relative,date_time=(2026,10,21,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16
            archive.writestr(info,data)
            entries.append({'path':relative,'bytes':len(data),'sha256':sha(data)})
    p=Path(r'C:\Users\Lomi\AppData\Local\Temp\yoke-owned-link-probe-60352f6b5c4e48eab9cae956e55d6e92\probe.json');data=p.read_bytes();archive.writestr('host-link-probe/probe.json',data);entries.append({'path':'host-link-probe/probe.json','bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as archive:
    assert set(archive.namelist())=={e['path'] for e in entries}
    assert archive.testzip() is None
    for e in entries:
        data=archive.read(e['path']);assert len(data)==e['bytes'] and sha(data)==e['sha256']
(out/'raw-index.json').write_text(json.dumps({'files':entries,'rawMembersByteExact':True,'archiveSha256':sha((out/'raw-logs.zip').read_bytes())},indent=2)+'\n',encoding='utf-8',newline='\n')
shutil.copyfile(assessment/'report.json',out/'checkpoint.json')
shutil.copyfile(__file__,out/'archive-and-docs.py')
t=r['totals']
text=f'''# Complete native default baseline at bcf442b

All {r['registeredGroups']} registered groups were attempted: {t['passed']} passed,
{t['failed']} failed and {t['ignored']} remained explicitly ignored. Examples compile.
All {r['frozenInputs']} frozen inputs, tool binaries and the existing native CLI
remain unchanged throughout the run. Cargo used the ordinary locked test profile,
one target at a time and one test thread; no ignored case became a default pass.
The digest-history group compiles and all thirteen tests pass in the ordinary
profile without an alternate output flag. Earlier held processes/files remain
untouched; the parser feature changes the dependency-based output hash.

Every runtime failure is one of the original Windows symlink fixture cases and
shows OS1314 before the intended security assertion. There are no compile failures
or additional unexplained runtime failures. The [checkpoint](checkpoint.json)
lists each case separately. This is a complete attempted default suite, not a
passing Windows acceptance or a completed R1-R9 claim.

The user requested progress without systemwide Developer Mode changes. A direct
owned probe creates a directory Junction and file hardlink successfully, while
CreateSymbolicLinkW fails with 1314 for both file/directory kinds, with and without
the unprivileged-create flag. The current token lacks the symlink privilege.
Existing links can be used independently of permission to create new ones.
Both actual Junction security cases pass in this same full suite: decision
import/export and provider-start refusal for directory moves. Junctions and
hardlinks do not substitute for file-symlink test coverage. No setting or privilege
was changed; new probe links were removed and target bytes stayed intact.

[raw-logs.zip](raw-logs.zip) contains the original compile JSONL, stderr, test
logs, per-group results, runner, source/tool manifests and assessment. Its members
are indexed by path, byte length and SHA-256 in [raw-index.json](raw-index.json).
Every decompressed member was verified against its original bytes, including
trailing whitespace. ZIP compression avoids copying repeated compiler metadata
uncompressed into the checkout. ZIP verification only reads member bytes.

This baseline qualifies the shared float-roundtrip feature at bcf442b alongside
the earlier scoped retention/JSON tests. New community-policy work, authenticated
Google export without an approved test document, other runtime/upstream/hosted/
platform gates, main integration and release remain separately open.
'''
(out/'README.md').write_text(text,encoding='utf-8',newline='\n')
index=[{'path':p.name,'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in sorted(out.iterdir()) if p.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'rawLogsCompressedWithoutModification':True},indent=2)+'\n',encoding='utf-8',newline='\n')
p=w/'docs/STATUS.md';s=p.read_text(encoding='utf-8-sig')
old='The shared parser feature still needs a complete native recheck; broader R3\ncommunity policy, R1-R9/platform/hosted/upstream acceptance remain open.'
new='The complete native recheck below records the shared parser feature; broader R3\ncommunity policy, R1-R9/platform/hosted/upstream acceptance remain open.'
assert s.count(old)==1;s=s.replace(old,new)
marker='## Current integration work\n';assert s.count(marker)==1
paragraph=f'''\nThe complete native default recheck at `bcf442b` attempts all {r['registeredGroups']} registered\ngroups: **{t['passed']:,} passed, {t['failed']} failed and {t['ignored']} explicitly ignored**. Examples compile,\nand all {r['frozenInputs']} inputs, tools and the CLI remain unchanged. Every failure is an\noriginal Windows symlink fixture with OS1314; no compile failures remain. The\ndigest-history group now passes normally without an alternate output flag.\nActual Junction import/export and directory-move security cases also pass. The\nuser requested progress without enabling Developer Mode; no system setting is\nchanged. File-symlink acceptance remains open and is not replaced by Junctions.\nSee the [complete baseline evidence](validation/integration-2026-10-21/native-baseline-floats/README.md).\nThis is not complete Windows/R1-R9, hosted, main or release acceptance.\n'''
s=s.replace(marker,marker+paragraph);p.write_text(s,encoding='utf-8',newline='\n')
print(json.dumps({'archive':str(out),'members':len(entries),'rawBytes':sum(e['bytes'] for e in entries),'zipBytes':(out/'raw-logs.zip').stat().st_size,'totals':t,'sourceCommit':r['sourceCommit']}))
