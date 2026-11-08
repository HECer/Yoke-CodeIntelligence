'use strict';
// Invoked only by the immutable compiler worker, never by analyzed code.
function inlineVariable(env) {
 const {input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations}=env;
 const intent=input.intent,text=source.get(abs(intent.path));
 assert(typeof text==='string'&&typeof intent.keepDefinition==='boolean','Inline captured source missing');
 const offset=bytes=>{assert(Number.isSafeInteger(bytes)&&bytes>=0&&bytes<=Buffer.byteLength(text),'Inline byte offset');const original=Buffer.from(text).subarray(0,bytes),prefix=original.toString('utf8');assert(Buffer.from(prefix).equals(original),'Inline UTF-8 boundary');return prefix.length;};
 const position=offset(intent.nameStartByte),end=offset(intent.nameEndByte);
 assert(text.slice(position,end)===intent.name,'Inline name source mismatch');
 if(!projects.size){limitations.push('No configured project: inferred context is nonexhaustive.');projects.set('[inferred]',{name:'[inferred]',problems:[],parsed:{fileNames:[...source.keys()].filter(p=>/\.[cm]?[jt]sx?$/.test(p)),options:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,allowJs:true,checkJs:true,jsx:ts.JsxEmit.Preserve,noEmit:true},projectReferences:[]}});}
 const contexts=[...projects.values()].filter(p=>p.parsed.fileNames.some(f=>slash(path.resolve(f))===abs(intent.path)));
 assert(contexts.length===1,'Inline requires exactly one captured compiler context for the selected file');
 const context=contexts[0];
 assert(!context.problems.some(d=>d.severity==='error'),'Inline project configuration has errors');
 const makeService=()=>ts.createLanguageService({
  getScriptFileNames:()=>context.parsed.fileNames,getScriptVersion:()=> '1',
  getScriptSnapshot:file=>{const value=read(file);return value===undefined?undefined:ts.ScriptSnapshot.fromString(value);},
  getCompilationSettings:()=>({...context.parsed.options,noEmit:true}),getProjectReferences:()=>context.parsed.projectReferences,
  useSourceOfProjectReferenceRedirect:()=>true,getCurrentDirectory:()=>root,
  getDefaultLibFileName:options=>ts.getDefaultLibFilePath(options),
  fileExists,readFile:read,directoryExists,getDirectories,readDirectory,
  realpath:p=>slash(path.resolve(p)),useCaseSensitiveFileNames:()=>true,
 });
 const service=makeService();
 try {
  const file=abs(intent.path),program=service.getProgram(),sf=program.getSourceFile(file),checker=program.getTypeChecker();
  assert(sf,'Inline source absent from selected compiler context');
  const before=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));
  assert(before.length<=20_000&&!before.some(d=>d.severity==='error'),'Inline compiler context has errors before refactoring');
  let selected,nodes=0;
  const walk=(node,callback)=>{assert(++nodes<=500_000,'Inline compiler AST capacity');callback(node);ts.forEachChild(node,child=>walk(child,callback));};
  walk(sf,node=>{if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.getStart(sf)===position&&node.name.end===end)selected=node;});
  assert(selected&&selected.initializer&&(ts.isArrowFunction(selected.initializer)||ts.isFunctionExpression(selected.initializer)),'Inline requires a captured function-valued variable declaration');
  assert(Buffer.byteLength(text.slice(0,selected.getStart(sf)))===intent.startByte&&Buffer.byteLength(text.slice(0,selected.end))===intent.endByte,'Inline compiler declaration differs from captured entity');
  const symbol=checker.getSymbolAtLocation(selected.name);
  assert(symbol&&symbol.declarations?.length===1&&symbol.declarations[0]===selected,'Inline declaration binding is ambiguous');
  const references=[];
  for(const unit of program.getSourceFiles()){
   if(!source.has(slash(path.resolve(unit.fileName))))continue;
   walk(unit,node=>{
    if(!ts.isIdentifier(node)||node===selected.name)return;
    const bound=ts.isShorthandPropertyAssignment(node.parent)?checker.getShorthandAssignmentValueSymbol(node.parent):checker.getSymbolAtLocation(node);
    if(bound!==symbol)return;
    assert(unit===sf,'Inline refuses consumers outside the selected captured file');
    assert(node.getStart(sf)<selected.getStart(sf)||node.end>selected.end,'Recursive initializer cannot be inlined');
    references.push(node);assert(references.length<=1999,'Inline reference capacity');
   });
  }
  assert(references.length,'Inline has no compiler-bound consumers');
  // Reject the compiler's otherwise legal text substitution when moving a free
  // identifier would bind it to a different declaration in the consumer scope.
  walk(selected.initializer,node=>{
   assert(node.kind!==ts.SyntaxKind.ThisKeyword&&node.kind!==ts.SyntaxKind.SuperKeyword&&!ts.isMetaProperty(node),'Inline lexical this/super/meta binding is unsupported');
   if(!ts.isIdentifier(node))return;
   const parent=node.parent;
   if((ts.isPropertyAccessExpression(parent)&&parent.name===node)||
      ((ts.isPropertyAssignment(parent)||ts.isPropertySignature(parent)||ts.isMethodDeclaration(parent))&&parent.name===node))return;
   const bound=ts.isShorthandPropertyAssignment(parent)?checker.getShorthandAssignmentValueSymbol(parent):checker.getSymbolAtLocation(node);
   if(!bound)return;
   if(bound.declarations?.some(d=>d.getSourceFile()===sf&&d.pos>=selected.initializer.pos&&d.end<=selected.initializer.end))return;
   assert(node.text!=='arguments','Inline lexical arguments binding is unsupported');
   for(const ref of references){
    assert(checker.resolveName(node.text,ref,ts.SymbolFlags.Value|ts.SymbolFlags.Type|ts.SymbolFlags.Namespace,false)===bound,'Inline would change a free identifier binding at a consumer');
   }
  });
  const available=service.getApplicableRefactors(file,position,{provideRefactorNotApplicableReason:true},'invoked','refactor.inline');
  const actions=available.flatMap(refactor=>refactor.actions.filter(action=>action.kind==='refactor.inline.variable'&&!action.notApplicableReason&&!action.isInteractive).map(action=>({refactor,action})));
  assert(actions.length===1,'Selected compiler has no unambiguous Inline variable refactoring');
  const {refactor,action}=actions[0];
  const changes=service.getEditsForRefactor(file,{newLineCharacter:text.includes('\r\n')?'\r\n':'\n'},position,refactor.name,action.name,{});
  assert(changes&&changes.edits.length===1&&changes.edits[0].fileName===file&&!changes.edits[0].isNewFile&&!changes.commands,'Inline compiler returned unsupported file or command effects');
  const edits=changes.edits[0].textChanges.map(change=>({...span(sf,change.span.start,change.span.start+change.span.length),text:change.newText}));
  assert(edits.length>=2&&edits.length<=2000,'Inline compiler edit capacity');
  const removal=edits.filter(edit=>edit.startByte<=intent.startByte&&edit.endByte>=intent.endByte&&edit.text==='');
  assert(removal.length===1,'Inline compiler declaration removal is ambiguous');
  let result=text;
  const effective=edits.filter(edit=>!intent.keepDefinition||edit!==removal[0]);
  for(const edit of [...effective].sort((a,b)=>b.startByte-a.startByte)){
   const before=Buffer.from(result);result=Buffer.concat([before.subarray(0,edit.startByte),Buffer.from(edit.text),before.subarray(edit.endByte)]).toString('utf8');
   assert(Buffer.byteLength(result)<=4_000_000,'Inline result capacity');
  }
  const libraries=()=>Object.fromEntries([...libCache].filter(([,value])=>value!==undefined).map(([p,value])=>[path.basename(p),sha(value)]).sort(([a],[b])=>a.localeCompare(b)));
  source.set(file,result);
  const afterService=makeService();let after;
  try{after=ts.getPreEmitDiagnostics(afterService.getProgram()).map(d=>diagnostic(d,context.name));}finally{afterService.dispose();source.set(file,text);}
  assert(after.length<=20_000&&!after.some(d=>d.severity==='error'),'Inline compiler context has errors after refactoring');
  const options=JSON.parse(JSON.stringify(context.parsed.options,(key,value)=>key==='configFile'?undefined:typeof value==='string'?value.replaceAll(root,'[snapshot]'):value));
  const contextId=sha(JSON.stringify([input.snapshotId,context.name,options,context.parsed.fileNames.map(rel).sort(),ts.version,compilerHash,libraries()]));
  return {schema:1,operation:'inline',snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,sourceBindings:input.sourceBindings,sourceExecuted:false,sourceTreeWritten:false,exhaustive:false,status:'passed',contexts:[],diagnostics:[],outputs:[],standardLibraryHashes:libraries(),limitations,
   inline:{schema:1,path:intent.path,keepDefinition:intent.keepDefinition,declaration:span(sf,selected.getStart(sf),selected.end),references:references.map(node=>span(sf,node.getStart(sf),node.end)),edits,config:context.name,contextId,actionKind:action.kind,beforeDiagnostics:before,afterDiagnostics:after,resultHash:sha(result)}};
 }finally{service.dispose();}
}
