from pathlib import Path
import subprocess,sys
p=Path(__file__).parent;s=(p/'ci-falkor-client-check-r3.py').read_text(encoding='utf-8').replace('/client-r3','/client-r4');r=p/'ci-falkor-client-check-r4.py';r.write_text(s,encoding='utf-8');subprocess.run([sys.executable,str(r)],check=True)
