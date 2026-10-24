from pathlib import Path
import subprocess,tempfile,json,hashlib,os,time
primary=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=primary.parent/'Yoke-CI-r1-attach';base=primary/'validation/integration-20261018/codex-host';out=base/'client-live-r1';out.mkdir(exist_ok=False);native=primary/'target/debug/yoke.exe';codex=Path(r'C:\Users\Lomi\AppData\Roaming\npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe');sha=lambda b:hashlib.sha256(b).hexdigest();commands=[]
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
before=inventory();binary=sha(native.read_bytes());(out/'source-before.json').write_text(json.dumps(before,indent=2)+'\n',encoding='utf-8')
with tempfile.TemporaryDirectory(prefix='yoke-owned-codex-host-') as directory:
 root=Path(directory);home=root/('Codex home '+chr(0x1f4a1));source=root/('source '+chr(0x1f4a1));state=root/'state'
 for path in [home,source,state]:path.mkdir()
 original=('# human '+chr(0x1f4a1)+'\r\n[mcp_servers.human]\r\ncommand = "unrelated-host-tool"\r\n').encode();config=home/'config.toml';config.write_bytes(original);source_bytes=('// source '+chr(0x1f4a1)+'\r\nfn alpha(){beta();beta();}\r\nfn beta(){}\r\n').encode();(source/'owned.rs').write_bytes(source_bytes);(out/'owned-fixture.rs').write_bytes(source_bytes);policy=root/'policy.json';policy.write_text(json.dumps({'projects':[{'name':'owned','root':str(source),'readOnly':True}],'modes':['planning','query-projects'],'notesPolicy':{'writable':False}}),encoding='utf-8')
 env={k:v for k,v in os.environ.items() if not k.upper().startswith(('CODEX_','OPENAI_','HTTP_PROXY','HTTPS_PROXY','ALL_PROXY'))};env['CODEX_HOME']=str(home)
 def run(name,argv,stdin=None,success=True,timeout=60):
  command=[str(x) for x in argv];started=time.monotonic();p=subprocess.run(command,input=stdin,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=source,env=env,timeout=timeout,creationflags=subprocess.CREATE_NO_WINDOW);(out/(name+'.stdout')).write_bytes(p.stdout);(out/(name+'.stderr')).write_bytes(p.stderr);commands.append({'name':name,'command':command,'exitCode':p.returncode,'seconds':time.monotonic()-started})
  if success is not None:assert (p.returncode==0)==success,(name,p.returncode,'see owned stderr')
  return p
 version=run('codex-version',[codex,'--version']).stdout.decode().strip();assert version=='codex-cli 0.163.0'
 selection=root/'selection.json';selection.write_text(json.dumps({'installation':'owned-fixture','targetRoots':[str(home)],'executable':str(native),'policy':str(policy),'stateRoot':str(state),'adapter':{'kind':'codex_mcp_v1','sourceRoot':str(source)}}),encoding='utf-8')
 cli=lambda name,args:run(name,[native,'--timeout','60000','--max-bytes','4000000','host-install',*args])
 planned=cli('plan',['plan','--selection',selection]);plan=json.loads(planned.stdout);assert plan['profile']=='codex-mcp-v1' and plan['files'][0]['path']=='config.toml';assert not (source/'.yoke').exists() and config.read_bytes()==original and list(state.iterdir())==[];plan_path=root/'approved.json';plan_path.write_bytes(planned.stdout)
 denied=run('missing-write-grant',[native,'host-install','apply','--selection',selection,'--plan',plan_path],success=False);assert b'allow-host-writes' in denied.stderr and config.read_bytes()==original
 cli('apply',['apply','--selection',selection,'--plan',plan_path,'--allow-host-writes']);installed=config.read_bytes();assert installed.startswith(original);(out/'installed-config.toml').write_bytes(installed)
 client=run('codex-get-installed',[codex,'mcp','get','yoke_owned-fixture','--json']);entry=json.loads(client.stdout);transport=entry['transport'];assert transport['type']=='stdio' and transport['command']==str(native) and transport['cwd']==str(source);assert entry['enabled'] is True
 argv=[transport['command'],*transport['args']];expected=['--root',str(source),'serve-session','--id','owned-fixture','--host-profile',str(policy),'--profile','native'];assert transport['args']==expected
 assert set(entry['enabled_tools'])=={'ci_find','ci_read','ci_trace','ci_context','ci_retrieve','ci_graph'} and not (source/'.yoke').exists()
 run('source-index',[native,'--root',source,'index']);requests=[{'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'yoke-owned-host-fixture','version':'0.5.0'}}},{'jsonrpc':'2.0','method':'notifications/initialized','params':{}},{'jsonrpc':'2.0','id':2,'method':'tools/list','params':{}},{'jsonrpc':'2.0','id':3,'method':'tools/call','params':{'name':'ci_find','arguments':{'query':'alpha'}}},{'jsonrpc':'2.0','id':4,'method':'tools/call','params':{'name':'ci_read','arguments':{'path':'owned.rs','startByte':0,'endByte':len(source_bytes)}}}]
 result=run('native-mcp-exact-client-argv',argv,stdin=b''.join(json.dumps(x).encode()+b'\n' for x in requests));replies={x['id']:x for x in [json.loads(line) for line in result.stdout.splitlines()] if 'id' in x};assert set(replies)=={1,2,3,4} and not any('error' in x for x in replies.values());assert set(entry['enabled_tools'])<={t['name'] for t in replies[2]['result']['tools']};assert 'alpha' in json.dumps(replies[3]) and not replies[3]['result'].get('isError')
 read=json.loads(replies[4]['result']['content'][0]['text']);assert not replies[4]['result'].get('isError')
 def strings(value):
  if isinstance(value,str):yield value
  elif isinstance(value,list):
   for item in value:yield from strings(item)
  elif isinstance(value,dict):
   for item in value.values():yield from strings(item)
 assert source_bytes.decode() in set(strings(read)),'Exact source byte response missing'
 appendix=b'\r\n[mcp_servers.appended]\r\ncommand = "appended-host-tool"\r\n';config.write_bytes(installed+appendix);remove=cli('uninstall-plan',['uninstall-plan','--selection',selection]);remove_path=root/'uninstall.json';remove_path.write_bytes(remove.stdout);cli('uninstall-apply',['apply','--selection',selection,'--plan',remove_path,'--allow-host-writes']);remaining=config.read_bytes();(out/'remaining-config.toml').write_bytes(remaining);assert remaining.startswith(original) and remaining.endswith(appendix) and b'yoke_owned-fixture' not in remaining
 missing=run('codex-get-after-uninstall',[codex,'mcp','get','yoke_owned-fixture','--json'],success=False);assert b'yoke_owned-fixture' in missing.stderr
 result={'actualClient':version,'clientBinarySha256':sha(codex.read_bytes()),'nativeBinarySha256':binary,'isolatedCodexHome':True,'realHomeOrRealConfigModified':False,'installedCommandArgsAndCwdParsedByActualClient':True,'sixExplicitToolsParsedByClient':True,'nativeMcpExactConfiguredArgvHandshakeAndReads':True,'actualCodexModelTurnOrAutomaticToolDispatchTested':False,'originalUnicodeCrLfSourceReadExact':True,'unownedHumanTablesAndAppendixPreserved':True,'clientEntryAbsentAfterUninstall':True,'planningAndClientInspectionCreatedNoSourceStore':True,'modelCalls':0,'projectCodeExecution':False,'configuredUnrelatedServersExecuted':False}
 (out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
(out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n',encoding='utf-8');after=inventory();(out/'source-after.json').write_text(json.dumps(after,indent=2)+'\n',encoding='utf-8');assert before==after and sha(native.read_bytes())==binary;(out/'frozen-inputs.json').write_text(json.dumps({'sourceUnchanged':True,'binaryUnchanged':True,'inputs':len(before),'binarySha256':binary})+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
