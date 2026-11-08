from pathlib import Path
import subprocess,json,hashlib,re
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
out=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/meaning-feedback'
base='92b090465dd70020155bd6b1be4951878849b0ca'
final=json.loads((out/'final-r1/result.json').read_bytes());native=final['commands'][0]
assert final['sourceUnchanged'] and native['totals']=={'passed':74,'failed':1,'ignored':0}
assert all(r['exitCode']==0 for r in final['commands'] if r['name']!='native')
log=(out/'final-r1/native.log').read_text(encoding='utf-8')
failure='windows_host_aliases_cannot_read_protected_files_or_alternate_streams'
assert 'code: 1314' in log and f'    {failure}\n' in log
part=log.split('Running tests\\meaning_feedback.rs',1)[1].split('Running tests\\meaning_view.rs',1)[0]
cases=re.findall(r'^test (\S+) \.\.\. ok$',part,re.M)
assert len(cases)==13 and 'FAILED' not in part
path='native/tests/feedback_cli.rs';current=(w/path).read_bytes();blob=subprocess.check_output(['git','show',base+':'+path],cwd=w)
assert current.replace(b'\r\n',b'\n')==blob.replace(b'\r\n',b'\n')
cli='native/src/feedback_cli.rs';old=subprocess.check_output(['git','show',base+':'+cli],cwd=w).decode().replace('\r\n','\n');new=(w/cli).read_text(encoding='utf-8')
def host_reader(s):return s.split('pub(crate) fn host_bytes',1)[1].split('impl Arguments',1)[0]
assert host_reader(old)==host_reader(new)
sha=lambda b:hashlib.sha256(b).hexdigest()
proof={'baseIntegration':base,'broaderNativeScopePassed':False,'nativeTotals':native['totals'],'nativeGroups':native['groups'],'newCasesPassed':len(cases),'newCasesFailed':0,'newCases':cases,'failedTest':failure,'failedAt':'native/tests/feedback_cli.rs:1100:59','cause':'Windows OS1314 at unchanged symlink_file fixture setup; no application alias read reached.','originalSecurityTestFileUnchangedExceptCheckoutCRLF':True,'originalSecurityTestWorkingSha256':sha(current),'originalSecurityTestGitBlobSha256':sha(blob),'hostInputReaderFunctionUnchanged':True,'hostInputReaderNormalizedSha256':sha(host_reader(new).encode()),'hostPrivileges':'host-privileges.txt','hostPrerequisites':'host-prerequisites.json','fixtureOrPermissionGuardWeakened':False,'wholeNativeRerun':False,'fullGoalComplete':False}
(out/'qualification.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in proof.items() if k!='newCases'}))
