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
 assert(input.schema===1 && ['analyze','build','inline','move','inlineBody'].includes(input.operation),'Compiler request contract');
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
 if(input.operation==='inlineBody')return inlineFunctionBody({input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations});
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
// Independent body expansion; TypeScript supplies immutable binding observations.
// This is not the compiler's Inline variable action.
function inlineFunctionBody(env){
 const {input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations}=env;
 const intent=input.intent,file=abs(intent.path),text=source.get(file);assert(typeof text==='string'&&typeof intent.keepDefinition==='boolean','Body inline captured source missing');
 if(!projects.size){limitations.push('No configured project: inferred context is nonexhaustive.');projects.set('[inferred]',{name:'[inferred]',problems:[],parsed:{fileNames:[...source.keys()].filter(p=>/\.[cm]?[jt]sx?$/.test(p)),options:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,allowJs:true,checkJs:true,jsx:ts.JsxEmit.Preserve,noEmit:true},projectReferences:[]}});}
 const contexts=[...projects.values()].filter(p=>p.parsed.fileNames.some(p=>slash(path.resolve(p))===file));assert(contexts.length===1,'Body inline requires one captured owning context');const context=contexts[0];assert([...projects.values()].every(p=>!p.problems.some(d=>d.severity==='error')),'Body inline project configuration incomplete');
 const makeService=()=>ts.createLanguageService({getScriptFileNames:()=>context.parsed.fileNames,getScriptVersion:()=> '1',getScriptSnapshot:p=>{const t=read(p);return t===undefined?undefined:ts.ScriptSnapshot.fromString(t);},getCompilationSettings:()=>({...context.parsed.options,noEmit:true}),getProjectReferences:()=>context.parsed.projectReferences,useSourceOfProjectReferenceRedirect:()=>true,getCurrentDirectory:()=>root,getDefaultLibFileName:o=>ts.getDefaultLibFilePath(o),fileExists,readFile:read,directoryExists,getDirectories,readDirectory,realpath:p=>slash(path.resolve(p)),useCaseSensitiveFileNames:()=>true});
 let visits=0;const walk=(node,callback)=>{assert(++visits<=2_000_000,'Body inline AST capacity');callback(node);ts.forEachChild(node,n=>walk(n,callback));};
 const resolve=(c,s)=>s&&(s.flags&ts.SymbolFlags.Alias)?c.getAliasedSymbol(s):s;
 const describeTargets=(checker,s)=>{s=resolve(checker,s);const targets=[];for(const d of s?.declarations||[]){assert(targets.length<128,'Body merged declaration capacity');const sf=d.getSourceFile(),p=slash(path.resolve(sf.fileName));if(ts.isSourceFile(d)){assert(source.has(p),'Body module target outside capture');targets.push({kind:'module',path:rel(p)});continue;}const named=d.name&&typeof d.name.getStart==='function'?d.name:d,at=span(sf,named.getStart(sf),named.end);if(source.has(p))targets.push({kind:'captured',span:at});else{assert(libCache.has(p),'Body target outside captured/library source');targets.push({kind:'library',name:path.basename(p),hash:sha(sf.text),startByte:at.startByte,endByte:at.endByte,text:at.text});}}return{resolution:!s?'unbound':targets.length?'resolved':'no_declarations',targets};};
 const observe=program=>{
  const checker=program.getTypeChecker(),bindings=[],types=[];let typeBytes=0;

  for(const sf of program.getSourceFiles())if(source.has(slash(path.resolve(sf.fileName))))walk(sf,n=>{if(!ts.isIdentifier(n))return;const symbol=ts.isShorthandPropertyAssignment(n.parent)?checker.getShorthandAssignmentValueSymbol(n.parent):checker.getSymbolAtLocation(n);bindings.push({span:span(sf,n.getStart(sf),n.end),...describeTargets(checker,symbol)});assert(bindings.length<=100_000,'Body binding capacity');
   if((ts.isVariableDeclaration(n.parent)||ts.isBindingElement(n.parent)||ts.isFunctionDeclaration(n.parent)||ts.isParameter(n.parent))&&n.parent.name===n){const type=checker.typeToString(checker.getTypeAtLocation(n),n,ts.TypeFormatFlags.NoTruncation);typeBytes+=Buffer.byteLength(type);assert(type.length<=16_000&&typeBytes<=2_000_000&&types.length<10_000,'Body type observation capacity');types.push({span:span(sf,n.getStart(sf),n.end),type});}
  });return{bindings,types};
 };
 const service=makeService();
 try{
  const program=service.getProgram(),sf=program.getSourceFile(file),checker=program.getTypeChecker();assert(sf&&ts.isExternalModule(sf),'Body inline requires an ES module with strict lexical bindings');const beforeDiagnostics=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));assert(beforeDiagnostics.length<=20_000&&!beforeDiagnostics.some(d=>d.severity==='error'),'Body context has errors before expansion');
  let selected;walk(sf,n=>{if(ts.isFunctionDeclaration(n)&&n.name&&span(sf,n.name.getStart(sf),n.name.end).startByte===intent.nameStartByte)selected=n;});assert(selected&&selected.name.text===intent.name&&selected.body&&!selected.asteriskToken&&!selected.typeParameters?.length&&!selected.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword),'Body declaration form unavailable');
  assert(Buffer.byteLength(text.slice(0,selected.end))===intent.endByte,'Body declaration end differs from capture');const start=Buffer.from(text).subarray(0,intent.startByte).toString('utf8').length;assert(start>=selected.getStart(sf)&&start<=selected.name.getStart(sf),'Body declaration start differs from capture');
  assert(intent.keepDefinition||!selected.modifiers?.some(m=>m.kind===ts.SyntaxKind.ExportKeyword||m.kind===ts.SyntaxKind.DefaultKeyword),'Exported body definition must be retained');
  assert(selected.parameters.length<=64&&selected.parameters.every((p,i)=>(ts.isIdentifier(p.name)||ts.isObjectBindingPattern(p.name)||ts.isArrayBindingPattern(p.name))&&p.name.getText(sf)!=='this'&&(!p.dotDotDotToken||(i===selected.parameters.length-1&&!p.initializer))),'Body parameter form unavailable');
  const vars=bodyVarPlan(ts,selected,sf,text,assert,()=>assert(++visits<=2_000_000,'Body var AST capacity'));
  const symbol=checker.getSymbolAtLocation(selected.name);assert(symbol?.declarations?.length===1&&symbol.declarations[0]===selected,'Body declaration binding ambiguous');const references=[],calls=[],used=new Set();
  for(const unit of program.getSourceFiles())if(source.has(slash(path.resolve(unit.fileName))))walk(unit,n=>{if(ts.isIdentifier(n))used.add(n.text);if(ts.isCallExpression(n)){let target=n.expression,depth=0;while(ts.isOuterExpression(target)){assert(++depth<=2000,'Body call wrapper capacity');target=target.expression;}if(ts.isIdentifier(target)&&target.text==='eval')assert(false,'Body context has dynamic eval authority');}if(!ts.isIdentifier(n)||n===selected.name)return;const bound=ts.isShorthandPropertyAssignment(n.parent)?checker.getShorthandAssignmentValueSymbol(n.parent):checker.getSymbolAtLocation(n);if(resolve(checker,bound)!==symbol)return;assert(unit===sf,'Body consumers outside owning file unavailable');assert(n.getStart(sf)<selected.getStart(sf)||n.end>selected.end,'Recursive body references unavailable');assert(ts.isCallExpression(n.parent)&&n.parent.expression===n&&!n.parent.questionDotToken,'Body noncall/write/optional consumers unavailable');references.push(span(sf,n.getStart(sf),n.end));calls.push(n.parent);assert(calls.length<=128,'Body call capacity');});assert(calls.length,'Body inline has no bound calls');
  walk(selected,n=>{assert(n.kind!==ts.SyntaxKind.ThisKeyword&&n.kind!==ts.SyntaxKind.SuperKeyword&&!ts.isMetaProperty(n)&&!ts.isAwaitExpression(n)&&!ts.isYieldExpression(n),'Body invocation context needs additional preservation');if(ts.isIdentifier(n))assert(n.text!=='arguments'&&n.text!=='eval','Body dynamic invocation identifier requires original scope');if(ts.isPrivateIdentifier(n))assert(false,'Body nominal private class requires preservation');if(n.modifiers)assert(!n.modifiers.some(m=>m.kind===ts.SyntaxKind.PrivateKeyword||m.kind===ts.SyntaxKind.ProtectedKeyword),'Body nominal private class requires preservation');
   if(!ts.isIdentifier(n)||n===selected.name)return;const p=n.parent;if(ts.isConstTypeReference(p)&&p.typeName===n&&(ts.isAsExpression(p.parent)||ts.isTypeAssertionExpression(p.parent))&&p.parent.type===p)return;if((ts.isPropertyAccessExpression(p)&&p.name===n)||(ts.isQualifiedName(p)&&p.right===n)||((ts.isPropertyAssignment(p)||ts.isPropertySignature(p)||ts.isMethodDeclaration(p))&&p.name===n))return;if((ts.isJsxOpeningElement(p)||ts.isJsxClosingElement(p)||ts.isJsxSelfClosingElement(p))&&p.tagName===n&&(/^[a-z]/.test(n.text)||n.text.includes('-')))return;if(ts.isJsxAttribute(p)&&p.name===n)return;const s=ts.isShorthandPropertyAssignment(p)?checker.getShorthandAssignmentValueSymbol(p):checker.getSymbolAtLocation(n);if(s?.declarations?.some(d=>d.getSourceFile()===sf&&d.getStart(sf)>=selected.getStart(sf)&&d.end<=selected.end))return;for(const call of calls)assert(resolve(checker,checker.resolveName(n.text,call,ts.SymbolFlags.Value|ts.SymbolFlags.Type|ts.SymbolFlags.Namespace,false))===resolve(checker,s),'Body free binding would change at consumer');
  });
  const varAssignments=vars.ids.map(n=>({span:span(sf,n.getStart(sf),n.end),...describeTargets(checker,checker.resolveName(n.text,n,ts.SymbolFlags.Value,false))}));
  const before=observe(program),frames=[],edits=[];let serial=1,outputBytes=0;const fresh=()=>{while(used.has('__ci_inline_'+serial))serial++;assert(serial<=100_000,'Body fresh name capacity');const name='__ci_inline_'+serial++;used.add(name);return name;};const primitive=new Set(['any','unknown','number','string','boolean','bigint','symbol','undefined','void','never']);const variableTypes=vars.groups.map(g=>{const n=g.ids[0],type=checker.typeToString(checker.getTypeAtLocation(n),n,ts.TypeFormatFlags.NoTruncation);return{span:span(sf,n.getStart(sf),n.end),type};});assert(variableTypes.every(v=>v.type.length<=16_000)&&variableTypes.reduce((sum,v)=>sum+Buffer.byteLength(v.type),0)<=2_000_000,'Body var type witness capacity');const varHints=vars.groups.map((g,i)=>!g.annotation&&!g.function&&primitive.has(variableTypes[i].type)?variableTypes[i].type:null);const typeGroups=vars.groups.flatMap((g,i)=>!g.annotation&&!g.function&&!g.parameter&&!primitive.has(variableTypes[i].type)?[i]:[]);const typed=bodyVarTypePlan(ts,vars,typeGroups,sf,text,assert,()=>assert(++visits<=2_000_000,'Body type witness AST capacity'));const resultType=checker.typeToString(checker.getReturnTypeOfSignature(checker.getSignatureFromDeclaration(selected)),selected,ts.TypeFormatFlags.NoTruncation);
  for(const call of calls){
   let value=call;while(ts.isParenthesizedExpression(value.parent))value=value.parent;const parent=value.parent;let statement;
   if(ts.isVariableDeclaration(parent)&&parent.initializer===value){assert(ts.isVariableDeclarationList(parent.parent)&&parent.parent.declarations.length===1&&ts.isVariableStatement(parent.parent.parent),'Body initializer needs statement sequencing');statement=parent.parent.parent;}else{assert((ts.isReturnStatement(parent)&&parent.expression===value)||(ts.isExpressionStatement(parent)&&parent.expression===value),'Body expression context needs sequencing');statement=parent;}
   assert(call.arguments.length<=64&&!call.arguments.some(ts.isSpreadElement),'Body spread/argument capacity unavailable');const names={result:fresh(),exit:fresh(),arguments:call.arguments.map(()=>fresh())},line=text.includes('\r\n')?'\r\n':'\n',wrap=!ts.isVariableStatement(statement),resultHint=!selected.type&&primitive.has(resultType)?resultType:null;
   if(vars.copyGroups.length)names.varParameters=vars.copyGroups.map(()=>fresh());if(typed.count)names.varSeeds=Array.from({length:typed.count},()=>fresh());
   let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;
   const syntax=call.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.SyntaxList&&n.pos>=call.expression.end),commas=(syntax?.getChildren(sf)||[]).filter(n=>n.kind===ts.SyntaxKind.CommaToken);let cursor=call.arguments.pos;
   const trivia=end=>{let t='';for(const comma of commas)if(comma.getStart(sf)>=cursor&&comma.end<=end){t+=text.slice(cursor,comma.getStart(sf));cursor=comma.end;}t+=text.slice(cursor,end);cursor=end;return t;};
   const close=call.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.CloseParenToken);assert(close,'Body argument closing delimiter unavailable');for(let i=0;i<call.arguments.length;i++){const arg=call.arguments[i];replacement+='const '+names.arguments[i]+' = ('+trivia(arg.getStart(sf))+arg.getText(sf)+');'+line;cursor=arg.end;}replacement+=trivia(close.getStart(sf))+line+'{'+line;
   const parameterSyntax=selected.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.SyntaxList&&n.pos===selected.parameters.pos),parameterCommas=(parameterSyntax?.getChildren(sf)||[]).filter(n=>n.kind===ts.SyntaxKind.CommaToken);let parameterCursor=selected.parameters.pos;
   const parameterTrivia=end=>{const start=parameterCursor;let t='';for(const comma of parameterCommas)if(comma.getStart(sf)>=parameterCursor&&comma.end<=end){t+=text.slice(parameterCursor,comma.getStart(sf));parameterCursor=comma.end;}t+=text.slice(parameterCursor,end);parameterCursor=end;return t+(end>start?line:'');};
   for(let i=0;i<selected.parameters.length;i++){
    replacement+=parameterTrivia(selected.parameters[i].getStart(sf));parameterCursor=selected.parameters[i].end;
    const p=selected.parameters[i],argument=names.arguments[i]||'void 0';replacement+='let '+p.name.getText(sf);
    if(p.type){const colon=p.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken);assert(colon,'Body parameter colon unavailable');replacement+=p.questionToken?text.slice(p.name.end,p.questionToken.getStart(sf))+text.slice(p.questionToken.end,colon.getStart(sf))+': ('+text.slice(colon.end,p.type.end)+') | undefined':text.slice(p.name.end,p.type.end);}
    const equal=p.initializer?p.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.EqualsToken):undefined;assert(!p.initializer||equal,'Body default assignment token unavailable');const prefixEnd=equal?equal.getStart(sf):p.end,prefixStart=p.type?p.type.end:p.name.end;
    replacement+=!p.type&&p.questionToken?text.slice(prefixStart,p.questionToken.getStart(sf))+text.slice(p.questionToken.end,prefixEnd):text.slice(prefixStart,prefixEnd);replacement+=' = ';
    if(p.dotDotDotToken)replacement+='['+names.arguments.slice(i).join(',')+']';
    else if(p.initializer){replacement+=i<names.arguments.length?'('+argument+') === void 0 ? (':'(';if(ts.isIdentifier(p.name)){const name=JSON.stringify(p.name.text);replacement+='{['+name+']: ('+(p.type?'(':'')+text.slice(equal.end,p.initializer.end);if(p.type){const colon=p.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken);replacement+=') as ('+text.slice(colon.end,p.type.end)+')';}replacement+=')})['+name+']';}else replacement+='('+text.slice(equal.end,p.initializer.end)+'))';if(i<names.arguments.length)replacement+=' : ('+argument+')';}
    else replacement+=argument;replacement+=';'+line;
   }
   const parameterClose=selected.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.CloseParenToken);assert(parameterClose,'Body parameter closing delimiter unavailable');replacement+=parameterTrivia(parameterClose.getStart(sf))+vars.captures(names,line);
   const changes=[...vars.render(line),...typed.changes(names,line)];const returns=n=>{assert(++visits<=2_000_000,'Body return syntax capacity');if(n!==selected.body&&ts.isFunctionLike(n))return;if(ts.isReturnStatement(n)){const at=n.getStart(sf),value=n.expression?'('+text.slice(at+6,n.expression.end)+')':'void 0',tail=text.slice(n.expression?n.expression.end:at+6,n.end);changes.push({start:at,end:n.end,text:'{ '+names.result+' = '+value+tail+'; break '+names.exit+'; }'});return;}ts.forEachChild(n,returns);};returns(selected.body);assert(changes.length<=2000,'Body var/return capacity');changes.sort((a,b)=>a.start-b.start||a.end-b.end);assert(changes.every((e,i)=>!i||changes[i-1].end<=e.start),'Body var/return overlap');let body=selected.body.getText(sf);for(const e of [...changes].reverse())body=body.slice(0,e.start-selected.body.getStart(sf))+e.text+body.slice(e.end-selected.body.getStart(sf));if(vars.groups.length)body=body.slice(0,1)+line+vars.hoists(names,varHints,/\.[cm]?jsx?$/.test(file),line,typed)+body.slice(1);body=body.slice(0,-1)+line+names.result+' = void 0;'+line+'}';replacement+=names.exit+': '+body+line+'}'+line+'}'+line;
   let result=names.result;if(selected.type)result='('+result+' as ('+text.slice(selected.type.pos,selected.type.end)+'))';else if(resultHint)result=/\.[cm]?jsx?$/.test(file)?'/** @type {'+resultHint+'} */ ('+result+')':'('+result+' as ('+resultHint+'))';replacement+=text.slice(statement.getStart(sf),call.getStart(sf))+result+text.slice(call.end,statement.end)+(wrap?line+'}':'');
   const st=span(sf,statement.getStart(sf),statement.end),at=span(sf,call.getStart(sf),call.end);frames.push({statementStart:st.startByte,statementEnd:st.endByte,callStart:at.startByte,callEnd:at.endByte,names,resultType:resultHint,...(varHints.some(Boolean)?{varTypes:varHints}:{}),...(typeGroups.length?{varSeedGroups:typeGroups}:{})});edits.push({path:intent.path,startByte:st.startByte,endByte:st.endByte,text:replacement});outputBytes+=Buffer.byteLength(replacement);assert(outputBytes<=4_000_000,'Body expanded text capacity');
  }
  if(!intent.keepDefinition)edits.push({path:intent.path,startByte:intent.startByte,endByte:intent.endByte,text:''});edits.sort((a,b)=>a.startByte-b.startByte);assert(edits.every((e,i)=>i===0||edits[i-1].endByte<=e.startByte),'Body call statements overlap');let result=Buffer.from(text);for(const e of [...edits].reverse())result=Buffer.concat([result.subarray(0,e.startByte),Buffer.from(e.text),result.subarray(e.endByte)]);assert(result.length<=4_000_000&&Buffer.from(result.toString('utf8')).equals(result),'Body result capacity/UTF-8');source.set(file,result.toString('utf8'));let after,afterDiagnostics;const afterService=makeService();try{const p=afterService.getProgram();afterDiagnostics=ts.getPreEmitDiagnostics(p).map(d=>diagnostic(d,context.name));assert(afterDiagnostics.length<=20_000&&!afterDiagnostics.some(d=>d.severity==='error'),'Body context has errors after expansion: '+JSON.stringify({afterDiagnostics,projected:source.get(file)}));after=observe(p);}finally{afterService.dispose();source.set(file,text);}
  const libraries=Object.fromEntries([...libCache].filter(([,v])=>v!==undefined).map(([p,v])=>[path.basename(p),sha(v)]).sort(([a],[b])=>a.localeCompare(b))),options=JSON.parse(JSON.stringify(context.parsed.options,(k,v)=>k==='configFile'?undefined:typeof v==='string'?v.replaceAll(root,'[snapshot]'):v)),contextId=sha(JSON.stringify([input.snapshotId,context.name,options,context.parsed.fileNames.map(rel).sort(),ts.version,compilerHash,libraries]));
  return{schema:1,operation:'inlineBody',snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,sourceBindings:input.sourceBindings,sourceExecuted:false,sourceTreeWritten:false,exhaustive:false,status:'passed',contexts:[],diagnostics:[],outputs:[],standardLibraryHashes:libraries,limitations:[...limitations,'Body frames are finite captured strict-module expansions; dynamic consumers, reflective call stacks and general runtime equivalence remain unverified.'],bodyInline:{schema:1,path:intent.path,keepDefinition:intent.keepDefinition,config:context.name,contextId,actionKind:'native.inline.function.body',frames,references,...(vars.ids.length?{varAssignments,varTypes:variableTypes}:{}),before,after,beforeDiagnostics,afterDiagnostics,edits,resultHash:sha(result)}};
 }finally{service.dispose();source.set(file,text);}
}

