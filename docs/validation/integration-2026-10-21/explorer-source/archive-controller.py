from pathlib import Path
import hashlib,json,subprocess,zipfile,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018';out=w/'docs/validation/integration-2026-10-21/explorer-source';out.mkdir(exist_ok=True);assert not any(out.iterdir())
sha=lambda b:hashlib.sha256(b).hexdigest();run=lambda *args:subprocess.check_output(args,cwd=w).decode().strip();head=run('git','rev-parse','HEAD')
source=json.loads((raw/'explorer-source-r3/source-before.json').read_text());assert all(sha((w/n).read_bytes())==h for n,h in source.items())
for phase in ['explorer-source-r1','explorer-source-r2','explorer-source-r3']:
    result=json.loads((raw/phase/'result.json').read_text());assert result['complete'] and result['sourceInputsUnchanged']
    for row in result['rows']:assert row['exitCode']==0
visual=json.loads((raw/'explorer-source-r3/visual-r3/client-result.json').read_text(encoding='utf8'));server=json.loads((raw/'explorer-source-r3/visual-r3/result.json').read_text(encoding='utf8'));legacy=json.loads((raw/'explorer-source-r3/legacy-control/result.json').read_text(encoding='utf8'));cli=json.loads((raw/'explorer-source-r3/cli-control-r3/result.json').read_text(encoding='utf8'))
assert visual['completed'] and len(visual['images'])==12 and not visual['exceptions'] and visual['embeddedGraphExact'] and visual['canonicalDownload']['fullGraphExact'] and visual['keyboardTabs']
assert visual['tailGeometry']['last']<=min(visual['tailGeometry']['containerBottom'],visual['tailGeometry']['viewport']) and visual['mobileSelection']['last']<=visual['mobileSelection']['viewport']
assert all(server[k] for k in ['serverClosed','sourceInputsUnchanged','nativeCliUnchanged','browserUnchanged','htmlUnchanged','allOwnBrowserProcessesGone']) and legacy['actualLegacyHtmlByteExact'] and cli['complete']
code=['native/src/export.rs','native/src/graph_extensions.rs','native/src/main.rs','native/src/explorer_source.html','native/tests/explorer_sources.rs']
assert all(subprocess.check_output(['git','show',head+':'+n],cwd=w)==(w/n).read_bytes() for n in code)
members=[]
with zipfile.ZipFile(out/'raw-logs.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for phase in ['explorer-source-r1','explorer-source-r2','explorer-source-r3']:
        for p in sorted((raw/phase).rglob('*')):
            if not p.is_file():continue
            assert not p.is_symlink();data=p.read_bytes();name=p.relative_to(raw).as_posix();z.writestr(name,data);members.append({'path':name,'bytes':len(data),'sha256':sha(data)})
with zipfile.ZipFile(out/'raw-logs.zip') as z:
    assert set(z.namelist())=={m['path'] for m in members}
    for m in members:
        b=z.read(m['path']);assert len(b)==m['bytes'] and sha(b)==m['sha256']
(out/'raw-index.json').write_text(json.dumps({'zipSha256':sha((out/'raw-logs.zip').read_bytes()),'members':members},indent=2)+'\n',encoding='utf8')
tool_paths=[Path(r'C:\Program Files\nodejs\node.exe'),Path(r'C:\Python314\python.exe'),Path(r'C:\Program Files\Google\Chrome\Application\chrome.exe'),Path(r'C:\Users\Lomi\.rustup\toolchains\1.94.1-x86_64-pc-windows-msvc\bin\rustc.exe'),Path(r'C:\Users\Lomi\.rustup\toolchains\1.94.1-x86_64-pc-windows-msvc\bin\cargo.exe')]
checkpoint={'sourceCommit':head,'sourceInputs':len(source),'codeFiles':{n:source[n] for n in code},'nativeCliSha256':cli['nativeCliSha256'],'tools':[{'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in tool_paths],'rustTests':15,'newRustTests':5,'typescriptTests':{'passed':260,'skipped':3},'allTargetClippyPassed':True,'finalSourceOnlyDeltaFromClippyAndWholeScopedRegression':['native/src/explorer_source.html: CSS code-panel maximum height only'],'finalDeltaCheckedBy':['actual rebuilt native CLI','new source export five tests','fmt','JavaScript syntax','twelve real Chrome images','exact byte selection and quote','desktop/mobile selected-range visibility'],'legacyHtmlByteExact':legacy['legacyHtmlSha256'],'visualImagesInspected':12,'originalCanonicalGraphUnchanged':True,'rawMembers':len(members),'rawBytes':sum(m['bytes'] for m in members),'rawZipBytes':(out/'raw-logs.zip').stat().st_size,'liveRebuildImplemented':False,'fullWindowsAcceptance':False,'fullStatusAcceptance':False,'wholeNativeBaselineRerun':False,'systemSettingsChanged':False,'modelRequested':False,'analyzedProjectCodeExecuted':False,'googleLiveValidated':False}
(out/'checkpoint.json').write_text(json.dumps(checkpoint,indent=2)+'\n',encoding='utf8')
readme='''# Explicit source-bearing Explorer checkpoint

The native `export --format html-source` command now exports checked immutable
captured files alongside the original canonical graph. Code, Context and Outline
tabs, exact observation quotes, keyboard tab navigation, paged source/outline and
relationship verb chips work offline. Original directions, parallel observations,
resolution labels, evidence, coverage and omissions stay distinct.

The old metadata-only HTML and document exports keep their original template.
An actual existing synthetic fixture reproduces its published HTML byte-for-byte
(`c3b4e6bbf9b63ce55a35e5cb2dfaf16237d64723e4aeb0f5051f0ef424a4f4e0`).
The actual downloaded canonical JSON remains deeply equal to the original graph.

Five new Rust tests plus ten existing export/explorer/document checks pass.
The frozen second scope passes all-target Clippy, format, typecheck and npm test
(260 passed, three existing skips). The final source delta is one CSS maximum-
height change; the native binary is rebuilt and all five source-export tests,
format, JavaScript syntax and the actual browser controls pass again.
The earlier complete native baseline is not rerun or relabeled for this addition.

Three real isolated Chrome156 rounds use only owned synthetic UTF-8/CRLF/BOM,
Unicode, hostile-text and long-file fixtures. The initial dense Outline and later
below-fold selection are corrected and regenerated. The twelve final PNGs are
visually inspected: initial view, Code, Context, exact quote, Outline, paged late
declaration, filtered neighbors, radial, Force start/middle/settled and mobile Code.
Exact UTF-8 selection and quote checks pass, keyboard tabs work, no JavaScript
exception occurs and the final selected declaration is geometrically visible on
desktop and mobile. There is no mobile horizontal overflow in this fixture.
No other viewport/browser, frame-rate or exhaustive accessibility certification
is claimed. The T3 preview twice returned `Preview automation snapshot timed out
after 15000ms.`; a fresh owned Chrome profile supplied the working fallback.
The first client controller had a duplicate local identifier before execution;
its original file is retained, corrected and run against the same live browser.
All browser/native jobs end empty and every owned server closes.

Source admission reuses canonical snapshot/graph/store/path-scope/hash/UTF-8/range
validation. Complete source bytes are admitted before opening blobs; source and
escaped final output have independent hard byte bounds. No partial result or
overwrite fallback is used. Source is text, never HTML or executable project code.
The default remains metadata-only; this explicit export contains source and needs
the same access controls as the captured project. Current disk freshness is not
checked. Live rebuild/reload still needs a separate explicit authorized workflow.
No system settings, privileges, symlink fixtures or ignores are changed.

[checkpoint.json](checkpoint.json) pins runtime source, tools and actual CLI.
[raw-index.json](raw-index.json) records every byte length and SHA-256 in the
lossless [raw ZIP](raw-logs.zip), including intermediate limitations and final
captures. Post-validation documentation changes are recorded separately.
Full STATUS/R1-R9, Windows, hosted/upstream, authenticated Google, main and release
acceptance remain open. No model or analyzed source is executed by these controls.
'''
(out/'README.md').write_text(readme,encoding='utf8',newline='\n')
print(json.dumps({k:checkpoint[k] for k in ['sourceCommit','rawMembers','rawBytes','rawZipBytes']}))
