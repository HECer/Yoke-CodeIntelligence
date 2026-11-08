from pathlib import Path
import hashlib,json,re,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=w.parent/'Yoke-CodeIntelligence';raw=root/'validation/integration-20261018/function-var-hoisting';out=raw/'fixture-repair'
sha=lambda b:hashlib.sha256(b).hexdigest()
before=json.loads((raw/'guard-r3/source-after.json').read_text())
p=w/'native/src/compiler_inline_body.rs';old=(out/'before.rs').read_bytes();s=old.decode('utf-8');text=p.read_bytes().decode('utf-8')
base=subprocess.check_output(['git','show','HEAD:native/src/compiler_inline_body.rs'],cwd=w).decode('utf-8')
pattern=r'/\* caf[^*]*\*/';broken=re.search(pattern,s).group();original=re.search(pattern,base).group()
assert original=='/* caf\u00e9 */' and broken!=original and s.count(broken)==2
expected=s.replace(broken,original)
arrays=r'for comment in \[[^\]]+\] \{'
original_array=re.search(arrays,base).group();expected_array=re.search(arrays,expected).group()
assert expected.count(expected_array)==1
expected=expected.replace(expected_array,original_array)
assert text==expected,'Unexpected change beyond literal restorations and original comment-array formatting'
marker='#[cfg(test)]';prefix=s[:s.index(marker)];assert text[:text.index(marker)]==prefix
assert all(sha((w/n).read_bytes())==h for n,h in before.items() if n!='native/src/compiler_inline_body.rs')
(out/'after.rs').write_bytes(p.read_bytes())
shutil.copyfile(out.parent.parent.parent.parent.parent/'AppData/Local/Temp/nonexistent',out/'never') if False else None
shutil.copyfile(Path(__file__).with_name('ci-function-var-fixture-repair.py'),out/'failed-whitespace-only-proof.py')
shutil.copyfile(__file__,out/'runner.py')
proof={'guardPhase':'guard-r3','guardNativeInputs':len(before),'changedFile':'native/src/compiler_inline_body.rs','changedRegion':'cfg(test) only: two original UTF-8 comment literals and exact original comment-array formatting (including optional trailing comma)','restoredLiteral':original,'restoredOccurrences':2,'exactExpectedTextChecked':True,'productionPrefixUnchanged':True,'productionPrefixSha256':sha(prefix.encode()),'allOtherGuardInputsUnchanged':True,'otherGuardInputs':len(before)-1,'beforeSha256':sha(old),'afterSha256':sha(p.read_bytes()),'mainBinaryBeforeReviewSha256':sha((root/'target/debug/yoke.exe').read_bytes()),'initialProofFailure':'The initial whitespace-only comparison rejected rustfmt removing the optional trailing comma; final proof checks exact expected text using the original Git comment-array spelling.'}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
print(json.dumps(proof))
