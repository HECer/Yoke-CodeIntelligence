from pathlib import Path
import json,re,subprocess,shutil
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');raw=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/function-body-completion';out=raw/'doc-bindings-r1';out.mkdir(exist_ok=False)
parts=['compiler_worker.cjs','compiler_inline_worker.cjs','compiler_inline_body_worker.cjs','compiler_inline_body_vars_worker.cjs','compiler_inline_body_types_worker.cjs','compiler_move_worker.cjs','compiler_move_repairs_worker.cjs'];worker=b'\n'.join((w/'native/src'/p).read_bytes() for p in parts)
extra=r'''  const ownedDocs=selected.jsDoc||[],docRoots=new Set();
  const owned=d=>d.getSourceFile()===sf&&(d.getStart(sf)>=selected.getStart(sf)&&d.end<=selected.end||ownedDocs.some(doc=>d.getStart(sf)>=doc.getStart(sf)&&d.end<=doc.end));
  for(const doc of ownedDocs)walk(doc,n=>{if(n.kind===ts.SyntaxKind.JSDocTypeExpression)docRoots.add(n);if(ts.isTypeParameterDeclaration(n)&&n.default)docRoots.add(n.default);});
  for(const doc of docRoots)walk(doc,n=>{if(!ts.isIdentifier(n))return;const p=n.parent;if((ts.isQualifiedName(p)&&p.right===n)||((ts.isPropertySignature(p)||ts.isMethodSignature(p))&&p.name===n))return;const symbol=checker.getSymbolAtLocation(n);if(symbol?.declarations?.some(owned))return;let at=n;while(ts.isQualifiedName(at.parent))at=at.parent;const meaning=ts.isTypeQueryNode(at.parent)?ts.SymbolFlags.Value|ts.SymbolFlags.Namespace:ts.SymbolFlags.Type|ts.SymbolFlags.Namespace;for(const call of calls)assert(resolve(checker,checker.resolveName(n.text,call,meaning,false))===resolve(checker,symbol),'Body documentation free binding would change at consumer');});
'''.encode()
marker=b'  const varAssignments=';assert worker.count(marker)==1;worker=worker.replace(marker,extra+marker)
for phase in ['before','after']:
 old=("'Body context has errors "+phase+" expansion'").encode();worker=worker.replace(old,("'Body context has errors "+phase+" expansion: '+JSON.stringify("+phase+"Diagnostics)").encode())
(out/'worker.cjs').write_bytes(worker);(out/'insert.cjs').write_bytes(extra);shutil.copyfile(__file__,out/'runner.py')
text=(w/'native/tests/compiler_inline_body.rs').read_text(encoding='utf-8');a=text.index('fn jsdoc_call_types_');b=text.index('    ] {',a);matches=re.findall(r'\(\s*(true|false),\s*("(?:[^"\\]|\\.)*"),?\s*\)',text[a:b]);assert len(matches)==9,len(matches);cases=[{'language':'js','keep':False,'source':json.loads(s)} for _,s in matches]
shadow='/** @typedef {{n:number,left?:number}} Row */\nconst outside={n:3};\n/** @returns {Row} */\nfunction helper(){return outside;}export function outer(){/** @typedef {{n:number,right?:number}} Row */ const marker=0;void marker;/** @type {Row} */ const local={n:4};void local;const result=helper();return typeof result;}export const observed=outer();'
cases.append({'language':'js','keep':False,'source':shadow});(out/'cases.json').write_text(json.dumps(cases),encoding='utf-8');shutil.copyfile(raw/'fixtures-r2/driver.cjs',out/'driver.cjs');subprocess.run(['node',str(out/'driver.cjs'),str(out),str(raw.parent/'function-body-inline/worker-r1/request.json'),str(w/'node_modules/typescript/lib/typescript.js')],check=True)
