'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const sha=v=>crypto.createHash('sha256').update(v).digest('hex');
const assert=(ok,message)=>{if(!ok)throw new Error(message);};
function boundedRead(file,limit){
 const fd=fs.openSync(file,'r'),chunks=[];let total=0;
 try{const buffer=Buffer.alloc(65536);for(;;){const count=fs.readSync(fd,buffer,0,buffer.length,null);if(!count)break;total+=count;assert(total<=limit,'Trusted compiler file capacity');chunks.push(Buffer.from(buffer.subarray(0,count)));}return Buffer.concat(chunks,total);}finally{fs.closeSync(fd);}
}
let raw=Buffer.alloc(0);
process.stdin.on('data',chunk=>{raw=Buffer.concat([raw,chunk]);assert(raw.length<=16_000_000,'Compiler input bound');});
process.stdin.on('end',()=>{try{process.stdout.write(JSON.stringify(run(JSON.parse(raw.toString('utf8')))));}catch(error){process.stderr.write(String(error.message).slice(0,2000));process.exitCode=1;}});
function run(input){
 const compilerHash=sha(boundedRead(input.compiler,16_000_000));
 const ts=require(input.compiler);
 assert(ts.version==='5.8.3' && typeof ts.matchFiles==='function','Unsupported host TypeScript version');
 assert(input.schema===1 && ['analyze','build','inline','move'].includes(input.operation),'Compiler request contract');
 const slash=p=>p.replaceAll('\\','/'),root=slash(path.resolve(process.cwd(),'virtual'));
 const library=slash(path.dirname(ts.getDefaultLibFilePath({target:ts.ScriptTarget.ES2022})));
 const abs=p=>slash(path.resolve(root,p));
 const rel=p=>{const r=slash(path.relative(root,path.resolve(p)));return !r||r==='..'||r.startsWith('../')||path.isAbsolute(r)?undefined:r;};
 const source=new Map(Object.entries(input.contents).map(([p,t])=>[abs(p),t])),outputs=new Map(),libCache=new Map();
 let directories=new Map(),dirty=true,outputBytes=0;
 const rebuild=()=>{
  if(!dirty)return;directories=new Map([[root,{files:new Set(),directories:new Set()}]]);
  for(const file of [...source.keys(),...[...outputs.keys()].map(abs)]){
   let child=file,dir=slash(path.dirname(child)),isFile=true;
   while(dir===root||dir.startsWith(root+'/')){
    const entry=directories.get(dir)||{files:new Set(),directories:new Set()};
    (isFile?entry.files:entry.directories).add(path.basename(child));directories.set(dir,entry);
    if(dir===root)break;child=dir;dir=slash(path.dirname(child));isFile=false;
   }
  }dirty=false;
 };
 const read=p=>{
  const file=slash(path.resolve(p));if(source.has(file))return source.get(file);
  const r=rel(file);if(r&&outputs.has(r))return outputs.get(r);
  // Never fall through to the repository or arbitrary host files.
  if(slash(path.dirname(file))!==library||!/^lib(?:\.[a-z0-9.-]+)?\.d\.ts$/i.test(path.basename(file)))return undefined;
  if(!libCache.has(file)){try{const data=boundedRead(file,4_000_000).toString('utf8');libCache.set(file,data);}catch(error){if(error.code==='ENOENT')libCache.set(file,undefined);else throw error;}}
  return libCache.get(file);
 };
 const fileExists=p=>read(p)!==undefined;
 const directoryExists=p=>{rebuild();return directories.has(slash(path.resolve(p)))||slash(path.resolve(p))===library;};
 const getDirectories=p=>{rebuild();return [...(directories.get(slash(path.resolve(p)))?.directories||[])].sort();};
 const readDirectory=(p,extensions,excludes,includes,depth)=>{rebuild();return ts.matchFiles(slash(path.resolve(p)),extensions,excludes,includes,true,root,depth,dir=>{const e=directories.get(slash(path.resolve(dir)));return{files:[...(e?.files||[])].sort(),directories:[...(e?.directories||[])].sort()};},p=>slash(path.resolve(p)));};
 const limitations=[],diagnostics=[],projects=new Map();
 const span=(sf,start,end)=>({path:rel(sf.fileName),startByte:Buffer.byteLength(sf.text.slice(0,start)),endByte:Buffer.byteLength(sf.text.slice(0,end)),text:sf.text.slice(start,end)});
 const diagnostic=(d,config)=>{
  const value={config,code:d.code,severity:d.category===ts.DiagnosticCategory.Error?'error':'warning',message:ts.flattenDiagnosticMessageText(d.messageText,'\n').replaceAll(root,'[snapshot]').slice(0,8000)};
  if(d.file&&rel(d.file.fileName)&&source.has(slash(path.resolve(d.file.fileName))))Object.assign(value,span(d.file,d.start||0,(d.start||0)+(d.length||0)));
  else value.path=config;return value;
 };
 const problem=(config,code,message)=>({config,path:config,code,severity:'error',message});
 const names=Object.keys(input.contents).filter(p=>!p.split('/').includes('node_modules')&&/(?:^|\/)(?:tsconfig|jsconfig)\.json$/.test(p)).filter(p=>!p.endsWith('jsconfig.json')||!source.has(abs(p.slice(0,-13)+'tsconfig.json'))).sort();
 const queue=[...names];
 for(let i=0;i<queue.length;i++){
  const name=queue[i];if(projects.has(name))continue;assert(projects.size<128,'Compiler project capacity');
  const errors=[];
  const parsed=ts.getParsedCommandLineOfConfigFile(abs(name),{},{useCaseSensitiveFileNames:true,getCurrentDirectory:()=>root,fileExists,readFile:read,readDirectory,onUnRecoverableConfigFileDiagnostic:d=>errors.push(d)})||{fileNames:[],options:{},errors:[]};
  errors.push(...parsed.errors);const p={name,parsed,problems:errors.map(d=>diagnostic(d,name))};projects.set(name,p);
  if(parsed.options.plugins?.length)limitations.push(name+': compiler plugins are not loaded.');
  parsed.projectReferences=(parsed.projectReferences||[]).filter(ref=>{
   const target=rel(/\.json$/i.test(ref.path)?ref.path:path.resolve(ref.path,'tsconfig.json'));
   if(!target||!source.has(abs(target))){p.problems.push(problem(name,'YOKE_REFERENCE_UNAVAILABLE','Referenced project is outside the immutable snapshot or missing.'));return false;}
   ref.path=abs(target);queue.push(target);return true;
  });
 }
 const active=new Set(),done=new Set();
 const visit=name=>{
  if(done.has(name))return;active.add(name);const p=projects.get(name);
  p.parsed.projectReferences=(p.parsed.projectReferences||[]).filter(ref=>{const target=rel(ref.path);if(active.has(target)){p.problems.push(problem(name,'YOKE_REFERENCE_CYCLE','Circular project reference: '+target));return false;}visit(target);return true;});
  active.delete(name);done.add(name);
 };
 for(const name of projects.keys())visit(name);
 if(input.operation==='inline')return inlineVariable({input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations});
 if(input.operation==='move')return moveSymbolToFile({input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations});
 const contexts=[];let status='passed',exitCode=null;
 if(input.operation==='analyze'){
  if(!projects.size){limitations.push('No configured project: inferred context is nonexhaustive.');projects.set('[inferred]',{name:'[inferred]',problems:[],parsed:{fileNames:[...source.keys()].filter(p=>/\.[cm]?[jt]sx?$/.test(p)),options:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,allowJs:true,checkJs:true,jsx:ts.JsxEmit.Preserve,noEmit:true},projectReferences:[]}});}
  let count=0;
  for(const p of projects.values()){
   const host=ts.createCompilerHost(p.parsed.options,true);
   host.useSourceOfProjectReferenceRedirect=()=>true;
   Object.assign(host,{readFile:read,fileExists,directoryExists,getDirectories,readDirectory,getCurrentDirectory:()=>root,getCanonicalFileName:p=>slash(path.resolve(p)),realpath:p=>slash(path.resolve(p)),useCaseSensitiveFileNames:()=>true,writeFile:()=>{throw new Error('Analysis cannot emit');},getSourceFile:(file,version)=>{const text=read(file);return text===undefined?undefined:ts.createSourceFile(file,text,version,true);}});
   const program=ts.createProgram({rootNames:p.parsed.fileNames,options:{...p.parsed.options,noEmit:true},projectReferences:p.parsed.projectReferences,host});
   const checker=program.getTypeChecker(),errors=[...p.problems,...ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,p.name))];diagnostics.push(...errors);
   const bindings=[];
   for(const sf of program.getSourceFiles()){
    if(!source.has(slash(path.resolve(sf.fileName))))continue;
    const walk=node=>{
     assert(++count<=500_000,'Compiler AST capacity');
     if(ts.isIdentifier(node)){
      let symbol=checker.getSymbolAtLocation(node);if(symbol?.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
      const definitions=[];
      for(const declaration of symbol?.declarations||[]){
       const target=declaration.getSourceFile();if(!source.has(slash(path.resolve(target.fileName))))continue;
       const named=declaration.name&&typeof declaration.name.getStart==='function'?declaration.name:declaration;
       definitions.push(span(target,named.getStart(target),named.getEnd()));
      }
      bindings.push({...span(sf,node.getStart(sf),node.getEnd()),type:checker.typeToString(checker.getTypeAtLocation(node)).replaceAll(root,'[snapshot]').slice(0,2000),definitions});
      assert(bindings.length<=100_000,'Compiler binding capacity');
     }ts.forEachChild(node,walk);
    };walk(sf);
   }
   const options=JSON.parse(JSON.stringify(p.parsed.options,(key,value)=>key==='configFile'?undefined:typeof value==='string'?value.replaceAll(root,'[snapshot]'):value));
   const files=p.parsed.fileNames.map(rel).filter(Boolean).sort();
   const libraries=Object.fromEntries(program.getSourceFiles().filter(sf=>libCache.has(slash(path.resolve(sf.fileName)))).map(sf=>[path.basename(sf.fileName),sha(sf.text)]).sort(([a],[b])=>a.localeCompare(b)));
   contexts.push({config:p.name,contextId:sha(JSON.stringify([input.snapshotId,p.name,options,files,ts.version,compilerHash,libraries])),options,files,standardLibraryHashes:libraries,status:errors.some(d=>d.severity==='error')?'failed':'passed',diagnostics:errors,bindings});
  }
  status=diagnostics.some(d=>d.severity==='error')?'failed':contexts.some(c=>c.files.length)?'passed':'incomplete';
 }else{
  for(const p of projects.values())diagnostics.push(...p.problems);
  if(diagnostics.some(d=>d.severity==='error'))status='failed';
  else if(!projects.size||![...projects.values()].some(p=>p.parsed.fileNames.length))status='incomplete';
  else{
   const local=p=>{const r=rel(p);assert(r,'Build output outside virtual root');return r;};
   const system={args:[],newLine:'\n',useCaseSensitiveFileNames:true,write:()=>{},getCurrentDirectory:()=>root,getExecutingFilePath:()=>input.compiler,readFile:read,fileExists,directoryExists,getDirectories,readDirectory,realpath:p=>slash(path.resolve(p)),createDirectory:p=>{local(p);},writeFile:(file,text)=>{const p=local(file);assert(!source.has(abs(p)),'Build overwrites input');outputBytes-=Buffer.byteLength(outputs.get(p)||'');outputBytes+=Buffer.byteLength(text);assert(outputBytes<=20_000_000&&(outputs.has(p)||outputs.size<20_000),'Build output capacity');outputs.set(p,text);dirty=true;},deleteFile:file=>{const p=local(file);outputBytes-=Buffer.byteLength(outputs.get(p)||'');outputs.delete(p);dirty=true;},getModifiedTime:p=>fileExists(p)?new Date(outputs.has(rel(p))?2000:1000):undefined,setModifiedTime:()=>{},exit:()=>{throw new Error('Compiler process exit denied');}};
   const host=ts.createSolutionBuilderHost(system,undefined,d=>diagnostics.push(diagnostic(d,'[solution-build]')),()=>{},()=>{});
   host.getParsedCommandLine=config=>projects.get(rel(config))?.parsed;
   exitCode=ts.createSolutionBuilder(host,[...projects.keys()].map(abs),{force:true}).build();
   status=exitCode===ts.ExitStatus.Success&&!diagnostics.some(d=>d.severity==='error')?'passed':'failed';
   if(!outputs.size&&status==='passed'){status='incomplete';limitations.push('Solution produced no generated artifacts.');}
  }
 }
 assert(diagnostics.length<=20_000,'Compiler diagnostic capacity');
 const grouped=new Map();
 for(const context of contexts)for(const b of context.bindings){const key=[b.path,b.startByte,b.endByte].join(':');const entries=grouped.get(key)||[];entries.push({contextId:context.contextId,definitions:b.definitions,type:b.type});grouped.set(key,entries);}
 const bindingConflicts=[...grouped].filter(([,entries])=>new Set(entries.map(e=>JSON.stringify([e.definitions,e.type]))).size>1).map(([anchor,contexts])=>({anchor,contexts}));
 limitations.push('Only captured files and trusted standard libraries are visible; unavailable dependencies, dynamic behavior and other build configurations remain outside this report.');
 return{schema:1,operation:input.operation,snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,standardLibraryHashes:Object.fromEntries([...libCache].filter(([,text])=>text!==undefined).map(([p,text])=>[path.basename(p),sha(text)]).sort(([a],[b])=>a.localeCompare(b))),sourceBindings:input.sourceBindings,status,exitCode,contexts,diagnostics,bindingConflicts,limitations,outputs:[...outputs].sort(([a],[b])=>a.localeCompare(b)).map(([p,content])=>({path:p,hash:sha(content),bytes:Buffer.byteLength(content),content})),sourceExecuted:false,sourceTreeWritten:false,exhaustive:false};
}

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
   if((ts.isJsxOpeningElement(parent)||ts.isJsxSelfClosingElement(parent)||ts.isJsxClosingElement(parent))&&parent.tagName===node&&(/^[a-z]/.test(node.text)||node.text.includes('-')))return;
   if((ts.isJsxAttribute(parent)&&parent.name===node)||ts.isJsxNamespacedName(parent))return;
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
  // The pinned compiler may delete the last declarator's comma separately.
  // Rust independently admits the complete contiguous removal bundle before
  // allowing keepDefinition to omit any of its original-coordinate pieces.
  const effective=edits.filter(edit=>!intent.keepDefinition||edit.text!=='');
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
  const repaired=repairCompilerMove(env,service,program,selected[0],edits,makeService,rawEdits.length);
  for(const p of Object.keys(resultHashes))delete resultHashes[p];
  for(const edit of repaired.edits)resultHashes[edit.path]=sha(source.get(abs(edit.path)));
  process.stderr.write(JSON.stringify({probeOnly:true,rawEdits,outputs:[...source]}));const afterService=makeService();let after,afterDiagnostics;
  try{const program=afterService.getProgram();afterDiagnostics=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));assert(afterDiagnostics.length<=20_000&&!afterDiagnostics.some(d=>d.severity==='error'),'Move compiler context has errors after refactoring');after=inspect(program);}finally{afterService.dispose();}
  const libraries=Object.fromEntries([...libCache].filter(([,value])=>value!==undefined).map(([p,value])=>[path.basename(p),sha(value)]).sort(([a],[b])=>a.localeCompare(b)));
  const options=JSON.parse(JSON.stringify(context.parsed.options,(key,value)=>key==='configFile'?undefined:typeof value==='string'?value.replaceAll(root,'[snapshot]'):value));
  const contextId=sha(JSON.stringify([input.snapshotId,context.name,context.parsed.fileNames.map(rel).sort(),options,ts.version,compilerHash,libraries]));
  return{schema:1,operation:'move',snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,sourceBindings:input.sourceBindings,sourceExecuted:false,sourceTreeWritten:false,exhaustive:false,status:'passed',contexts:[],diagnostics:[],outputs:[],standardLibraryHashes:libraries,limitations:[...limitations,'Known static compiler bindings are checked; dynamic/omitted consumers and module evaluation order remain unverified.'],move:{schema:1,normalization:'ordered_insertions_and_original_bom_v1',config:context.name,contextId,actionKind:action.kind,before,after,rawEdits,edits:repaired.edits,resultHashes,beforeDiagnostics,afterDiagnostics,...(repaired.repairs.length?{repairs:repaired.repairs,composition:"original_byte_pieces_v1"}:{})}};
 }finally{source.clear();for(const [p,value]of original)source.set(p,value);service.dispose();}
}

