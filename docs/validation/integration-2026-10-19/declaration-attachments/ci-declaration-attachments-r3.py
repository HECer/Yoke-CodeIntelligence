from pathlib import Path
import subprocess,os,json,time,hashlib,shutil,datetime
root=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence');work=Path.cwd();out=root/'validation/integration-20261018/declaration-attachments-development/r3';out.mkdir(parents=True,exist_ok=False)
files=sorted(p for p in work.rglob('*') if p.is_file() and (p.relative_to(work).as_posix().startswith(('native/src/','native/tests/')) or p.relative_to(work).as_posix() in ['Cargo.toml','Cargo.lock','native/Cargo.toml']))
def hashes():return {p.relative_to(work).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
before=hashes()
for p in files:
 dst=out/'source'/p.relative_to(work);dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,dst)
cargo=str(Path.home()/'.cargo/bin/cargo.exe');env=os.environ.copy();env['PATH']=str(Path(cargo).parent)+os.pathsep+env['PATH'];env['YOKE_RUSTC']=str(Path.home()/'.cargo/bin/rustc.exe');env['YOKE_PYTHON']='C:/Python314/python.exe';ts=Path.home()/'AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules';env['YOKE_TS_LSP']=str(ts/'typescript-language-server/lib/cli.mjs');env['YOKE_TSC']=str(ts/'typescript/lib/tsc.js');base=[cargo,'--locked','-j1','--target-dir',str(root/'target')]
commands=[('format',[cargo,'fmt','--all','--check']),('actual-rust-python',[cargo,'test',*base[1:],'--test','declaration_attachments_live','--','--ignored','--nocapture','--test-threads=1']),('ranges',[cargo,'test',*base[1:],'--test','declaration_attachment_ranges','--test','symbol_delete_ranges','--','--nocapture','--test-threads=1']),('clippy',[cargo,'clippy',*base[1:],'--all-targets','--','-D','warnings']),('native',[cargo,'test',*base[1:],'--lib','--test','semantic_refactor','--test','agent_symbol_contracts','--test','agent_facade_semantics','--test','capture_changes','--test','plan_integrity','--test','symbol_move_own_v1','--test','symbol_move_effect_admission','--test','symbol_move_declarations','--','--nocapture','--test-threads=1']),('actual-typescript',[cargo,'test',*base[1:],'--test','symbol_delete_live','--','--ignored','--nocapture','--test-threads=1'])];results=[]
for name,cmd in commands:
 start=time.monotonic();print(json.dumps({'name':name,'state':'running'}),flush=True)
 with (out/(name+'.log')).open('wb') as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 result={'name':name,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'command':cmd};results.append(result);print(json.dumps(result),flush=True);(out/'progress.json').write_bytes((json.dumps(results,indent=2)+'\n').encode())
 if r.returncode:break
after=hashes();result={'recordedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'commands':results,'plannedCommandCount':len(commands),'sourceBefore':before,'sourceAfter':after,'sourceUnchanged':before==after};(out/'result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps({'sourceUnchanged':before==after,'inputCount':len(before),'allCommandsExecuted':len(results)==len(commands)}));raise SystemExit(int(any(r['exitCode'] for r in results) or before!=after or len(results)!=len(commands)))
