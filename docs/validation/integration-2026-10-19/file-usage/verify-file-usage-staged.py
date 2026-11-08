from pathlib import Path
import subprocess,json,hashlib,io
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/file-usage';frozen=json.loads((out/'integrated/source-before.json').read_bytes());metadata=json.loads((out/'metadata-after-freeze.json').read_bytes());tracker='docs/parity/native-reconciliation-2026-10-15.json';names=sorted(frozen);data=subprocess.check_output(['git','cat-file','--batch'],input=(''.join(':'+n+'\n' for n in names)).encode());stream=io.BytesIO(data);entries=[]
for n in names:
 header=stream.readline().decode().strip().split();assert len(header)==3 and header[1]=='blob',(n,header);blob=stream.read(int(header[2]));assert stream.read(1)==b'\n';current=(root/n).read_bytes()
 if n==tracker:
  original=(out/'frozen-tracker.json').read_bytes();assert hashlib.sha256(original).hexdigest()==frozen[n];assert hashlib.sha256(current).hexdigest()==metadata['trackerAfterSha256'];old=json.loads(original);new=json.loads(current);assert all(old[k]==new[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);materialization='post_freeze_thematic_metadata_only'
 else:
  assert hashlib.sha256(current).hexdigest()==frozen[n],n;materialization='raw_equal' if current==blob else 'crlf_only_checkout_materialization'
 assert current==blob or current.replace(b'\r\n',b'\n')==blob,n;entries.append({'path':n,'frozenSha256':frozen[n],'gitBlobSha256':hashlib.sha256(blob).hexdigest(),'gitObject':header[0],'materialization':materialization})
assert not stream.read();(out/'source-materialization.json').write_bytes((json.dumps({'allRuntimeInputsVerified':True,'count':len(entries),'metadataExceptions':[tracker],'entries':entries},indent=2)+'\n').encode());p=out/'index.json';index=json.loads(p.read_bytes());index['sourceMaterialization']='source-materialization.json';index['artifacts']=[]
for artifact in sorted(out.rglob('*')):
 if artifact.is_file() and artifact!=p:
  b=artifact.read_bytes();index['artifacts'].append({'path':artifact.relative_to(out).as_posix(),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
p.write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'inputs':len(entries),'rawEqual':sum(e['materialization']=='raw_equal' for e in entries),'crlfOnly':sum(e['materialization']=='crlf_only_checkout_materialization' for e in entries),'metadataExceptions':1,'artifacts':len(index['artifacts'])}))
