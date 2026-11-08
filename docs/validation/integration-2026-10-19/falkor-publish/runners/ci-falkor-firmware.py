from pathlib import Path
import subprocess,json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';tool=json.loads((base/'runtime-build/result.json').read_bytes());qemu=Path(tool['qemu']).parent;cache=qemu.parent;out=base/'firmware-r2';out.mkdir(exist_ok=False)
listing=(base/'acquire/qemu-archive-list.txt').read_text(encoding='utf-8-sig');paths=[l[7:].strip() for l in listing.splitlines() if l.startswith('Path = ')];selected=[p for p in paths if p.replace('\\','/').split('/')[-1] in ['kvmvapic.bin','linuxboot_dma.bin']];assert len(selected)==2
p=subprocess.run([r'C:\Program Files\7-Zip\7z.exe','e',str(cache/'qemu-installer.exe'),'-o'+str(qemu),'-y',*selected],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW);(out/'extract.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0
records=[{'file':name,'bytes':(qemu/name).stat().st_size,'sha256':hashlib.sha256((qemu/name).read_bytes()).hexdigest()} for name in ['kvmvapic.bin','linuxboot_dma.bin']];(out/'result.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps(records))
