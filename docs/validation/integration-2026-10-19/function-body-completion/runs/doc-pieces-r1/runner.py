from pathlib import Path
import json,subprocess,hashlib,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion';out=raw/'doc-pieces-r1';out.mkdir(exist_ok=False)
worker=(raw/'literal-mode-r1/worker.cjs').read_bytes()
old=b"  const ownedDocs=selected.jsDoc||[],docRoots=new Set();";assert worker.count(old)==1
new=r'''  const allDocs=selected.jsDoc||[],declarationTag=n=>[ts.SyntaxKind.JSDocTypedefTag,ts.SyntaxKind.JSDocCallbackTag,ts.SyntaxKind.JSDocImportTag].includes(n.kind),functionTags=new Set(['param','returns','return','template','this','type','satisfies','overload','constructor','class','extends','augments','implements']);
  const splitDocs=allDocs.some(doc=>(doc.tags||[]).some(declarationTag)),docPieces=splitDocs?allDocs.flatMap(doc=>[...(doc.tags||[])].filter(tag=>functionTags.has(tag.tagName.text)).map(tag=>({start:tag.getStart(sf),end:Math.min(tag.end,doc.end-2),tag:true}))):undefined;
  const copiedDocs=docPieces||allDocs.map(doc=>({start:doc.getStart(sf),end:doc.end,tag:false})),ownedDocs=allDocs,docRoots=new Set();
'''.encode();worker=worker.replace(old,new)
old=b"ownedDocs.some(doc=>d.getStart(sf)>=doc.getStart(sf)&&d.end<=doc.end)";assert worker.count(old)==1;worker=worker.replace(old,b"copiedDocs.some(doc=>d.getStart(sf)>=doc.start&&d.end<=doc.end)")
old=b"for(const doc of ownedDocs)walk(doc,n=>{if(n.kind===ts.SyntaxKind.JSDocTypeExpression)docRoots.add(n);if(ts.isTypeParameterDeclaration(n)&&n.default)docRoots.add(n.default);});";assert worker.count(old)==1;worker=worker.replace(old,b"for(const doc of ownedDocs)walk(doc,n=>{if(!copiedDocs.some(piece=>n.getStart(sf)>=piece.start&&n.end<=piece.end))return;if(n.kind===ts.SyntaxKind.JSDocTypeExpression)docRoots.add(n);if(ts.isTypeParameterDeclaration(n)&&n.default)docRoots.add(n.default);});")
old=b"const docs=(selected.jsDoc||[]).map(n=>n.getText(sf)).join(line);";assert worker.count(old)==1;worker=worker.replace(old,b"const docs=copiedDocs.map(doc=>doc.tag?'/** '+text.slice(doc.start,doc.end)+' */':text.slice(doc.start,doc.end)).join(line);")
old=b"names,resultType:resultHint,";assert worker.count(old)==1;worker=worker.replace(old,b"names,...(docPieces?{docPieces:docPieces.map(piece=>{const s=span(sf,piece.start,piece.end);return{startByte:s.startByte,endByte:s.endByte,tag:piece.tag};})}:{}),resultType:resultHint,")
(out/'worker.cjs').write_bytes(worker);shutil.copyfile(__file__,out/'runner.py');results=[]
for i in range(13):
 r=json.loads((raw/f'literal-mode-r1/request-{i}.json').read_bytes());data=json.dumps(r).encode();(out/f'request-{i}.json').write_bytes(data);p=subprocess.run(['node',str(out/'worker.cjs'),hashlib.sha256(data).hexdigest()],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd=out);(out/f'stdout-{i}.json').write_bytes(p.stdout);(out/f'stderr-{i}.log').write_bytes(p.stderr);row={'case':i,'exitCode':p.returncode,'error':p.stderr.decode()};results.append(row)
 if p.returncode==0:
  reply=json.loads(p.stdout)['bodyInline'];before=[t['type'] for t in reply['before']['types'] if t['span']['text'] in ['result','outer']];after=[t['type'] for t in reply['after']['types'] if t['span']['text'] in ['result','outer']];row.update({'beforeTypes':before,'afterTypes':after,'callerTypesExact':before==after,'literalBox':reply['frames'][0]['names'].get('literalBox',False),'docPieces':reply['frames'][0].get('docPieces')})
result={'cases':results,'allChecksPassed':all(x['exitCode']==0 and x.get('callerTypesExact') for x in results),'nativeAdmission':False,'snapshotIdsSynthetic':True,'analyzedCodeExecuted':False,'prototypeWorkerPatch':True};(out/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
