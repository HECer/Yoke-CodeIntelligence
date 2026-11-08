from pathlib import Path
import json,tarfile,gzip,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/falkor-publish';receipt=json.loads((base/'acquire/result.json').read_bytes());cache=Path(receipt['cache']);out=base/'runtime-inspect';out.mkdir(exist_ok=False)
listing=(base/'acquire/qemu-archive-list.txt').read_text(encoding='utf-8-sig');paths=[line[7:].strip() for line in listing.splitlines() if line.startswith('Path = ')];qemu=[p for p in paths if 'qemu-system-x86_64.exe' in p or p.lower().endswith('.dll') or p.endswith('bios-256k.bin')];(out/'qemu-files.json').write_text(json.dumps(qemu,indent=2)+'\n')
images=[]
for layer in receipt['image']['layers']:
 path=cache/'image'/(layer['digest'].split(':')[1]+'.blob')
 with tarfile.open(path,'r:*') as archive:
  for m in archive:
   if m.isfile() and ('falkor' in m.name.lower() or m.name.endswith('redis-server')):images.append({'path':m.name,'bytes':m.size,'mode':m.mode})
(out/'server-paths.json').write_text(json.dumps(images,indent=2)+'\n')
raw=(cache/'initramfs-virt').read_bytes();data=gzip.decompress(raw) if raw.startswith(b'\x1f\x8b') else raw;assert len(data)<=128_000_000;pos=0;entries=[]
while pos+110<=len(data):
 header=data[pos:pos+110];assert header[:6] in [b'070701',b'070702'],(pos,header[:6]);fields=[int(header[6+i*8:14+i*8],16) for i in range(13)];size=fields[6];namesize=fields[11];pos+=110;name=data[pos:pos+namesize-1].decode();pos=(pos+namesize+3)&~3;content=data[pos:pos+size];pos=(pos+size+3)&~3
 if name=='TRAILER!!!':break
 entries.append({'path':name,'bytes':size,'mode':fields[1],'inode':fields[0],'links':fields[4]})
(out/'initramfs-files.json').write_text(json.dumps(entries,indent=2)+'\n');(out/'init-original').write_bytes(next((b'' for _ in []),b''))
result={'qemuSelectedCount':len(qemu),'qemuSelectedExample':qemu[:12],'serverFiles':images,'initramfsBytes':len(raw),'initramfsUncompressedBytes':len(data),'initramfsEntries':len(entries),'initramfsImportant':[e for e in entries if e['path'] in ['init','bin/busybox','usr/bin/busybox','bin/sh'] or 'virtio_net' in e['path'] or 'ld-musl' in e['path']]};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
