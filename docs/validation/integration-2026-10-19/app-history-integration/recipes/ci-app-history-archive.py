from pathlib import Path
import subprocess,json,hashlib,copy,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach';raw=root/'validation/integration-20261018/app-history-integration'
out=work/'docs/validation/integration-2026-10-19/app-history-integration'
assert not out.exists();out.mkdir(parents=True)
sha=lambda b:hashlib.sha256(b).hexdigest()
def write(name,b):
 p=out/name;p.parent.mkdir(parents=True,exist_ok=True)
 if p.exists():assert p.read_bytes()==b,name
 else:p.write_bytes(b)
scope=json.loads((raw/'final-scope.json').read_text());head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=work).decode().strip();assert head==scope['sourceCommit']
final=json.loads((raw/'final-r2/source-before.json').read_text())
for name,digest in final.items():assert sha((work/name).read_bytes())==digest,name
for phase in ['native-final-r5','native-job-output-r6','live-cli-r3']:
 r=json.loads((raw/phase/'result.json').read_text());assert r['allChecksPassed'] and r['sourceUnchanged'];assert json.loads((raw/phase/'source-before.json').read_text())==final
assert json.loads((raw/'retained-r2/result.json').read_text())['allChecksPassed']
phase_records=[];source_links=[]
for phase in sorted(raw.iterdir()):
 if not phase.is_dir() or not (phase/'result.json').is_file():continue
 result=json.loads((phase/'result.json').read_text());before=json.loads((phase/'source-before.json').read_text())
 phase_records.append({'phase':phase.name,'resultPath':'runs/'+phase.name+'/result.json','allChecksPassed':result['allChecksPassed'],'inputs':len(before),'scope':'Exact recorded commands; completed-result totals exclude any target that aborted before its summary.'})
 for p in sorted(phase.rglob('*')):
  if not p.is_file():continue
  name=p.relative_to(phase)
  if name.parts[0]=='source':
   source_name='/'.join(name.parts[1:]);b=p.read_bytes();digest=sha(b)
   assert before.get(source_name)==digest,(phase.name,source_name)
   variant='source-variants/'+digest+p.suffix
   write(variant,b);source_links.append({'phase':phase.name,'sourcePath':source_name,'sha256':digest,'archivePath':variant,'origin':'exact raw frozen source copy'})
  else:write('runs/'+phase.name+'/'+name.as_posix(),p.read_bytes())
# Always retain every currently changed native source/test/doc in exact final bytes.
tracked=subprocess.check_output(['git','diff','--name-only','a6bbe2c',head,'--','native'],cwd=work).decode().splitlines()
for name in tracked:
 b=(work/name).read_bytes();digest=sha(b);assert final.get(name)==digest,name
 variant='source-variants/'+digest+Path(name).suffix;write(variant,b)
 source_links.append({'phase':'final-checked-source','sourcePath':name,'sha256':digest,'archivePath':variant,'origin':'exact final checkout bytes; matches all final native manifests'})
write('source-copies.json',(json.dumps(source_links,indent=2)+'\n').encode())
write('phases.json',(json.dumps(phase_records,indent=2)+'\n').encode())
for name in ['final-scope.json','inactive-cache-cleanup-1.jsonl','held-cache-relocation.json']:
 write('receipts/'+name,(raw/name).read_bytes())
# Source names not captured by early runners remain honestly hash-only.
coverage=[]
relevant=set(tracked)|{'native/tests/github_app_worker_history.rs','native/tests/support/github_app_worker_history_fixture.rs'}
for phase in phase_records:
 before=json.loads((raw/phase['phase']/'source-before.json').read_text())
 provided={(s['sourcePath'],s['sha256']) for s in source_links if s['phase']==phase['phase']}
 for name in sorted(relevant&before.keys()):
  if (name,before[name]) not in provided:
   matching=next((s['archivePath'] for s in source_links if s['sourcePath']==name and s['sha256']==before[name]),None)
   coverage.append({'phase':phase['phase'],'sourcePath':name,'sha256':before[name],'rawArchivePath':matching,'origin':'matching raw copy from another phase' if matching else 'hash-only; older raw variant was not captured'})
write('additional-source-coverage.json',(json.dumps(coverage,indent=2)+'\n').encode())
old_raw=subprocess.check_output(['git','show','a6bbe2c:docs/parity/native-reconciliation-2026-10-15.json'],cwd=work)
new_raw=(work/'docs/parity/native-reconciliation-2026-10-15.json').read_bytes();old=json.loads(old_raw);new=json.loads(new_raw)
def row(v):
 if isinstance(v,dict):
  if v.get('id')=='R8':return v
  for child in v.values():
   result=row(child)
   if result is not None:return result
 elif isinstance(v,list):
  for child in v:
   result=row(child)
   if result is not None:return result
 return None
