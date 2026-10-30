from pathlib import Path
import json,hashlib,subprocess,shutil,datetime,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/postgres-catalog';out=root/'docs/validation/integration-2026-10-19/postgres-catalog';out.mkdir(exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
native=json.loads((base/'r8/result.json').read_bytes());quality=json.loads((base/'quality-r2/result.json').read_bytes());integrated=json.loads((base/'integrated/result.json').read_bytes());source=json.loads((base/'source-integration.json').read_bytes())
assert native['sourceUnchanged'] and quality['allChecksPassed'] and quality['sourceUnchanged'] and integrated['allChecksPassed'] and integrated['sourceUnchanged']
for folder in sorted(base.iterdir()):
    if folder.is_dir():shutil.copytree(folder,out/folder.name)
shutil.copyfile(base/'source-integration.json',out/'source-integration.json')
temporary=Path.home()/'AppData/Local/Temp';helpers=['tooling','baseline','red','r2','r3','r4','r5','r6','r7','r8','quality','quality-r2','live','tls','tls-r2','tls-r3','tls-r4','tls-unicode-r1','tls-final-r1','tls-final-r3','weird-owner','weird-r2','weird-final-r1','weird-final-r2','prompt-diagnostic','deadline','integrated','commit-source','preserve']
for name in helpers:
    file=temporary/('ci-pg-'+name+'.py');assert file.is_file(),file;target=out/'runners'/file.name;target.parent.mkdir(exist_ok=True);shutil.copyfile(file,target)
for name in ['Cargo.toml','Cargo.lock','src/main.rs']:
    target=out/'runners/fixture-certgen'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(temporary/'ci-pg-certgen'/name,target)
