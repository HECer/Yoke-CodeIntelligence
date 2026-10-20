from pathlib import Path
import subprocess,sys
p=Path(__file__).parent;s=(p/'ci-falkor-host-check.py').read_text(encoding='utf-8').replace("falkor-publish/host-r1","falkor-publish/host-r2");r=p/'ci-falkor-host-check-r2.py';r.write_text(s,encoding='utf-8');subprocess.run([sys.executable,str(r)],check=True)
