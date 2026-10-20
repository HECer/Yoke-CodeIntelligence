from pathlib import Path
import subprocess,os,json,ctypes,hashlib,urllib.request
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/falkor-publish/tool-probe';out.mkdir(parents=True,exist_ok=False);sdk=Path.home()/'AppData/Local/Android/Sdk/emulator';env=os.environ.copy();env['PATH']=str(sdk/'lib64')+os.pathsep+str(sdk/'qemu/windows-x86_64')+os.pathsep+str(sdk)+os.pathsep+env.get('PATH','');env['ANDROID_EMULATOR_LAUNCHER_DIR']=str(sdk)
previous=ctypes.windll.kernel32.SetErrorMode(0x0001|0x0002|0x8000);results=[]
try:
 for label,path,args in [('wsl',Path(r'C:\Windows\System32\wsl.exe'),['--list','--quiet']),('android-qemu',sdk/'qemu/windows-x86_64/qemu-system-x86_64-headless.exe',['-version']),('android-qemu-machines',sdk/'qemu/windows-x86_64/qemu-system-x86_64-headless.exe',['-machine','help'])]:
  if not path.is_file():results.append({'tool':label,'available':False});continue
  try:p=subprocess.run([str(path),*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,timeout=8,creationflags=subprocess.CREATE_NO_WINDOW)
  except subprocess.TimeoutExpired:results.append({'tool':label,'timedOut':True});continue
  (out/(label+'.stdout')).write_bytes(p.stdout);(out/(label+'.stderr')).write_bytes(p.stderr)
  def decoded(raw):return raw.decode('utf-16-le',errors='replace') if b'\0' in raw else raw.decode(errors='replace')
  results.append({'tool':label,'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'exitCode':p.returncode,'stdout':decoded(p.stdout)[:1200],'stderr':decoded(p.stderr)[:1200]})
finally:ctypes.windll.kernel32.SetErrorMode(previous)
opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));request=urllib.request.Request('https://api.github.com/repos/FalkorDB/FalkorDB/releases/latest',headers={'User-Agent':'Yoke-owned-validation-probe/0.5'})
with opener.open(request,timeout=20) as response:value=json.load(response)
public={'tag':value['tag_name'],'publishedAt':value['published_at'],'assets':[{'name':a['name'],'bytes':a['size'],'url':a['browser_download_url'],'digest':a.get('digest')} for a in value['assets']]};(out/'release.json').write_text(json.dumps(public,indent=2)+'\n');(out/'result.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps({'tools':results,'release':public}),flush=True)
