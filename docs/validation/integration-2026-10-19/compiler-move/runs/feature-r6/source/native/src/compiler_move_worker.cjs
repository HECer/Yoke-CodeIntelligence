'use strict';
// Compiler analysis over the same immutable virtual filesystem as analyze/build.
function moveSymbolToFile(env) {
 const {input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations}=env;
 const intent=input.intent,file=abs(intent.path),destination=abs(intent.destination),text=source.get(file);
 assert(typeof text==='string'&&source.has(destination)&&file!==destination,'Move requires two different captured files');
 const offset=bytes=>{assert(Number.isSafeInteger(bytes)&&bytes>=0&&bytes<=Buffer.byteLength(text),'Move byte offset');const raw=Buffer.from(text).subarray(0,bytes),value=raw.toString('utf8');assert(Buffer.from(value).equals(raw),'Move UTF-8 boundary');return value.length;};
 const start=offset(intent.startByte),end=offset(intent.endByte);
 if(!projects.size){limitations.push('No configured project: inferred context is nonexhaustive.');projects.set('[inferred]',{name:'[inferred]',problems:[],parsed:{fileNames:[...source.keys()].filter(p=>/\.[cm]?[jt]sx?$/.test(p)),options:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,allowJs:true,checkJs:true,jsx:ts.JsxEmit.Preserve,noEmit:true},projectReferences:[]}});}
 const contexts=[...projects.values()].filter(p=>p.parsed.fileNames.some(f=>slash(path.resolve(f))===file));
 assert(contexts.length===1,'Move requires exactly one captured compiler context for its source');const context=contexts[0];
 assert(context.parsed.fileNames.some(f=>slash(path.resolve(f))===destination),'Move destination must belong to the same captured compiler context');
 assert([...projects.values()].filter(p=>p.parsed.fileNames.some(f=>slash(path.resolve(f))===destination)).length===1,'Move destination has ambiguous compiler contexts');
 assert([...projects.values()].every(p=>!p.problems.some(d=>d.severity==='error')),'Move refuses incomplete project configurations/references');
 const makeService=()=>ts.createLanguageService({getScriptFileNames:()=>context.parsed.fileNames,getScriptVersion:()=> '1',getScriptSnapshot:p=>{const value=read(p);return value===undefined?undefined:ts.ScriptSnapshot.fromString(value);},getCompilationSettings:()=>({...context.parsed.options,noEmit:true}),getProjectReferences:()=>context.parsed.projectReferences,useSourceOfProjectReferenceRedirect:()=>true,getCurrentDirectory:()=>root,getDefaultLibFileName:o=>ts.getDefaultLibFilePath(o),fileExists,readFile:read,directoryExists,getDirectories,readDirectory,realpath:p=>slash(path.resolve(p)),useCaseSensitiveFileNames:()=>true});
 const original=new Map(source),service=makeService();
 let visited=0;
 const inspect=(program)=>{
  const checker=program.getTypeChecker(),bindings=[],modules=[];
  const target=symbol=>{
   if(!symbol)return{resolution:'unbound',targets:[]};
   if(symbol.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
   const targets=[];
   for(const declaration of symbol.declarations||[]){
    const sf=declaration.getSourceFile(),p=slash(path.resolve(sf.fileName));
    if(ts.isSourceFile(declaration)){assert(source.has(p),'Move module binding outside capture');targets.push({kind:'module',path:rel(p)});continue;}
    const named=declaration.name&&typeof declaration.name.getStart==='function'?declaration.name:declaration;
    const location=span(sf,named.getStart(sf),named.end);
    if(source.has(p))targets.push({kind:'captured',span:location});
    else {assert(libCache.has(p),'Move definition is outside captured/trusted-library scope');targets.push({kind:'library',name:path.basename(p),hash:sha(sf.text),startByte:location.startByte,endByte:location.endByte,text:location.text});}
   }
   assert(targets.length<=128,'Move merged declaration capacity');
   return{resolution:targets.length?'resolved':'no_declarations',targets};
  };
  for(const sf of program.getSourceFiles()){
   const p=slash(path.resolve(sf.fileName));if(!source.has(p))continue;
   const imports=[];
   const walk=node=>{
    assert(++visited<=1_000_000,'Move compiler AST capacity');
    if(ts.isIdentifier(node)){
     const symbol=ts.isShorthandPropertyAssignment(node.parent)?checker.getShorthandAssignmentValueSymbol(node.parent):checker.getSymbolAtLocation(node);
     bindings.push({span:span(sf,node.getStart(sf),node.end),...target(symbol)});assert(bindings.length<=100_000,'Move binding capacity');
    }
    ts.forEachChild(node,walk);
   };walk(sf);
   for(const node of sf.statements){
    if(!(ts.isImportDeclaration(node)||ts.isExportDeclaration(node))||!node.moduleSpecifier)continue;
    const moduleSymbol=checker.getSymbolAtLocation(node.moduleSpecifier),moduleTarget=target(moduleSymbol);
    const names=[];if(node.importClause){
     if(node.importClause.name)names.push(node.importClause.name);
     const named=node.importClause.namedBindings;
     if(named){if(ts.isNamespaceImport(named))names.push(named.name);else for(const item of named.elements)names.push(item.name);}
    }
    if(ts.isExportDeclaration(node)&&node.exportClause){if(ts.isNamedExports(node.exportClause)){for(const item of node.exportClause.elements)names.push(item.name);}else names.push(node.exportClause.name);}
    imports.push({kind:ts.isImportDeclaration(node)?'import':'reexport',span:span(sf,node.getStart(sf),node.end),module:moduleTarget,names:names.map(name=>({span:span(sf,name.getStart(sf),name.end),...target(checker.getSymbolAtLocation(name))}))});
   }
   const exports=sf.symbol?checker.getExportsOfModule(sf.symbol).map(symbol=>({name:symbol.getName(),...target(symbol)})).sort((a,b)=>a.name.localeCompare(b.name)):[];
   modules.push({path:rel(p),isModule:ts.isExternalModule(sf),exports,imports});
  }
  return{bindings,modules};
 };
 try{
  const program=service.getProgram(),sf=program.getSourceFile(file),targetFile=program.getSourceFile(destination);
  assert(sf&&targetFile&&ts.isExternalModule(sf)&&ts.isExternalModule(targetFile)&&!sf.commonJsModuleIndicator&&!targetFile.commonJsModuleIndicator,'Move requires two captured ES modules');
  const selected=sf.statements.filter(node=>node.getStart(sf)===start&&node.end===end);assert(selected.length===1,'Move compiler statement differs from native selection');
  const selectedName=ts.isVariableStatement(selected[0])?selected[0].declarationList.declarations[0]?.name:selected[0].name;
  assert(selectedName&&ts.isIdentifier(selectedName)&&selectedName.text===intent.name&&program.getTypeChecker().getSymbolAtLocation(selectedName)?.declarations?.length===1,'Move selected declaration is merged or ambiguous');
  const beforeDiagnostics=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));assert(beforeDiagnostics.length<=20_000&&!beforeDiagnostics.some(d=>d.severity==='error'),'Move compiler context has errors before refactoring');
  const before=inspect(program),preferences={allowTextChangesInNewFiles:true,provideRefactorNotApplicableReason:true};
  const available=service.getApplicableRefactors(file,{pos:start,end},preferences,'invoked','refactor.move.file',true);
  const actions=available.flatMap(refactor=>refactor.actions.filter(action=>action.kind==='refactor.move.file'&&!action.notApplicableReason).map(action=>({refactor,action})));assert(actions.length===1,'Selected compiler has no unambiguous Move to file action');
  const {refactor,action}=actions[0];
  const response=service.getEditsForRefactor(file,{newLineCharacter:text.includes('\r\n')?'\r\n':'\n'},{pos:start,end},refactor.name,action.name,preferences,{targetFile:destination});
  assert(response&&!response.notApplicableReason&&!response.commands&&!response.renameFilename&&!response.renameLocation&&response.edits.length<=2000,'Move compiler response has unsupported effects');
  const rawEdits=[],edits=[],resultHashes={};let bytes=0;
  const seen=new Set();
  for(const changed of response.edits){
   const p=slash(path.resolve(changed.fileName));assert(source.has(p)&&!changed.isNewFile&&!seen.has(p),'Move cannot create resources or repeat a document');seen.add(p);const inputText=source.get(p),unit=program.getSourceFile(p);assert(unit,'Move edit document is outside compiler program');
   const changes=changed.textChanges.map((change,ordinal)=>({path:rel(p),startByte:Buffer.byteLength(inputText.slice(0,change.span.start)),endByte:Buffer.byteLength(inputText.slice(0,change.span.start+change.span.length)),text:change.newText,ordinal}));rawEdits.push(...changes);
   assert(rawEdits.length<=2000,'Move edit capacity');
   const normalized=[];
   for(const change of changes){
    const previous=normalized.at(-1);
    if(previous&&change.startByte===previous.startByte){assert(previous.startByte===previous.endByte,'Move ambiguous same-offset edit order');previous.endByte=change.endByte;previous.text+=change.text;}
    else {assert(!previous||previous.endByte<=change.startByte,'Move compiler edits must be ordered and nonoverlapping');normalized.push({path:change.path,startByte:change.startByte,endByte:change.endByte,text:change.text});}
   }
   if(inputText.startsWith('\ufeff')){
    for(const change of normalized)if(change.startByte===0){change.startByte=3;change.endByte=Math.max(3,change.endByte);}
    for(let i=1;i<normalized.length;i++)if(normalized[i-1].startByte===normalized[i].startByte){const previous=normalized[i-1],change=normalized[i];assert(previous.startByte===previous.endByte,'Move BOM normalization overlaps replacement');previous.endByte=change.endByte;previous.text+=change.text;normalized.splice(i--,1);}
   }
   let output=Buffer.from(inputText);
   for(const change of [...normalized].reverse())output=Buffer.concat([output.subarray(0,change.startByte),Buffer.from(change.text),output.subarray(change.endByte)]);
   bytes+=output.length;assert(output.length<=4_000_000&&bytes<=16_000_000,'Move result capacity');const value=output.toString('utf8');assert(Buffer.from(value).equals(output),'Move result UTF-8');source.set(p,value);edits.push(...normalized);resultHashes[rel(p)]=sha(output);
  }
  assert(seen.has(file)&&seen.has(destination),'Move did not change both selected files');
  const afterService=makeService();let after,afterDiagnostics;
  try{const program=afterService.getProgram();afterDiagnostics=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));assert(afterDiagnostics.length<=20_000&&!afterDiagnostics.some(d=>d.severity==='error'),'Move compiler context has errors after refactoring');after=inspect(program);}finally{afterService.dispose();}
  const libraries=Object.fromEntries([...libCache].filter(([,value])=>value!==undefined).map(([p,value])=>[path.basename(p),sha(value)]).sort(([a],[b])=>a.localeCompare(b)));
  const options=JSON.parse(JSON.stringify(context.parsed.options,(key,value)=>key==='configFile'?undefined:typeof value==='string'?value.replaceAll(root,'[snapshot]'):value));
  const contextId=sha(JSON.stringify([input.snapshotId,context.name,context.parsed.fileNames.map(rel).sort(),options,ts.version,compilerHash,libraries]));
  return{schema:1,operation:'move',snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,sourceBindings:input.sourceBindings,sourceExecuted:false,sourceTreeWritten:false,exhaustive:false,status:'passed',contexts:[],diagnostics:[],outputs:[],standardLibraryHashes:libraries,limitations:[...limitations,'Known static compiler bindings are checked; dynamic/omitted consumers and module evaluation order remain unverified.'],move:{schema:1,normalization:'ordered_insertions_and_original_bom_v1',config:context.name,contextId,actionKind:action.kind,before,after,rawEdits,edits,resultHashes,beforeDiagnostics,afterDiagnostics}};
 }finally{source.clear();for(const [p,value]of original)source.set(p,value);service.dispose();}
}
