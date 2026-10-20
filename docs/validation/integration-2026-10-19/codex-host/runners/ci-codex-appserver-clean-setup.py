from pathlib import Path
p=Path(__file__).parent
for name in ['ci-codex-appserver-block.txt','ci-codex-host-live-r2.py']:
 file=p/name;s=file.read_text(encoding='utf-8').replace('messages=queue.Queue(512);transcript=[];app=None','messages=queue.Queue(512);transcript=[];app=None').replace('reader.start();counter=0','reader.start()').replace('    global_unused=None\n    nonlocal_unavailable=0\n','');file.write_text(s,encoding='utf-8')