'use strict';
// Source-only var lowering. Compiler declaration and initialization bindings stay separate.
function bodyVarPlan(ts,selected,sf,text,assert,tick){
 const ids=[],changes=[],parameters=new Map(),functions=new Map();
 const bound=name=>{if(ts.isIdentifier(name))return[name];const result=[];for(const e of name.elements){if(ts.isOmittedExpression(e))continue;result.push(...bound(e.name));}return result;};
 for(const p of selected.parameters)for(const id of bound(p.name))parameters.set(id.text,id);
 for(const f of selected.body.statements)if(ts.isFunctionDeclaration(f)&&f.name)functions.set(f.name.text,f.name);
 const collect=n=>{tick();if(n!==selected.body&&(ts.isFunctionLike(n)||ts.isClassDeclaration(n)||ts.isClassExpression(n)))return;
  if(ts.isVariableDeclarationList(n)&&!(n.flags&ts.NodeFlags.BlockScoped)){
   for(const d of n.declarations)for(const id of bound(d.name))ids.push({id,annotation:ts.isIdentifier(d.name)?d.type:undefined});
   if(ts.isForInStatement(n.parent)||ts.isForOfStatement(n.parent)){assert(n.declarations.length===1&&!n.declarations[0].initializer,'Body var loop initializer unsupported');changes.push({start:n.getStart(sf),end:n.getStart(sf)+3,text:''});}
   else{const statement=ts.isVariableStatement(n.parent)?n.parent:n;changes.push({start:statement.getStart(sf),end:statement.end,declaration:n});}
  }
  ts.forEachChild(n,collect);
 };collect(selected.body);ids.sort((a,b)=>a.id.getStart(sf)-b.id.getStart(sf));assert(ids.length<=2000&&changes.length<=2000,'Body var declaration capacity');
 const groups=[],byName=new Map();for(const {id,annotation}of ids){let g=byName.get(id.text);if(!g){g={name:id.text,ids:[],parameter:parameters.get(id.text),function:functions.get(id.text),annotation};byName.set(id.text,g);groups.push(g);}g.ids.push(id);}assert(groups.length<=512,'Body var name capacity');
 const copyGroups=groups.filter(g=>g.parameter&&!g.function);
 function captures(names,line){assert((names.varParameters||[]).length===copyGroups.length,'Body var parameter copy mismatch');return copyGroups.map((g,i)=>'const '+names.varParameters[i]+' = ('+g.parameter.getText(sf)+');'+line).join('');}
 function hoists(names,hints,js,line,typed){let index=0,out='';for(let i=0;i<groups.length;i++){const g=groups[i],hint=hints[i];if(g.function)continue;const inferred=typed.nameFor(i,names);if(js&&(hint||inferred))out+='/** @type {'+(inferred?'typeof '+inferred:hint)+'} */ ';out+='let '+g.ids[0].getText(sf);if(g.annotation){const owner=g.ids[0].parent,colon=owner.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken);assert(colon,'Body var type colon absent');out+=text.slice(colon.getStart(sf),g.annotation.end);}else if(!js&&(hint||inferred))out+=': '+(inferred?'typeof '+inferred:hint);if(g.parameter)out+=' = '+names.varParameters[index++];else if(g.annotation||hint||inferred)out+=js?' = /** @type {any} */ (void 0)':' = void 0 as any';out+=';'+line;}return out;}

 function render(line){return changes.map(e=>{if(!e.declaration)return e;const list=e.declaration;let out='(',cursor=list.getStart(sf)+3;for(const d of list.declarations){out+=text.slice(cursor,d.name.getStart(sf));const colon=d.type?d.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken):undefined;assert(!d.type||colon,'Body var type colon absent');
   if(d.initializer){const equal=d.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.EqualsToken);assert(equal,'Body var assignment token absent');out+=d.name.getText(sf)+text.slice(d.name.end,colon?colon.getStart(sf):equal.getStart(sf));if(d.type)out+=text.slice(d.type.end,equal.getStart(sf));out+='= ('+(d.type?'(':'')+text.slice(equal.end,d.initializer.end);if(d.type)out+=') as ('+text.slice(colon.end,d.type.end)+')';out+=')';}
   else if(d.type)out+='void '+text.slice(d.name.end,colon.getStart(sf))+'(0 as unknown as ('+text.slice(colon.end,d.type.end)+'))'+text.slice(d.type.end,d.end);
   else out+='void 0'+text.slice(d.name.end,d.end);
   cursor=d.end;
  }out+=')'+text.slice(cursor,e.end);return{start:e.start,end:e.end,text:out};});}
 return{ids:ids.map(v=>v.id),groups,copyGroups,captures,hoists,render};
}