receipt=json.loads((Path.home()/'AppData/Local/YokeValidation/postgres-investigation/tooling-receipt.json').read_bytes())
(out/'tooling-receipt.json').write_bytes((json.dumps(receipt,indent=2)+'\n').encode())
tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';shutil.copyfile(tracker,out/'frozen-tracker.json')
index={'schemaVersion':1,'kind':'native_postgres_catalog_scoped_evidence','asOf':datetime.datetime.now(datetime.timezone.utc).isoformat(),'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'parentCommit':source['parentCommit'],'nativeTotals':source['nativeTotals'],'nativeResultGroups':9,'newContractCases':{'receivedData':8,'cli':4,'credentialAdmission':1,'uriParser':1},'frozenNativeInputs':native['inputs'],'frozenIntegratedInputs':integrated['frozenInputs'],'integratedNativeInputsMatched':integrated['nativeInputsMatched'],'wholeNativeRerun':False,'fullUpstreamContractCertified':False,'fullParityCertified':False,'wholeTaskComplete':False,'mainMerged':False,'releasePublished':False,'serverVersionsActuallyTested':['18.6'],'clientVersionsActuallyTested':['18.6'],'nonWindowsLiveValidated':False,'maximumSizeQualification':False,'attemptNotes':{
'red':'Initial received-data API returned unavailable; the real client fixture contract failed. Frozen stub/source/log retained.',
'r3':'Compiler rejected a path-validation match arm returning String instead of unit; corrected before another frozen attempt.',
'r6':'476 passed, 2 failed, 11 ignored: unchanged archive CLI OS1314 fixture plus genuine new CLI extra-field acceptance. Serde tagged unit variants were replaced by closed empty struct variants.',
'r7':'479 passed, 1 unchanged archive CLI OS1314 failure, 11 ignored; this source still used the console-opening password prompt before the final non-console fallback correction.',
'tls-r1':'Fixture TCP readiness raced PostgreSQL SQL readiness; setup failed and server shutdown log retained. Not a product acceptance result.',
'tls-r2':'Explicit empty sslcrldir made live certificate verification fail. This was a real transport configuration defect.',
'tls-r3':'Direct libpq with/without empty CRL directory reproduced the failure; native failure retained. Isolated HOME/APPDATA avoids ambient CRLs without enabling an empty CRL directory.',
'quoted-endpoint-r1':'Reference harness narrow Windows argv lost the Unicode database name before seeding. Not a native product result.',
'quoted-endpoint-r2':'An ASCII percent-encoded reference URI reached the owned Unicode database; native narrow argv still lost its name. Fixed-key individually encoded URI values corrected the native path.',
'tls-unicode-r1':'Native PGPASSWORD narrow Windows conversion failed a real Unicode password authentication.',
'tls-final-r1':'Native -W opened its Windows console stream and reached the shared deadline. The direct owned prompt diagnostic proved PostgreSQL18 OSTYPE=msys stdin fallback. Windows now uses that private UTF-8 pipe; other hosts use private UTF-8 environment with prompting disabled.',
'prompt-diagnostic':'Direct real TLS/SCRAM/channel-binding query with Unicode private stdin and explicit non-console fallback passed. This diagnostic exits after its finally cleanup, so no source-after/result receipt is claimed.',
'r8':'Final frozen nine-group native run: 479 passed, one unchanged archive CLI OS1314 fixture failure, 11 existing ignored gates. All14 new contracts pass; earlier attempts are nonadditive.',
'quality-r2':'All-target Clippy with warnings denied and format pass on the final frozen native source; earlier lint required only moving a test module after production items.'},
'notes':['Native Rust owns fixed read-only serializable catalog SQL, typed received-data archive/replay/graph/query, authority gates and process supervision. The optional trusted psql18 installation is not a Graphify application wrapper; no GPL application code is copied.',
'Actual PostgreSQL18.6 captures retain overloaded routine OIDs, parallel declared directed FK constraint OIDs, ordered composite column metadata, hidden/available definition states and exact received UTF-8 JSONL record ranges. Columns are not nodes. View trees and stored routine bodies are not original SQL/DDL source. The raising must_not_run routine is captured without execution.',
'Final real TLS/SCRAM/channel-binding capture succeeds with Unicode password. Wrong TLS hostname, server skipping SCRAM, missing credential, missing host permissions, wrong executable hash, existing output and row quota are refused. Hostile ambient PG/psqlrc settings are ignored. Lost stdout keeps the published artifact and reports its identities. Three exclusively owned OS fixture credentials are removed; the runner checks its password did not enter artifacts.',
'Final literal-loopback capture succeeds with Unicode, equals, quote and backslash database/role names and visible owner definitions. Frozen outgoing traversal succeeds after real database shutdown. A separately labeled stalled TCP fixture exceeds its 1500ms deadline at about1.55s, with peer closure and no output file.',
'Imported capture claims remain nested unverified data. Structural replay, a response hash, read-only/TLS metadata and OIDs do not attest server truth, source authority, semantic identity or observed execution. Projection network counters are scoped to received-byte replay; live capture requires separate host network authority.',
'Hard quotas are independent admissions, not fully populated maximum-size qualification. No libpq internal allocation/frame, server memory, runtime DLL attestation, OS sandbox, automatic model/project execution or query-time fetch guarantee is invented.',
'Mandatory integrated typecheck, TypeScript tests and Python contracts run with unchanged inputs; exact commands/counts remain in raw logs. Selected native/Windows evidence does not replace earlier whole-native/platform failures or certify PostgreSQL14–17/non-Windows/upstream equivalence. Wider R7 connectors and STATUS/R1–R9 remain active.'],
'artifacts':[]}
for file in sorted(out.rglob('*')):
    if file.is_file():
        raw=file.read_bytes();index['artifacts'].append({'path':file.relative_to(out).as_posix(),'bytes':len(raw),'sha256':sha(raw)})
(out/'index.json').write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode())
print(json.dumps({'artifacts':len(index['artifacts']),'indexSha256':sha((out/'index.json').read_bytes()),'nativeTotals':index['nativeTotals'],'integratedInputs':index['frozenIntegratedInputs']}))