restored=copy.deepcopy(new);r=row(restored);assert r is not None;r.clear();r.update(copy.deepcopy(row(old)));assert restored==old
allowed={'missingBehavior','suggestedScope','appHistoryDevelopment'}
assert all(row(new).get(k)==v for k,v in row(old).items() if k not in allowed)
write('tracker/before.json',old_raw);write('tracker/after.json',new_raw)
write('tracker/proof.json',(json.dumps({'onlyR8Changed':True,'changedR8Fields':sorted(allowed),'allOtherRowsInventoryAndCertificationFlagsPreserved':True,'beforeSha256':sha(old_raw),'afterSha256':sha(new_raw)},indent=2)+'\n').encode())
pg='native/tests/fixtures/postgres-catalog-response.jsonl';pg_bytes=(work/pg).read_bytes()
assert sha(pg_bytes)=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
assert subprocess.check_output(['git','show',head+':'+pg],cwd=work)==pg_bytes
write('receipts/postgres-fixture.json',(json.dumps({'path':pg,'bytes':len(pg_bytes),'sha256':sha(pg_bytes),'committedAndWorkingBytesIdentical':True},indent=2)+'\n').encode())
prior=subprocess.check_output(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body,headRefOid,state,isDraft,url'],cwd=work)
prior_obj=json.loads(prior);assert prior_obj['headRefOid']=='a6bbe2c4d6cae58f6ce3118d0f0fdb34ff01ea34'
write('receipts/prior-pr.json',prior)
for name in ['ci-app-history-source-commit.py','ci-app-history-tracker.py','ci-app-history-archive.py','ci-app-history-cache.ps1','ci-app-history-held-cache.ps1','ci-app-history-held-linker.ps1']:
 write('recipes/'+name,(Path(__file__).parent/name).read_bytes())
readme='''# App-bound history integration evidence

Checked source: `'''+head+'''`, based on integration `a6bbe2c`.
The locally preserved WIP ancestors are development history, not independent
acceptance. The checked source adds signed original-plan bindings, authenticated
worker/Git replay, exact queue/native fences, durable bounded execution from the
worker CLI, existing-only read-only management/status and typed retention.
Production deployment, receiver-side independent source verification/reasoning,
full upstream equivalence and whole-platform acceptance remain open.

| Final check | Actual result |
| --- | --- |
| 13 ordinary Cargo test targets | 287 passed, 0 failed, 6 default ignored |
| Unchanged digest-history-job target, explicit test profile | 13 passed, 0 failed |
| Combined scoped native targets | 300 passed, 0 failed, 6 default ignored, 14 groups |
| Format, build, all-target Clippy `-D warnings` | Passed on the same 758 native input hashes |
| Actual Windows CLI/Credential Manager gate | Passed; source and both binary hashes unchanged |
| Temporary owned signing credential | Removed, absence independently verified by outer controller |
| Retained checks, 759 unchanged runtime inputs | Python17, typecheck, TypeScript260 / 3 skipped pass |

The digest-history-job target used `cargo rustc --profile test` and only an
additional output suffix (`-C extra-filename=-r8-acquisition`), followed by its
normal native test harness. Windows retained the crashed MSVC linker PID22940
and denied termination; the original output path remained locked. No test source,
assertion, permission gate or recovery rule was changed for this execution.
The standard Cargo-path failure is retained, and this does not establish a
whole-native or standard whole-platform pass. The default ignored counts are
preserved even though one separately selected CLI gate was executed explicitly.

The tests exercise actual local Git objects and a host-owned loopback App
transport, not a configured production App. They cover closed input and permission
ordering; exact CRLF/Unicode source; source kind refusal; idempotent signed binding;
current versus historical selection; stale and future initial revisions; consistent
edits to both saved plans; key rotation and current grant denial; missing sidecars
and native jobs; output/SQL rollback; restart/offline read-only status; typed retention;
and worker replay without repeating HTTP or adopting another revision. The real
CLI gate includes lost stdout followed by explicit status/recovery. It owns no
real account secret: its temporary public fixture key is created/deleted outside
the potentially terminated test process.

Earlier failures are retained: the initial nonexistent Snapshot field (zero groups),
incorrect fixture HTTP200 instead of201, empty/probe and inherited nonblocking
socket handling, a double-panic process abort, a test-only private-field compile
error, a nonexistent test-target name, and linker/cache failures. A deliberately
failing revision test first proved that an invalid initial history revision could
publish both history rows; the final source checks that fence inside creation's
transaction. Prior durable worker completion remains a separate phase.

The final native source manifests are identical across quality, ordinary targets,
the separately named target and the CLI gate. All final changed native source
bytes are archived; additional historic source variants are provided when they
were captured. `additional-source-coverage.json` explicitly identifies older
hash-only variants instead of inventing missing bytes. Completed-result totals
do not include a target that aborted before its summary. Raw logs/commands and
before/after manifests retain those distinctions.

`tracker/proof.json` proves that only the R8 development fields changed; inventory,
other rows and certification flags remain exact. The protected PostgreSQL fixture
retains its original 2,779 bytes and SHA-256. Cache cleanup removed only old inactive
owned executable/PDB cache files after recording their hashes; no source/evidence
was removed. One crashed test image was preserved under a separate cache filename.
The attempted linker-image relocation was refused before mutation because its
bytes could not be read. Historic recipes reference completed phase directories
and must not be blindly rerun. They carry no permission to deploy or publish.

`index.json` hashes every artifact in this directory except itself. PR metadata
shown here is the prior description; post-push remote verification is recorded
separately in the ignored local validation receipt. No release receipt is claimed.
'''
write('README.md',readme.encode())
records=[]
for p in sorted(out.rglob('*')):
 if p.is_file():b=p.read_bytes();records.append({'path':p.relative_to(out).as_posix(),'bytes':len(b),'sha256':sha(b)})
index={'schemaVersion':1,'kind':'app-bound-history-integration','sourceCommit':head,'baseIntegration':scope['base'],'scope':scope,'phases':phase_records,'artifacts':records}
write('index.json',(json.dumps(index,indent=2)+'\n').encode())
print(json.dumps({'artifacts':len(records),'phases':len(phase_records),'sourceCopies':len(source_links),'hashOnlyVariants':sum(x['rawArchivePath'] is None for x in coverage),'indexSha256':sha((out/'index.json').read_bytes())}))
