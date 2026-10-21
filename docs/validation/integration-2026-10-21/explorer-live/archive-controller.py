from pathlib import Path
import json,hashlib,zipfile,subprocess,re,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018';out=w/'docs/validation/integration-2026-10-21/explorer-live';out.mkdir(exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip();assert head=='bf00ddc07743ab8b277baf221a970c66dbc55d74'
source=json.loads((raw/'explorer-live-r3/source-before.json').read_text());assert all(sha((w/n).read_bytes())==h for n,h in source.items())
for phase in ['explorer-live-r1','explorer-live-r2','explorer-live-r3']:
    result=json.loads((raw/phase/'result.json').read_text());assert result['complete'] and result['sourceInputsUnchanged'] and all(r['exitCode']==0 for r in result['rows'])
v=json.loads((raw/'explorer-live-r3/visual-r1/client-result.json').read_text(encoding='utf8'));s=json.loads((raw/'explorer-live-r3/visual-r1/result.json').read_text(encoding='utf8'));legacy=json.loads((raw/'explorer-live-r3/legacy-control/result.json').read_text())
assert v['completed'] and len(v['images'])==11 and not v['exceptions'] and all(v[k] for k in ['actualPendingStateVisible','parentCannotReadSandboxSource','hostileSourceInert','fullPageReloadPreservesCapture','hostGrantCannotBeUpgraded','mobileWidthExact','fixtureWritesOnlyExplicitVersions']) and v['sandboxCanonicalDownload']['exact']
assert all(s[k] for k in ['allOwnJobsEmpty','servicePortsClosed','sourceInputsUnchanged','nativeCliUnchanged','browserUnchanged']) and legacy['actualLegacyHtmlByteExact']
a=json.loads((raw/'explorer-live-r2/source-before.json').read_text());assert [n for n in a if a[n]!=source[n]]==['native/src/explorer_live.html']
code=['native/src/lib.rs','native/src/main.rs','native/src/explorer_http.rs','native/src/explorer_live.html','native/tests/explorer_http.rs']
assert all(subprocess.check_output(['git','show',head+':'+n],cwd=w)==(w/n).read_bytes() for n in code)
phases=['explorer-live-r1','explorer-live-r2','explorer-live-r3'];capabilities=[]
for phase in phases:
    for p in (raw/phase).rglob('ready.json'):
        r=json.loads(p.read_text(encoding='utf8'));private=json.loads(Path(r['privateReadyPath']).read_text(encoding='utf8'));capabilities.extend(server['url'].split('#')[1] for server in private['servers'].values())
members=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for phase in phases:
        for p in sorted((raw/phase).rglob('*')):
            if not p.is_file():continue
            assert not p.is_symlink();data=p.read_bytes()
            assert all(cap.encode() not in data and cap.encode('utf-16le') not in data for cap in capabilities),'A private test capability would be published'
            assert not re.search(rb'http://127\.0\.0\.1:[0-9]+/#[a-f0-9]{64}',data),'Private capability URL would be published'
            name=p.relative_to(raw).as_posix();z.writestr(name,data);members.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as z:
    assert set(z.namelist())=={m['path'] for m in members}
    for m in members:
        b=z.read(m['path']);assert len(b)==m['bytes'] and sha(b)==m['sha256']
