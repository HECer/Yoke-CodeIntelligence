from pathlib import Path
import tempfile,subprocess,os,json,hashlib,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/codex-host/client-schema';out.mkdir(exist_ok=False);codex=Path(r'C:\Users\Lomi\AppData\Roaming\npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe')
with tempfile.TemporaryDirectory(prefix='yoke-codex-owned-schema-') as directory:
 folder=Path(directory);home=folder/'home';home.mkdir();env={k:v for k,v in os.environ.items() if not k.upper().startswith(('CODEX_','OPENAI_'))};env['CODEX_HOME']=str(home);schema=folder/'schema'
 p=subprocess.run([str(codex),'app-server','generate-json-schema','--out',str(schema),'--experimental'],cwd=folder,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW);(out/'generate.stdout').write_bytes(p.stdout);(out/'generate.stderr').write_bytes(p.stderr);assert p.returncode==0
 metadata=[];selected={}
 for file in sorted(schema.rglob('*.json')):
  raw=file.read_bytes();metadata.append({'path':file.relative_to(schema).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
  if file.stem in ['McpServerStatusListParams','McpServerToolCallParams','ThreadStartParams','InitializeParams','McpServerStatus','McpServerToolCallResponse']:
   dest=out/file.name;shutil.copyfile(file,dest);value=json.loads(raw);selected[file.stem]={'properties':value.get('properties',{}),'required':value.get('required',[])}
 (out/'generated-manifest.json').write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf-8');(out/'selected-fields.json').write_text(json.dumps(selected,indent=2)+'\n',encoding='utf-8');print(json.dumps({'generated':len(metadata),'selected':selected}),flush=True)
