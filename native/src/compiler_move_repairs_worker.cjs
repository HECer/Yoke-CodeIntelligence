'use strict';
// Source-only repairs. Rust reconstructs their stages and original-byte edits.
function repairCompilerMove(env,service,program,selected,compilerEdits,makeService,compilerOperations){
 const {ts,assert,source,abs,rel,slash,span}=env,file=abs(env.input.intent.path),destination=abs(env.input.intent.destination);
 const original=new Map(Object.entries(env.input.contents).map(([p,t])=>[abs(p),t])),tables=new Map(),repairs=[];
 const shorthand=(name,newName)=>name==='__proto__'?'["__proto__"]: '+newName:name+': '+newName;
 let operations=compilerOperations,visits=0;const tick=()=>assert(++visits<=2_000_000,'Move repair AST capacity');
 const raw=p=>Buffer.from(original.get(abs(p))),table=p=>{if(!tables.has(p))tables.set(p,[{start:0,end:raw(p).length}]);return tables.get(p);};
 const split=(p,parts,at)=>{let cursor=0;for(let i=0;i<parts.length;i++){const part=parts[i],size=part.text===undefined?part.end-part.start:Buffer.byteLength(part.text);if(at===cursor)return i;if(at<cursor+size){const offset=at-cursor;let left,right;if(part.text===undefined){left={start:part.start,end:part.start+offset};right={start:part.start+offset,end:part.end};}else{const b=Buffer.from(part.text);left={text:b.subarray(0,offset).toString('utf8')};right={text:b.subarray(offset).toString('utf8')};assert(Buffer.byteLength(left.text)===offset&&Buffer.byteLength(right.text)===size-offset,'Move repair UTF-8 split');}parts.splice(i,1,left,right);return i+1;}cursor+=size;}assert(at===cursor,'Move repair piece offset');return parts.length;};
 const applyPieces=changes=>{for(const p of new Set(changes.map(e=>e.path))){const parts=table(p);for(const e of changes.filter(e=>e.path===p).reverse()){const start=split(p,parts,e.startByte),end=split(p,parts,e.endByte);parts.splice(start,end-start,...(e.text?[{text:e.text}]:[]));assert(parts.length<=8001,'Move repair piece capacity');}}};
 const materialize=p=>table(p).map(part=>part.text===undefined?raw(p).subarray(part.start,part.end).toString('utf8'):part.text).join('');
 applyPieces(compilerEdits);for(const p of tables.keys())assert(materialize(p)===source.get(abs(p)),'Move compiler piece replay mismatch');
 const applyRepair=(repair,changes)=>{
  assert(repairs.length<256&&(operations+=changes.length)<=2000,'Move repair operation capacity');changes.sort((a,b)=>Buffer.compare(Buffer.from(a.path),Buffer.from(b.path))||a.startByte-b.startByte);
  for(let i=1;i<changes.length;i++)assert(changes[i-1].path!==changes[i].path||changes[i-1].endByte<=changes[i].startByte,'Move repair overlap');
  applyPieces(changes);for(const p of new Set(changes.map(e=>e.path)))source.set(abs(p),materialize(p));let bytes=0;for(const p of tables.keys()){const size=Buffer.byteLength(source.get(abs(p)));bytes+=size;assert(size<=4_000_000&&bytes<=16_000_000,'Move repaired output capacity');}repairs.push(repair);
 };
 const checker=program.getTypeChecker(),resolve=(c,s)=>s&&(s.flags&ts.SymbolFlags.Alias)?c.getAliasedSymbol(s):s;
 const nameOf=n=>ts.isVariableStatement(n)?n.declarationList.declarations[0]?.name:n.name;
 const selectedSymbol=resolve(checker,checker.getSymbolAtLocation(nameOf(selected)));
 const rename=service.getEditsForFileRename(file,destination,{newLineCharacter:'\n'},{allowTextChangesInNewFiles:true});
 for(const sf of program.getSourceFiles()){
  const p=slash(path.resolve(sf.fileName));if(!original.has(p))continue;
  for(const node of sf.statements){
   tick();if(!ts.isExportDeclaration(node)||!node.moduleSpecifier||!node.exportClause||!ts.isNamedExports(node.exportClause))continue;
   const module=checker.getSymbolAtLocation(node.moduleSpecifier),moduleFile=module?.declarations?.find(ts.isSourceFile)?.fileName;
   if(!moduleFile||slash(path.resolve(moduleFile))!==file)continue;
   const indices=[];for(let i=0;i<node.exportClause.elements.length;i++)if(resolve(checker,checker.getSymbolAtLocation(node.exportClause.elements[i].name))===selectedSymbol)indices.push(i);if(!indices.length)continue;
   const currentText=source.get(p),currentFile=ts.createSourceFile(p,currentText,ts.ScriptTarget.Latest,true,sf.scriptKind),matches=currentFile.statements.filter(n=>ts.isExportDeclaration(n)&&n.getText(currentFile)===node.getText(sf));
   if(!matches.length)continue;assert(matches.length===1,'Move ambiguous current reexport');const current=matches[0];
   const fixes=rename.filter(e=>slash(path.resolve(e.fileName))===p).flatMap(e=>e.textChanges).filter(e=>e.span.start>=node.moduleSpecifier.getStart(sf)&&e.span.start+e.span.length<=node.moduleSpecifier.end);
   assert(fixes.length===1,'Move reexport destination specifier unavailable');const fix=fixes[0],literal=node.moduleSpecifier.getText(sf),at=fix.span.start-node.moduleSpecifier.getStart(sf),moduleText=literal.slice(0,at)+fix.newText+literal.slice(at+fix.span.length);
   const originalSpan=span(sf,node.getStart(sf),node.end),currentSpan=span(currentFile,current.getStart(currentFile),current.end);let replacement;
   if(indices.length===current.exportClause.elements.length){const at=current.moduleSpecifier.getStart(currentFile)-current.getStart(currentFile);replacement=currentSpan.text.slice(0,at)+moduleText+currentSpan.text.slice(at+current.moduleSpecifier.getWidth(currentFile));}
   else{
    const clause=current.exportClause,syntax=clause.getChildren(currentFile).find(n=>n.kind===ts.SyntaxKind.SyntaxList);assert(syntax,'Move export syntax unavailable');const commas=syntax.getChildren(currentFile).filter(n=>n.kind===ts.SyntaxKind.CommaToken),items=[...clause.elements];assert(commas.length===items.length-1||commas.length===items.length,'Move export separator shape');
    let previous=clause.getStart(currentFile)+1;const parts=[];for(let i=0;i<items.length;i++){const end=commas[i]?.getStart(currentFile)??clause.end-1;assert(previous<=items[i].getStart(currentFile)&&items[i].end<=end,'Move export item ownership');parts.push(currentText.slice(previous,end));previous=commas[i]?.end??end;}
    const trailing=commas.length===items.length,tail=trailing?currentText.slice(previous,clause.end-1):'',prefix=currentText.slice(current.getStart(currentFile),clause.getStart(currentFile)+1),suffix=currentText.slice(clause.end-1,current.end),moduleAt=current.moduleSpecifier.getStart(currentFile)-(clause.end-1),newSuffix=suffix.slice(0,moduleAt)+moduleText+suffix.slice(moduleAt+current.moduleSpecifier.getWidth(currentFile)),chosen=new Set(indices);
    replacement=prefix+parts.filter((_,i)=>!chosen.has(i)).join(',')+(trailing?','+tail:'')+suffix+(currentText.includes('\r\n')?'\r\n':'\n')+prefix+parts.filter((_,i)=>chosen.has(i)).join(',')+(trailing?',':'')+newSuffix;
   }
   applyRepair({kind:'reexport',original:originalSpan,current:currentSpan,selected:indices,moduleText},[{path:rel(p),startByte:currentSpan.startByte,endByte:currentSpan.endByte,text:replacement}]);
  }
 }
 const symbolKey=(c,s)=>{s=resolve(c,s);if(!s)return undefined;return JSON.stringify((s.declarations||[]).map(d=>{const owners=[];for(let n=d;n&&!ts.isSourceFile(n);n=n.parent)if(n.name&&typeof n.name.getText==='function')owners.push([n.kind,n.name.getText()]);return[slash(path.resolve(d.getSourceFile().fileName)),d.kind,owners];}));};
 const symbolAt=(c,n)=>ts.isShorthandPropertyAssignment(n.parent)?c.getShorthandAssignmentValueSymbol(n.parent):c.getSymbolAtLocation(n);
 const identifiers=node=>{const result=[];const walk=n=>{tick();if(ts.isIdentifier(n)){result.push(n);return;}if(ts.isPropertyAssignment(n)&&ts.isIdentifier(n.name)&&ts.isIdentifier(n.initializer)){walk(n.initializer);return;}ts.forEachChild(n,walk);};walk(node);return result;};
 const oldIdentifiers=identifiers(selected),free=new Map();for(let i=0;i<oldIdentifiers.length;i++){const n=oldIdentifiers[i],s=resolve(checker,symbolAt(checker,n));if(!s?.declarations?.length||s.declarations.some(d=>d.getSourceFile()===selected.getSourceFile()&&d.getStart()>=selected.getStart()&&d.end<=selected.end))continue;const key=symbolKey(checker,s);if(key)free.set(i,key);}
 const used=new Set(),targetNames=new Set();for(const sf of program.getSourceFiles())if(original.has(slash(path.resolve(sf.fileName)))){const walk=n=>{tick();if(ts.isIdentifier(n)){used.add(n.text);if(slash(path.resolve(sf.fileName))===destination)targetNames.add(n.text);}ts.forEachChild(n,walk);};walk(sf);}
 const importNames=n=>{const result=[];if(n.importClause?.name)result.push([n.importClause.name,'default']);const named=n.importClause?.namedBindings;if(named){if(ts.isNamespaceImport(named))result.push([named.name,'*']);else for(const item of named.elements)result.push([item.name,item.propertyName?.text||item.name.text]);}return result;};
 const originalImports=[];for(const node of program.getSourceFile(destination).statements)if(ts.isImportDeclaration(node))for(const [name]of importNames(node))originalImports.push([name.text,symbolKey(checker,checker.getSymbolAtLocation(name))]);let serial=1;
 for(let round=0;round<256;round++){
  const currentService=makeService();let next;
  try{
   const currentProgram=currentService.getProgram(),c=currentProgram.getTypeChecker(),sf=currentProgram.getSourceFile(destination);
   for(const node of sf.statements){
    if(!ts.isImportDeclaration(node))continue;const module=c.getSymbolAtLocation(node.moduleSpecifier);
    for(const [name,exported]of importNames(node)){
     if(!targetNames.has(name.text))continue;const imported=exported==='*'?module:module&&c.getExportsOfModule(module).find(s=>s.getName()===exported),key=symbolKey(c,imported);if(!key||originalImports.some(([n,k])=>n===name.text&&k===key))continue;
     const indices=[...free].filter(([i,k])=>k===key&&oldIdentifiers[i].text===name.text).map(([i])=>i);if(!indices.length)continue;
     const declarations=sf.statements.filter(n=>n.kind===selected.kind&&nameOf(n)?.text===env.input.intent.name);assert(declarations.length===1,'Move repaired declaration unavailable');const currentIdentifiers=identifiers(declarations[0]);assert(currentIdentifiers.length===oldIdentifiers.length,'Move copied identifier structure changed');
     while(used.has('__ci_move_'+serial)){serial++;assert(serial<=100_000,'Move fresh alias capacity');}const newName='__ci_move_'+serial++;used.add(newName);const references=indices.map(i=>{const n=currentIdentifiers[i];assert(n.text===name.text,'Move copied free reference spelling mismatch');return n;});
     const importText=ts.isImportSpecifier(name.parent)&&!name.parent.propertyName?name.getText(sf)+' as '+newName:newName;
     const changes=[{...span(sf,name.getStart(sf),name.end),text:importText},...references.map(n=>({...span(sf,n.getStart(sf),n.end),text:ts.isShorthandPropertyAssignment(n.parent)?shorthand(n.text,newName):newName}))];
     next={repair:{kind:'importAlias',importName:span(sf,name.getStart(sf),name.end),originalReferences:indices.map(i=>span(oldIdentifiers[i].getSourceFile(),oldIdentifiers[i].getStart(),oldIdentifiers[i].end)),currentReferences:references.map(n=>span(sf,n.getStart(sf),n.end)),newName},changes};break;
    }if(next)break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move alias repair capacity');
 }
 // The compiler can omit imports used only by object shorthand. Recover only
 // original captured import bindings; never infer a module from a bare name.
 const moduleTextFor=(modulePath,currentProgram)=>{
  const relative=slash(path.relative(path.dirname(destination),modulePath)),base=relative.startsWith('../')?relative:'./'+relative,stem=base.replace(/(?:\.d)?\.[cm]?[jt]sx?$/,'');
  const candidates=[stem,stem+'.js',stem+'.mjs',stem+'.cjs',base],host={fileExists:env.fileExists,readFile:env.read,directoryExists:env.directoryExists,getDirectories:env.getDirectories,realpath:p=>slash(path.resolve(p))};
  const name=candidates.find(candidate=>{const resolved=ts.resolveModuleName(candidate,destination,currentProgram.getCompilerOptions(),host).resolvedModule;return resolved&&slash(path.resolve(resolved.resolvedFileName))===modulePath;});assert(name!==undefined,'Move missing-import destination module specifier unavailable');return JSON.stringify(name);
 };
 const origins=[];for(const node of selected.getSourceFile().statements)if(ts.isImportDeclaration(node))for(const [name]of importNames(node))origins.push({node,name,symbol:checker.getSymbolAtLocation(name)});
 for(let round=0;round<256;round++){
  const currentService=makeService();let next;
  try{
   const currentProgram=currentService.getProgram(),c=currentProgram.getTypeChecker(),sf=currentProgram.getSourceFile(destination),declarations=sf.statements.filter(n=>n.kind===selected.kind&&nameOf(n)?.text===env.input.intent.name);
   assert(declarations.length===1,'Move missing-import declaration unavailable');const moved=declarations[0],currentIdentifiers=identifiers(moved);assert(currentIdentifiers.length===oldIdentifiers.length,'Move missing-import identifier structure changed');
   for(const origin of origins){
    const indices=[...free].filter(([i])=>symbolAt(checker,oldIdentifiers[i])===origin.symbol).map(([i])=>i);if(!indices.length)continue;
    if(indices.every(i=>symbolKey(c,symbolAt(c,currentIdentifiers[i]))===free.get(i)))continue;
    const module=checker.getSymbolAtLocation(origin.node.moduleSpecifier),moduleFile=module?.declarations?.find(ts.isSourceFile)?.fileName;assert(moduleFile&&original.has(slash(path.resolve(moduleFile))),'Move missing-import module must be captured');
    const moduleText=moduleTextFor(slash(path.resolve(moduleFile)),currentProgram);
    while(used.has('__ci_move_'+serial)){serial++;assert(serial<=100_000,'Move fresh alias capacity');}const newName='__ci_move_'+serial++;used.add(newName);
    const parent=origin.name.parent,originalName=span(origin.name.getSourceFile(),origin.name.getStart(),origin.name.end);let binding;
    if(ts.isImportSpecifier(parent))binding='{'+(parent.propertyName?parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName:parent.getText()+' as '+newName)+'}';
    else if(ts.isNamespaceImport(parent))binding=parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName;
    else {assert(ts.isImportClause(parent),'Move missing-import original binding shape');binding=newName;}
    const line=sf.text.includes('\r\n')?'\r\n':'\n',text=line+'import '+(origin.node.importClause.isTypeOnly?'type ':'')+binding+' from '+moduleText+(origin.node.attributes?' '+origin.node.attributes.getText():'')+';'+line,insertion=span(sf,moved.end,moved.end),references=indices.map(i=>currentIdentifiers[i]);
    const changes=[{...insertion,text},...references.map(n=>({...span(sf,n.getStart(sf),n.end),text:ts.isShorthandPropertyAssignment(n.parent)?shorthand(n.text,newName):newName}))];
    next={repair:{kind:'missingImport',originalImport:span(origin.node.getSourceFile(),origin.node.getStart(),origin.node.end),originalName,insertion,originalReferences:indices.map(i=>span(oldIdentifiers[i].getSourceFile(),oldIdentifiers[i].getStart(),oldIdentifiers[i].end)),currentReferences:references.map(n=>span(sf,n.getStart(sf),n.end)),newName,moduleText},changes};break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move missing-import repair capacity');
 }
 const locals=[];for(const node of selected.getSourceFile().statements)if(node!==selected)for(const declaration of ts.isVariableStatement(node)?node.declarationList.declarations:[node])if(declaration.name&&ts.isIdentifier(declaration.name))locals.push({name:declaration.name,symbol:checker.getSymbolAtLocation(declaration.name)});
 for(let round=0;round<256;round++){
  const currentService=makeService();let next;
  try{
   const currentProgram=currentService.getProgram(),c=currentProgram.getTypeChecker(),sf=currentProgram.getSourceFile(destination),originalSource=currentProgram.getSourceFile(file),moved=sf.statements.filter(n=>n.kind===selected.kind&&nameOf(n)?.text===env.input.intent.name);assert(moved.length===1,'Move source-dependency declaration unavailable');const ids=identifiers(moved[0]);assert(ids.length===oldIdentifiers.length,'Move source-dependency identifier structure changed');
   for(const local of locals){
    const indices=[...free].filter(([i])=>symbolAt(checker,oldIdentifiers[i])===local.symbol).map(([i])=>i);if(!indices.length||indices.every(i=>symbolKey(c,symbolAt(c,ids[i]))===free.get(i)))continue;
    const matches=originalSource.statements.flatMap(n=>(ts.isVariableStatement(n)?[...n.declarationList.declarations]:[n]).filter(d=>d.name&&ts.isIdentifier(d.name)&&d.name.text===local.name.text).map(d=>({statement:n,name:d.name})));assert(matches.length===1,'Move source-dependency original declaration unavailable');const dependency=matches[0],key=symbolKey(c,c.getSymbolAtLocation(dependency.name)),exported=originalSource.symbol&&c.getExportsOfModule(originalSource.symbol).some(s=>s.getName()===local.name.text&&symbolKey(c,s)===key);
    while(used.has('__ci_move_'+serial)){serial++;assert(serial<=100_000,'Move fresh alias capacity');}const newName='__ci_move_'+serial++;used.add(newName);const insertion=span(sf,moved[0].end,moved[0].end),exportInsertion=exported?null:span(originalSource,dependency.statement.end,dependency.statement.end),line=sf.text.includes('\r\n')?'\r\n':'\n',moduleText=moduleTextFor(file,currentProgram),references=indices.map(i=>ids[i]);
    const changes=[{...insertion,text:line+'import { '+local.name.text+' as '+newName+' } from '+moduleText+';'+line},...references.map(n=>({...span(sf,n.getStart(sf),n.end),text:ts.isShorthandPropertyAssignment(n.parent)?shorthand(n.text,newName):newName}))];
    if(exportInsertion){const sourceLine=originalSource.text.includes('\r\n')?'\r\n':'\n';changes.push({...exportInsertion,text:sourceLine+'export { '+local.name.text+' };'+sourceLine});}
    next={repair:{kind:'sourceDependency',originalDefinition:span(local.name.getSourceFile(),local.name.getStart(),local.name.end),currentDefinition:span(originalSource,dependency.name.getStart(),dependency.name.end),exportInsertion,insertion,originalReferences:indices.map(i=>span(oldIdentifiers[i].getSourceFile(),oldIdentifiers[i].getStart(),oldIdentifiers[i].end)),currentReferences:references.map(n=>span(sf,n.getStart(sf),n.end)),newName,moduleText},changes};break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move source-dependency repair capacity');
 }
 // Restore attributes omitted by the compiler. Final native binding/syntax
 // admission independently checks the original attribute policy for every target.
 const attributeOrigins=[];
 for(const node of selected.getSourceFile().statements)if(ts.isImportDeclaration(node)&&node.attributes)
  for(const [name]of importNames(node))attributeOrigins.push({node,key:symbolKey(checker,checker.getSymbolAtLocation(name))});
 for(let round=0;round<256;round++){
  const currentService=makeService();let next;
  try{
   const currentProgram=currentService.getProgram(),c=currentProgram.getTypeChecker(),sf=currentProgram.getSourceFile(destination);
   for(const node of sf.statements){
    tick();if(!ts.isImportDeclaration(node)||node.attributes)continue;
    const names=importNames(node);if(!names.length)continue;
    const candidates=names.map(([name])=>attributeOrigins.filter(o=>o.key&&o.key===symbolKey(c,c.getSymbolAtLocation(name))));
    if(!candidates.some(a=>a.length))continue;
    assert(candidates.every(a=>a.length),'Move cannot merge attributed and unattributed import requests');
    const suffixes=new Set(candidates.flat().map(o=>o.node.attributes.getText()));
    assert(suffixes.size===1,'Move ambiguous original import attributes');
    const origin=candidates[0][0].node;
    next={repair:{kind:'importAttributes',originalImport:span(origin.getSourceFile(),origin.getStart(),origin.end),currentImport:span(sf,node.getStart(sf),node.end)},changes:[{...span(sf,node.moduleSpecifier.end,node.moduleSpecifier.end),text:' '+origin.attributes.getText()}]};break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move attribute repair capacity');
 }
 if(!repairs.length)return{repairs,edits:compilerEdits};const edits=[];
 for(const p of [...tables.keys()].sort((a,b)=>Buffer.compare(Buffer.from(a),Buffer.from(b)))){let cursor=0,pending='';const emit=end=>{if(cursor!==end||pending){if(raw(p).subarray(cursor,end).toString('utf8')!==pending)edits.push({path:p,startByte:cursor,endByte:end,text:pending});}pending='';};for(const part of table(p)){if(part.text!==undefined){pending+=part.text;continue;}assert(part.start>=cursor,'Move original pieces reordered');emit(part.start);cursor=part.end;}emit(raw(p).length);}
 assert(edits.length<=2000,'Move composed operation capacity');return{repairs,edits};
}