(out/'raw-index.json').write_text(json.dumps({'zipSha256':sha((out/'raw-logs.zip').read_bytes()),'members':members},indent=2)+'\n')
checkpoint={'sourceCommit':head,'sourceInputs':len(source),'codeFiles':{n:source[n] for n in code},'nativeCliSha256':v['nativeCliSha256'],'rustPassed':31,'newLiveRustPassed':6,'typeScript':{'passed':260,'existingSkips':3},'allTargetClippyPassed':True,'finalDeltaFromWholeScopedChecks':'Only two live-HTML access-state labels; Rust source and tests unchanged','finalDeltaChecks':['rebuilt actual CLI','six live HTTP cases','fmt','JavaScript syntax','eleven real browser images and functional controls'],'realVisualRounds':5,'finalImagesVisuallyInspected':11,'actualPendingRequestVisuallyChecked':True,'sandboxCanonicalDownload':v['sandboxCanonicalDownload'],'legacyHtmlSha256':legacy['legacyHtmlSha256'],'rawMembers':len(members),'rawBytes':sum(m['bytes'] for m in members),'rawZipBytes':(out/'raw-logs.zip').stat().st_size,'privateCapabilityValuesScanned':len(capabilities),'privateCapabilityValuesPublished':0,'privateProcessStdoutStderrReadyAndProfilesExcluded':True,'wholeNativeBaselineRerun':False,'fullWindowsAccepted':False,'fullStatusAccepted':False,'systemSettingsChanged':False,'analyzedSourceExecuted':False,'modelRequested':False,'automaticWatchImplemented':False}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n')
(out/'README.md').write_text('''# Authenticated native live Explorer checkpoint

`yoke --root /project explorer` starts a private loopback saved-source view.
`--allow-rebuild` separately enables manual source capture. The private fragment
capability becomes a Bearer header; request data cannot grant host permission,
choose another root or widen capture options. Reload reads the cached view only.
Rebuild requires exact expected snapshot and revision, creates an immutable native
capture and replaces the displayed server view only after checked source export.
Unchanged captures still advance the revision. Failures retain the previous view;
a capture may already have been persisted before an export failure.

The bounded single-engine transport checks Host/Origin, HTTP1.1, duplicate headers,
framing, content type, complete1024-byte request bounds, closed/duplicate-free JSON
and deadline/response budgets. A separate at-most1000ms admission bound prevents
idle speculative browser sockets from consuming a longer capture deadline.
Bootstrap and refusal responses expose no source/snapshot headers. Authenticated
views preserve original graph/source identities and source export limits.
The generated source graph runs in an opaque sandboxed iframe, has no parent DOM
access and retains its offline connect-src none policy. Static exports stay exact.

The scoped second guard passes31 Rust cases (six new live cases, five source-export
cases and twenty original export/editor HTTP regressions), all-target Clippy,
format, typecheck and npm test260/3 existing skips. The final delta changes only
two access-state labels in live HTML; Rust source/test bytes are unchanged. The
actual native binary is rebuilt and all six live tests, syntax/format and final
browser controls pass again. The old entire-native baseline is not rerun or
relabeled for this addition. Its ten original OS1314 fixtures remain unchanged.

Five actual isolated Chrome156 rounds preserve the causal work. Initial loading
failed because an unused browser preconnection occupied the listener; a real
regression test fails with timeout before the admission-bound fix and passes
afterward. Its PowerShell-captured diagnostic transcript is preserved in its
original encoding. Later page-reload automation had to follow the source iframe
into its new DevTools session; no sandbox restriction was relaxed. A final visible
missing-capability label is corrected and regenerated. Initial immediate port
cleanup probing had one false result; the original record and later verified
closed-port postflight both remain. All final jobs end empty and service ports close.

All eleven final PNGs are visually inspected: initial source, reload of old saved
bytes after a disk edit, actual pending rebuild, new source, unchanged capture,
stale revision refusal, explicit reload, full page reload, mobile, no host rebuild
grant and missing capability. For the pending-state image the controller briefly
holds a real request before dispatch and then releases it; there is no synthetic
response or product CSS override. The live workflow preserves literal Unicode/
BOM/CRLF and hostile text, exact original graph on reload, immutable old captures,
zero JavaScript exceptions and a deeply equal actual canonical JSON download from
the sandbox. Source remains inert, and one client cannot upgrade the host grant.
The final server and browser jobs all end empty. No model/analyzed source executes.

Native CLI stdout containing private capability URLs, browser/private process logs,
private ready files and fresh profiles stay outside the published evidence tree.
Public diagnostic reports redact capabilities and record no Authorization values.
Every archived file is checked against all ten owned ephemeral capability values
in UTF8 and UTF16LE and against capability-URL syntax. Zero values are published.
The ZIP preserves the included files byte-exactly; it is not all private process
stdio. T3 preview twice timed out15000ms, so owned fresh Chrome supplied the fallback.
No user browser profile, credentials, Google document or global setting is used.

[checkpoint.json](checkpoint.json) pins code/CLI scope and limitations.
[raw-index.json](raw-index.json) indexes the lossless included [evidence ZIP](raw-logs.zip).
Post-validation docs are mapped separately. An actual legacy fixture still exports
HTML byte-identical to SHA256c3b4e6bbf9b63ce55a35e5cb2dfaf16237d64723e4aeb0f5051f0ef424a4f4e0.
Broader browsers/accessibility/upstream parity, automatic watching, full STATUS/
R1-R9, Windows, hosted/main/release and authenticated Google acceptance remain open.
''',encoding='utf8',newline='\n')
shutil.copyfile(__file__,out/'archive-controller.py')
print(json.dumps({k:checkpoint[k] for k in ['sourceCommit','rawMembers','rawBytes','rawZipBytes','privateCapabilityValuesScanned','privateCapabilityValuesPublished']}))
