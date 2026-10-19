from pathlib import Path
import os,subprocess,time,json,hashlib,datetime,re,shutil
root=Path.cwd();out=root/'validation/integration-20261018/digest-service-final';out.mkdir(exist_ok=False)
env=os.environ.copy();cargo=str(Path.home()/'.cargo/bin/cargo.exe');env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH']
def save(n,v):(out/n).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
def manifest():
 names=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z']).decode().split('\0')
 return {n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in sorted(set(names)) if n and (root/n).is_file() and (n.startswith(('native/src/','native/tests/','src/','test/','workers/','scripts/')) or n in ['Cargo.toml','Cargo.lock','native/Cargo.toml','package.json','package-lock.json','tsconfig.json','UPSTREAMS.json','docs/parity/native-reconciliation-2026-10-15.json'])}
before=manifest();save('source-before.json',before);receipts=[]
def run(label,cmd,timeout=900):
 print(json.dumps({'gate':label,'state':'running'}),flush=True);start=time.monotonic()
 with (out/(label+'.log')).open('wb') as f:r=subprocess.run(cmd,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=timeout)
 log=(out/(label+'.log')).read_text(encoding='utf-8',errors='replace');groups=[dict(zip(['passed','failed','ignored'],map(int,m))) for m in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)]
 receipt={'gate':label,'command':cmd,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'resultGroups':groups};receipts.append(receipt);save('receipts.json',receipts);print(json.dumps(receipt),flush=True)
run('format',[cargo,'fmt','--all','--check'])
run('clippy',[cargo,'clippy','--workspace','--all-targets','--locked','--keep-going','-j2','--','-D','warnings'])
targets=['github','github_discussions','github_app_digest','github_app_digest_cli','github_app_digest_mapping','github_app_digest_handoff','github_app_cli','github_app_auth_cli','github_app_publication','github_app_publication_cli','github_app_worker','github_app_worker_run','retention','parity_retention','decision_retention','meaning_retention_review','store_read_only','store_open_deadline','store_cancellation']
cmd=[cargo,'test','--locked','--no-fail-fast','-j2','--lib']
for target in targets:cmd+=['--test',target]
run('native-service-and-regressions',cmd+['--','--test-threads=2'])
run('python-contracts',['python','-m','unittest','discover','-s','scripts/tests','-v'])
run('typescript-typecheck',['npm.cmd','run','typecheck']);run('typescript-tests',['npm.cmd','test'])
run('actual-os-credential-cli',[cargo,'test','--locked','-j2','--test','github_app_digest_handoff','actual_cli_send_status_and_lost_response_recovery_use_os_key_without_retry','--','--exact','--ignored','--test-threads=1'])
save('tested-native-binary.json',{'path':'target/debug/yoke.exe','sha256':hashlib.sha256((root/'target/debug/yoke.exe').read_bytes()).hexdigest()})
after=manifest();save('source-after.json',after);summary={'allChecksPassed':all(r['exitCode']==0 for r in receipts),'sourceUnchanged':before==after,'checks':receipts,'scope':'Frozen local Windows digest service client/approval/journal/recovery validation using owned socket receivers, actual SQLite and captured source; separate actual Windows Credential Manager CLI gate. Configured production receiver acceptance, proprietary service equivalence and full platform/STATUS acceptance remain open.'};save('summary.json',summary);raise SystemExit(not(summary['allChecksPassed'] and summary['sourceUnchanged']))
