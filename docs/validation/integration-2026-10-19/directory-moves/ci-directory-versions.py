from pathlib import Path
import hashlib,json,subprocess
out=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence/validation/integration-20261018/directory-moves-development')
values=[]
for name in ['rust-analyzer','rustc','cargo']:
 p=Path.home()/'.cargo/bin'/(name+'.exe');raw=subprocess.check_output([str(p),'--version'],stderr=subprocess.STDOUT);(out/(name+'-version.log')).write_bytes(raw);values.append({'name':name,'executable':str(p),'launcherSha256':hashlib.sha256(p.read_bytes()).hexdigest(),'version':raw.decode().strip()})
(out/'tool-versions.json').write_text(json.dumps(values,indent=2)+'\n',encoding='utf-8');print(json.dumps(values))
