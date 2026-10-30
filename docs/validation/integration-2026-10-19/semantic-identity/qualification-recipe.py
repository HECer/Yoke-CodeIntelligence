from pathlib import Path
import subprocess,json,hashlib,re,winreg
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');out=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/semantic-identity'
base='7a61aa782462614843fcd879dfffa31e9dacd382'
final=json.loads((out/'final-r1/result.json').read_bytes());native=final['commands'][0]
assert final['sourceUnchanged'] and native['totals']=={'passed':574,'failed':1,'ignored':7} and native['groups']==10
assert all(r['exitCode']==0 for r in final['commands'] if r['name']!='native')
log=(out/'final-r1/native.log').read_text(encoding='utf-8');failure='windows_host_aliases_cannot_read_protected_files_or_alternate_streams'
assert 'code: 1314' in log and f'    {failure}\n' in log
part=log.split('Running tests\\meaning_identity.rs',1)[1].split('Running tests\\meaning_join_cli.rs',1)[0]
cases=re.findall(r'^test (\S+) \.\.\. ok$',part,re.M);assert len(cases)==18 and 'FAILED' not in part
units=re.findall(r'^test (meaning_identity::\S+) \.\.\. ok$',log,re.M);assert len(units)==4
join=log.split('Running tests\\meaning_join_cli.rs',1)[1].split('Running tests\\retention.rs',1)[0]
assert 'test result: ok. 13 passed; 0 failed;' in join
path='native/tests/feedback_cli.rs';current=(w/path).read_bytes();blob=subprocess.check_output(['git','show',base+':'+path],cwd=w);assert current.replace(b'\r\n',b'\n')==blob.replace(b'\r\n',b'\n')
cli='native/src/feedback_cli.rs';old=subprocess.check_output(['git','show',base+':'+cli],cwd=w).decode().replace('\r\n','\n');new=(w/cli).read_text(encoding='utf-8')
def host_reader(s):return s.split('pub(crate) fn host_bytes',1)[1].split('impl Arguments',1)[0]
assert host_reader(old)==host_reader(new)
alias='native/src/meaning_aliases.rs';old_alias=subprocess.check_output(['git','show',base+':'+alias],cwd=w).decode().replace('\r\n','\n');new_alias=(w/alias).read_text(encoding='utf-8')
def norm(s):return s.split('fn normalized(',1)[1].split('fn term<',1)[0]
def jaro(s):return s.split('fn jaro_winkler(',1)[1].split('fn trigram_jaccard(',1)[0]
assert norm(old_alias)==norm(new_alias) and jaro(old_alias)==jaro(new_alias)
priv=subprocess.check_output(['whoami','/priv']);(out/'host-privileges.txt').write_bytes(priv);present=b'SeCreateSymbolicLinkPrivilege' in priv
try:
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,r'SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock') as k:mode=winreg.QueryValueEx(k,'AllowDevelopmentWithoutDevLicense')[0]
except FileNotFoundError:mode='not_configured'
assert not present and mode in ['not_configured',0]
(out/'host-prerequisites.json').write_text(json.dumps({'SeCreateSymbolicLinkPrivilegePresent':present,'DeveloperMode':mode,'MutationPerformed':False},indent=2)+'\n',encoding='utf-8')
oracle=Path.home()/'AppData/Local/Temp/ci-semantic-identity-minhash-oracle.py';result=subprocess.check_output([r'C:\Python314\python.exe',str(oracle)]);assert json.loads(result)['matching']==43 and json.loads(result)['bands']==3
(out/'minhash-oracle-result.json').write_bytes(result)
sha=lambda b:hashlib.sha256(b).hexdigest()
proof={'baseIntegration':base,'broaderNativeScopePassed':False,'nativeTotals':native['totals'],'nativeGroups':native['groups'],'newIntegrationCasesPassed':18,'newUnitCasesPassed':4,'newCasesPassed':22,'newCasesFailed':0,'newIntegrationCases':cases,'newUnitCases':units,'existingJoinCliCasesPassed':13,'existingAliasNormalizationAndJaroBodiesUnchanged':True,'aliasNormalizationNormalizedSha256':sha(norm(new_alias).encode()),'jaroNormalizedSha256':sha(jaro(new_alias).encode()),'failedTest':failure,'failedAt':'native/tests/feedback_cli.rs:1100:59','cause':'Windows OS1314 at unchanged symlink_file fixture setup; no application alias read reached.','originalSecurityTestFileUnchangedExceptCheckoutCRLF':True,'originalSecurityTestWorkingSha256':sha(current),'originalSecurityTestGitBlobSha256':sha(blob),'hostInputReaderFunctionUnchanged':True,'hostInputReaderNormalizedSha256':sha(host_reader(new).encode()),'fixtureOrPermissionGuardWeakened':False,'wholeNativeRerun':False,'liveSemanticQualityQualified':False,'fullGoalComplete':False}
(out/'qualification.json').write_text(json.dumps(proof,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in proof.items() if not k.endswith('Cases')}))
