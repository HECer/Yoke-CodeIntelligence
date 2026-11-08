from pathlib import Path
import subprocess,json,hashlib,re
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence')
work=root.parent/'Yoke-CI-r1-attach'
out=root/'validation/integration-20261018/platform-d819'
raw=(out/'ubuntu-job.log').read_bytes()
text=re.sub(r'\x1b\[[0-9;]*m','',raw.decode('utf-8'))
groups=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
failed=re.findall(r'Z test (\S+) \.\.\. FAILED',text)
assert len(failed)==36 and len(set(failed))==36
assert all(n.startswith('host_install::tests::') or n.startswith('host_install_cli_') or '::' not in n for n in failed)
summary={'sourceCommit':'d819d7100b06f239fae291083908e17e18d3ae35','jobUrl':'https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/39398858360/job/118548252678','rawLogSha256':hashlib.sha256(raw).hexdigest(),'groups':len(groups),'totals':{k:sum(int(g[i]) for g in groups) for i,k in enumerate(['passed','failed','ignored'])},'failedTests':failed,'cause':'Each panic reports the explicitly Windows-only host-install guarded filesystem backend. Cargo stdout/stderr interleave, so nearby Running lines are not reliable group labels.'}
(out/'hosted-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
paths=['native/src/host_install.rs','native/tests/host_install.rs','native/tests/host_install_cli.rs']
proof=[]
for path in paths:
 old=subprocess.check_output(['git','show','d819d71:'+path],cwd=work).decode('utf-8').replace('\r\n','\n')
 new=(work/path).read_text(encoding='utf-8')
 # Each complete original test body remains byte-identical after newline normalization.
 original=re.findall(r'^(?:    )?fn (\w+)\([^\n]*\).*?\{',old,re.M)
 tests=[]
 for match in re.finditer(r'^([ ]*)fn (\w+)\(',old,re.M):
  name=match[2]
  if not re.search(r'#\[test\]\s*(?:#\[cfg\(windows\)\]\s*)?$',old[:match.start()]): continue
  prefix=match[1]+'fn '+name+'('
  end=old.find('\n'+match[1]+'}',match.end())
  assert end>=0
  body=old[match.start():end+len(match[1])+2]
  assert body in new,(path,name)
  tests.append({'name':name,'normalizedBodySha256':hashlib.sha256(body.encode()).hexdigest()})
 assert tests
 proof.append({'file':path,'unchangedOriginalTests':tests})
(out/'original-test-bodies.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
r=subprocess.run(['gh','pr','view','7','--repo','HECer/Yoke-CodeIntelligence','--json','body,headRefOid,url'],capture_output=True,check=True)
(out/'prior-pr.json').write_bytes(r.stdout)
print(json.dumps({'groups':len(groups),'totals':summary['totals'],'unchangedTests':{x['file']:len(x['unchangedOriginalTests']) for x in proof}}))