'use strict';
// Source-only repairs. Rust reconstructs their stages and original-byte edits.
function repairCompilerMove(env,service,program,selected,compilerEdits,makeService,compilerOperations){
 const {ts,assert,source,abs,rel,slash,span}=env,file=abs(env.input.intent.path),destination=abs(env.input.intent.destination);
 const original=new Map(Object.entries(env.input.contents).map(([p,t])=>[abs(p),t])),tables=new Map(),repairs=[];
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
     const changes=[{...span(sf,name.getStart(sf),name.end),text:importText},...references.map(n=>({...span(sf,n.getStart(sf),n.end),text:ts.isShorthandPropertyAssignment(n.parent)?n.getText(sf)+': '+newName:newName}))];
     next={repair:{kind:'importAlias',importName:span(sf,name.getStart(sf),name.end),originalReferences:indices.map(i=>span(oldIdentifiers[i].getSourceFile(),oldIdentifiers[i].getStart(),oldIdentifiers[i].end)),currentReferences:references.map(n=>span(sf,n.getStart(sf),n.end)),newName},changes};break;
    }if(next)break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move alias repair capacity');
 }
 // The compiler can omit imports used only by object shorthand. Recover only
 // original captured import bindings; never infer a module from a bare name.
 const origins=[];for(const node of selected.getSourceFile().statements)if(ts.isImportDeclaration(node)&&!node.attributes)for(const [name]of importNames(node))origins.push({node,name,symbol:checker.getSymbolAtLocation(name)});
 for(let round=0;round<256;round++){
  const currentService=makeService();let next;
  try{
   const currentProgram=currentService.getProgram(),c=currentProgram.getTypeChecker(),sf=currentProgram.getSourceFile(destination),declarations=sf.statements.filter(n=>n.kind===selected.kind&&nameOf(n)?.text===env.input.intent.name);
   assert(declarations.length===1,'Move missing-import declaration unavailable');const moved=declarations[0],currentIdentifiers=identifiers(moved);assert(currentIdentifiers.length===oldIdentifiers.length,'Move missing-import identifier structure changed');
   for(const origin of origins){
    const indices=[...free].filter(([i])=>symbolAt(checker,oldIdentifiers[i])===origin.symbol).map(([i])=>i);if(!indices.length)continue;
    if(indices.every(i=>symbolKey(c,symbolAt(c,currentIdentifiers[i]))===free.get(i)))continue;
    const module=checker.getSymbolAtLocation(origin.node.moduleSpecifier),moduleFile=module?.declarations?.find(ts.isSourceFile)?.fileName;assert(moduleFile&&original.has(slash(path.resolve(moduleFile))),'Move missing-import module must be captured');
    const modulePath=slash(path.resolve(moduleFile)),relative=slash(path.relative(path.dirname(destination),modulePath)),base=relative.startsWith('../')?relative:'./'+relative,stem=base.replace(/(?:\.d)?\.[cm]?[jt]sx?$/,'');
    const candidates=[stem,stem+'.js',stem+'.mjs',stem+'.cjs',base];
    const host={fileExists:env.fileExists,readFile:env.read,directoryExists:env.directoryExists,getDirectories:env.getDirectories,realpath:p=>slash(path.resolve(p))};
    const moduleName=candidates.find(candidate=>{const resolved=ts.resolveModuleName(candidate,destination,currentProgram.getCompilerOptions(),host).resolvedModule;return resolved&&slash(path.resolve(resolved.resolvedFileName))===modulePath;});assert(moduleName!==undefined,'Move missing-import destination module specifier unavailable');const moduleText=JSON.stringify(moduleName);
    while(used.has('__ci_move_'+serial)){serial++;assert(serial<=100_000,'Move fresh alias capacity');}const newName='__ci_move_'+serial++;used.add(newName);
    const parent=origin.name.parent,originalName=span(origin.name.getSourceFile(),origin.name.getStart(),origin.name.end);let binding;
    if(ts.isImportSpecifier(parent))binding='{'+(parent.propertyName?parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName:parent.getText()+' as '+newName)+'}';
    else if(ts.isNamespaceImport(parent))binding=parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName;
    else {assert(ts.isImportClause(parent),'Move missing-import original binding shape');binding=newName;}
    const line=sf.text.includes('\r\n')?'\r\n':'\n',text=line+'import '+(origin.node.importClause.isTypeOnly?'type ':'')+binding+' from '+moduleText+';'+line,insertion=span(sf,moved.end,moved.end),references=indices.map(i=>currentIdentifiers[i]);
    const changes=[{...insertion,text},...references.map(n=>({...span(sf,n.getStart(sf),n.end),text:ts.isShorthandPropertyAssignment(n.parent)?n.getText(sf)+': '+newName:newName}))];
    next={repair:{kind:'missingImport',originalImport:span(origin.node.getSourceFile(),origin.node.getStart(),origin.node.end),originalName,insertion,originalReferences:indices.map(i=>span(oldIdentifiers[i].getSourceFile(),oldIdentifiers[i].getStart(),oldIdentifiers[i].end)),currentReferences:references.map(n=>span(sf,n.getStart(sf),n.end)),newName,moduleText},changes};break;
   }
  }finally{currentService.dispose();}
  if(!next)break;applyRepair(next.repair,next.changes);assert(round<255,'Move missing-import repair capacity');
 }
 if(!repairs.length)return{repairs,edits:compilerEdits};const edits=[];
 for(const p of [...tables.keys()].sort((a,b)=>Buffer.compare(Buffer.from(a),Buffer.from(b)))){let cursor=0,pending='';const emit=end=>{if(cursor!==end||pending){if(raw(p).subarray(cursor,end).toString('utf8')!==pending)edits.push({path:p,startByte:cursor,endByte:end,text:pending});}pending='';};for(const part of table(p)){if(part.text!==undefined){pending+=part.text;continue;}assert(part.start>=cursor,'Move original pieces reordered');emit(part.start);cursor=part.end;}emit(raw(p).length);}
 assert(edits.length<=2000,'Move composed operation capacity');return{repairs,edits};
}