'use strict';
// Type inference uses original declarations in an unreachable witness block.
// Only fixed typeof syntax and fresh names are generated; initializers stay inert.
function bodyVarTypePlan(ts,vars,indices,sf,text,assert,tick){
 const sites=[],byDeclaration=new Map(),groupSites=new Map();
 const declarationOf=id=>{let at=id;for(let depth=0;at&&depth<2000;depth++,at=at.parent){tick();if(ts.isVariableDeclaration(at))return at;}assert(false,'Body var type declaration unavailable');};
 for(const i of indices){
  const g=vars.groups[i];assert(g&&!g.annotation&&!g.parameter&&!g.function,'Body var type group unsupported');
  const declaration=g.ids.map(declarationOf).find(d=>d.initializer||d.type||ts.isForInStatement(d.parent.parent)||ts.isForOfStatement(d.parent.parent));
  assert(declaration,'Body var source type witness unavailable');
  let site=byDeclaration.get(declaration);
  if(!site){const list=declaration.parent,parent=list.parent;assert(ts.isVariableDeclarationList(list),'Body var type list unavailable');
   const loop=(ts.isForInStatement(parent)||ts.isForOfStatement(parent))?parent:undefined;
   let placement=ts.isVariableStatement(parent)||ts.isForStatement(parent)||loop?parent:undefined;
   assert(placement,'Body var type statement unavailable');
   let labels=0;while(ts.isLabeledStatement(placement.parent)&&placement.parent.statement===placement){tick();assert(++labels<2000,'Body var type label capacity');placement=placement.parent;}
   site={declaration,placement,loop};byDeclaration.set(declaration,site);sites.push(site);
  }
  groupSites.set(i,site);
 }
 sites.sort((a,b)=>a.declaration.getStart(sf)-b.declaration.getStart(sf));
 let count=0;const bound=name=>{tick();if(ts.isIdentifier(name))return[name];const result=[];for(const e of name.elements){if(ts.isOmittedExpression(e))continue;result.push(...bound(e.name));}return result;};
 for(const site of sites){site.ids=bound(site.declaration.name);site.bindings=new Map();for(const id of site.ids)if(!site.bindings.has(id.text))site.bindings.set(id.text,count++);}
 assert(count<=2000&&sites.length<=512,'Body var type witness capacity');
 const seedForGroup=new Map([...groupSites].map(([i,s])=>[i,s.bindings.get(vars.groups[i].name)]));
 function nameFor(i,names){const at=seedForGroup.get(i);return at===undefined?undefined:names.varSeeds[at];}
 function changes(names,line){assert((names.varSeeds||[]).length===count,'Body var type witness name count');const placements=new Map();
  for(const site of sites){const d=site.declaration;let pattern='',cursor=d.name.getStart(sf);
   for(const id of site.ids){const owner=id.parent,shorthand=ts.isBindingElement(owner)&&owner.name===id&&!owner.dotDotDotToken&&!owner.propertyName&&ts.isObjectBindingPattern(owner.parent);pattern+=text.slice(cursor,id.getStart(sf))+(shorthand?JSON.stringify(id.text)+': ':'')+names.varSeeds[site.bindings.get(id.text)];cursor=id.end;}
   pattern+=text.slice(cursor,d.name.end);
   const reads=[...site.bindings.values()].map(i=>'void '+names.varSeeds[i]+';').join('');let witness;
   if(site.loop)witness='for (var '+pattern+(ts.isForOfStatement(site.loop)?' of (':' in (')+site.loop.expression.getText(sf)+')) {'+reads+'}';
   else witness='var '+pattern+text.slice(d.name.end,d.initializer?d.initializer.end:d.end)+(d.initializer?'':' = void 0 as any')+';'+reads;
   const existing=placements.get(site.placement)||[];existing.push(witness);placements.set(site.placement,existing);
  }
  const result=[];for(const [placement,witnesses]of [...placements].sort(([a],[b])=>a.getStart(sf)-b.getStart(sf))){result.push({start:placement.getStart(sf),end:placement.getStart(sf),text:'{'+line+'if (void 0 !== void 0) {'+line+witnesses.join(line)+line+'}'+line});result.push({start:placement.end,end:placement.end,text:line+'}'});}
  return result;
 }
 return{count,nameFor,changes};
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
  const afterService=makeService();let after,afterDiagnostics;
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
    const moduleText=moduleTextFor(slash(path.resolve(moduleFile)),currentProgram);
    while(used.has('__ci_move_'+serial)){serial++;assert(serial<=100_000,'Move fresh alias capacity');}const newName='__ci_move_'+serial++;used.add(newName);
    const parent=origin.name.parent,originalName=span(origin.name.getSourceFile(),origin.name.getStart(),origin.name.end);let binding;
    if(ts.isImportSpecifier(parent))binding='{'+(parent.propertyName?parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName:parent.getText()+' as '+newName)+'}';
    else if(ts.isNamespaceImport(parent))binding=parent.getText().slice(0,origin.name.getStart()-parent.getStart())+newName;
    else {assert(ts.isImportClause(parent),'Move missing-import original binding shape');binding=newName;}
    const line=sf.text.includes('\r\n')?'\r\n':'\n',text=line+'import '+(origin.node.importClause.isTypeOnly?'type ':'')+binding+' from '+moduleText+';'+line,insertion=span(sf,moved.end,moved.end),references=indices.map(i=>currentIdentifiers[i]);
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
 if(!repairs.length)return{repairs,edits:compilerEdits};const edits=[];
 for(const p of [...tables.keys()].sort((a,b)=>Buffer.compare(Buffer.from(a),Buffer.from(b)))){let cursor=0,pending='';const emit=end=>{if(cursor!==end||pending){if(raw(p).subarray(cursor,end).toString('utf8')!==pending)edits.push({path:p,startByte:cursor,endByte:end,text:pending});}pending='';};for(const part of table(p)){if(part.text!==undefined){pending+=part.text;continue;}assert(part.start>=cursor,'Move original pieces reordered');emit(part.start);cursor=part.end;}emit(raw(p).length);}
 assert(edits.length<=2000,'Move composed operation capacity');return{repairs,edits};
}
