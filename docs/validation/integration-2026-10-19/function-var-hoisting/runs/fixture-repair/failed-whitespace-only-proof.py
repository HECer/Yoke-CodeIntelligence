from pathlib import Path
import hashlib,json,re,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
root=w.parent/'Yoke-CodeIntelligence'
raw=root/'validation/integration-20261018/function-var-hoisting'
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
sha=lambda b:hashlib.sha256(b).hexdigest()
guard=read(raw/'guard-r3/result.json')
assert guard['allChecksPassed'] and guard['sourceUnchanged']
before=read(raw/'guard-r3/source-after.json')
assert all(sha((w/n).read_bytes())==h for n,h in before.items())
out=raw/'fixture-repair';out.mkdir(exist_ok=False)
p=w/'native/src/compiler_inline_body.rs'
old=p.read_bytes();s=old.decode('utf-8')
base=subprocess.check_output(['git','show','HEAD:native/src/compiler_inline_body.rs'],cwd=w).decode('utf-8')
pattern=r'/\* caf[^*]*\*/'
broken=re.search(pattern,s).group();original=re.search(pattern,base).group()
assert original=='/* caf\u00e9 */' and broken!=original and s.count(broken)==2
expected=s.replace(broken,original)
(out/'before.rs').write_bytes(old)
binary=root/'target/debug/yoke.exe'
binary_before=sha(binary.read_bytes())
p.write_bytes(expected.encode('utf-8'))
with (out/'format.log').open('wb') as log:
 subprocess.run([str(Path.home()/'.cargo/bin/cargo.exe'),'fmt','--all'],cwd=w,stdout=log,stderr=subprocess.STDOUT,check=True)
after=p.read_bytes();text=after.decode('utf-8')
marker='#[cfg(test)]'
prefix=s[:s.index(marker)]
assert text[:text.index(marker)]==prefix
assert re.sub(r'\s+','',text[text.index(marker):])==re.sub(r'\s+','',expected[expected.index(marker):])
assert all(sha((w/n).read_bytes())==h for n,h in before.items() if n!='native/src/compiler_inline_body.rs')
(out/'after.rs').write_bytes(after)
shutil.copyfile(__file__,out/'runner.py')
proof={'guardPhase':'guard-r3','guardNativeInputs':len(before),'changedFile':'native/src/compiler_inline_body.rs','changedRegion':'cfg(test) only: restore two original UTF-8 comment literals plus formatter whitespace','restoredLiteral':original,'restoredOccurrences':2,'productionPrefixUnchanged':True,'productionPrefixSha256':sha(prefix.encode()),'allOtherGuardInputsUnchanged':True,'otherGuardInputs':len(before)-1,'beforeSha256':sha(old),'afterSha256':sha(after),'mainBinaryBeforeReviewSha256':binary_before}
(out/'proof.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
print(json.dumps(proof))
