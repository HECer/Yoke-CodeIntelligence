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
 if(input.operation==='inlineBody')return inlineFunctionBody({input,ts,sha,assert,source,projects,read,fileExists,directoryExists,getDirectories,readDirectory,root,abs,rel,slash,span,diagnostic,libCache,compilerHash,limitations});
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
 const observe=program=>{
  const checker=program.getTypeChecker(),bindings=[],types=[];let typeBytes=0;
  const target=s=>{s=resolve(checker,s);const targets=[];for(const d of s?.declarations||[]){const sf=d.getSourceFile(),p=slash(path.resolve(sf.fileName)),named=d.name&&typeof d.name.getStart==='function'?d.name:d,at=span(sf,named.getStart(sf),named.end);if(ts.isSourceFile(d)){assert(source.has(p),'Body module target outside capture');targets.push({kind:'module',path:rel(p)});}else if(source.has(p))targets.push({kind:'captured',span:at});else{assert(libCache.has(p),'Body target outside captured/library source');targets.push({kind:'library',name:path.basename(p),hash:sha(sf.text),startByte:at.startByte,endByte:at.endByte,text:at.text});}}assert(targets.length<=128,'Body merged declaration capacity');return{resolution:!s?'unbound':targets.length?'resolved':'no_declarations',targets};};
  for(const sf of program.getSourceFiles())if(source.has(slash(path.resolve(sf.fileName))))walk(sf,n=>{if(!ts.isIdentifier(n))return;const symbol=ts.isShorthandPropertyAssignment(n.parent)?checker.getShorthandAssignmentValueSymbol(n.parent):checker.getSymbolAtLocation(n);bindings.push({span:span(sf,n.getStart(sf),n.end),...target(symbol)});assert(bindings.length<=100_000,'Body binding capacity');
   if((ts.isVariableDeclaration(n.parent)||ts.isFunctionDeclaration(n.parent)||ts.isParameter(n.parent))&&n.parent.name===n){const type=checker.typeToString(checker.getTypeAtLocation(n),n,ts.TypeFormatFlags.NoTruncation);typeBytes+=Buffer.byteLength(type);assert(type.length<=16_000&&typeBytes<=2_000_000&&types.length<10_000,'Body type observation capacity');types.push({span:span(sf,n.getStart(sf),n.end),type});}
  });return{bindings,types};
 };
 const service=makeService();
 try{
  const program=service.getProgram(),sf=program.getSourceFile(file),checker=program.getTypeChecker();assert(sf&&ts.isExternalModule(sf),'Body inline requires an ES module with strict lexical bindings');const beforeDiagnostics=ts.getPreEmitDiagnostics(program).map(d=>diagnostic(d,context.name));assert(beforeDiagnostics.length<=20_000&&!beforeDiagnostics.some(d=>d.severity==='error'),'Body context has errors before expansion');
  let selected;walk(sf,n=>{if(ts.isFunctionDeclaration(n)&&n.name&&span(sf,n.name.getStart(sf),n.name.end).startByte===intent.nameStartByte)selected=n;});assert(selected&&selected.name.text===intent.name&&selected.body&&!selected.asteriskToken&&!selected.typeParameters?.length&&!selected.modifiers?.some(m=>m.kind===ts.SyntaxKind.AsyncKeyword),'Body declaration form unavailable');
  assert(Buffer.byteLength(text.slice(0,selected.end))===intent.endByte,'Body declaration end differs from capture');const start=Buffer.from(text).subarray(0,intent.startByte).toString('utf8').length;assert(start>=selected.getStart(sf)&&start<=selected.name.getStart(sf),'Body declaration start differs from capture');
  assert(intent.keepDefinition||!selected.modifiers?.some(m=>m.kind===ts.SyntaxKind.ExportKeyword||m.kind===ts.SyntaxKind.DefaultKeyword),'Exported body definition must be retained');
  assert(selected.parameters.length<=64&&selected.parameters.every(p=>ts.isIdentifier(p.name)&&!p.initializer&&!p.dotDotDotToken&&!p.questionToken),'Body parameter form unavailable');
  const symbol=checker.getSymbolAtLocation(selected.name);assert(symbol?.declarations?.length===1&&symbol.declarations[0]===selected,'Body declaration binding ambiguous');const references=[],calls=[],used=new Set();
  for(const unit of program.getSourceFiles())if(source.has(slash(path.resolve(unit.fileName))))walk(unit,n=>{if(ts.isIdentifier(n))used.add(n.text);if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='eval')assert(false,'Body context has dynamic eval authority');if(!ts.isIdentifier(n)||n===selected.name)return;const bound=ts.isShorthandPropertyAssignment(n.parent)?checker.getShorthandAssignmentValueSymbol(n.parent):checker.getSymbolAtLocation(n);if(resolve(checker,bound)!==symbol)return;assert(unit===sf,'Body consumers outside owning file unavailable');assert(n.getStart(sf)<selected.getStart(sf)||n.end>selected.end,'Recursive body references unavailable');assert(ts.isCallExpression(n.parent)&&n.parent.expression===n&&!n.parent.questionDotToken,'Body noncall/write/optional consumers unavailable');references.push(span(sf,n.getStart(sf),n.end));calls.push(n.parent);assert(calls.length<=128,'Body call capacity');});assert(calls.length,'Body inline has no bound calls');
  walk(selected,n=>{assert(n.kind!==ts.SyntaxKind.ThisKeyword&&n.kind!==ts.SyntaxKind.SuperKeyword&&!ts.isMetaProperty(n)&&!ts.isAwaitExpression(n)&&!ts.isYieldExpression(n),'Body invocation context needs additional preservation');if(ts.isVariableDeclarationList(n))assert(Boolean(n.flags&ts.NodeFlags.BlockScoped),'Body var hoisting requires additional preservation');if(ts.isIdentifier(n))assert(n.text!=='arguments','Body arguments object requires preservation');if(ts.isPrivateIdentifier(n))assert(false,'Body nominal private class requires preservation');if(n.modifiers)assert(!n.modifiers.some(m=>m.kind===ts.SyntaxKind.PrivateKeyword||m.kind===ts.SyntaxKind.ProtectedKeyword),'Body nominal private class requires preservation');
   if(!ts.isIdentifier(n)||n===selected.name)return;const p=n.parent;if((ts.isPropertyAccessExpression(p)&&p.name===n)||(ts.isQualifiedName(p)&&p.right===n)||((ts.isPropertyAssignment(p)||ts.isPropertySignature(p)||ts.isMethodDeclaration(p))&&p.name===n))return;if((ts.isJsxOpeningElement(p)||ts.isJsxClosingElement(p)||ts.isJsxSelfClosingElement(p))&&p.tagName===n&&(/^[a-z]/.test(n.text)||n.text.includes('-')))return;if(ts.isJsxAttribute(p)&&p.name===n)return;const s=ts.isShorthandPropertyAssignment(p)?checker.getShorthandAssignmentValueSymbol(p):checker.getSymbolAtLocation(n);if(s?.declarations?.some(d=>d.getSourceFile()===sf&&d.getStart(sf)>=selected.getStart(sf)&&d.end<=selected.end))return;for(const call of calls)assert(resolve(checker,checker.resolveName(n.text,call,ts.SymbolFlags.Value|ts.SymbolFlags.Type|ts.SymbolFlags.Namespace,false))===resolve(checker,s),'Body free binding would change at consumer');
  });
  const before=observe(program),frames=[],edits=[];let serial=1,outputBytes=0;const fresh=()=>{while(used.has('__ci_inline_'+serial))serial++;assert(serial<=100_000,'Body fresh name capacity');const name='__ci_inline_'+serial++;used.add(name);return name;};const primitive=new Set(['any','unknown','number','string','boolean','bigint','symbol','undefined','void','never']);const resultType=checker.typeToString(checker.getReturnTypeOfSignature(checker.getSignatureFromDeclaration(selected)),selected,ts.TypeFormatFlags.NoTruncation);
  for(const call of calls){
   let value=call;while(ts.isParenthesizedExpression(value.parent))value=value.parent;const parent=value.parent;let statement;
   if(ts.isVariableDeclaration(parent)&&parent.initializer===value){assert(ts.isVariableDeclarationList(parent.parent)&&parent.parent.declarations.length===1&&ts.isVariableStatement(parent.parent.parent),'Body initializer needs statement sequencing');statement=parent.parent.parent;}else{assert((ts.isReturnStatement(parent)&&parent.expression===value)||(ts.isExpressionStatement(parent)&&parent.expression===value),'Body expression context needs sequencing');statement=parent;}
   assert(call.arguments.length<=64&&!call.arguments.some(ts.isSpreadElement),'Body spread/argument capacity unavailable');const names={result:fresh(),exit:fresh(),arguments:call.arguments.map(()=>fresh())},line=text.includes('\r\n')?'\r\n':'\n',wrap=!ts.isVariableStatement(statement),resultHint=!selected.type&&primitive.has(resultType)?resultType:null;
   let replacement=(wrap?'{'+line:'')+'let '+names.result+';'+line+'{'+line;
   const syntax=call.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.SyntaxList&&n.pos>=call.expression.end),commas=(syntax?.getChildren(sf)||[]).filter(n=>n.kind===ts.SyntaxKind.CommaToken);let cursor=call.arguments.pos;
   const trivia=end=>{let t='';for(const comma of commas)if(comma.getStart(sf)>=cursor&&comma.end<=end){t+=text.slice(cursor,comma.getStart(sf));cursor=comma.end;}t+=text.slice(cursor,end);cursor=end;return t;};
   for(let i=0;i<call.arguments.length;i++){const arg=call.arguments[i];replacement+='const '+names.arguments[i]+' = ('+trivia(arg.getStart(sf))+arg.getText(sf)+');'+line;cursor=arg.end;}replacement+=trivia(call.arguments.end)+line+'{'+line;
   for(let i=0;i<selected.parameters.length;i++)replacement+='let '+selected.parameters[i].getText(sf)+' = '+(names.arguments[i]||'void 0')+';'+line;
   const changes=[];const returns=n=>{assert(++visits<=2_000_000,'Body return syntax capacity');if(n!==selected.body&&ts.isFunctionLike(n))return;if(ts.isReturnStatement(n)){const at=n.getStart(sf),value=n.expression?'('+text.slice(at+6,n.expression.end)+')':'void 0',tail=text.slice(n.expression?n.expression.end:at+6,n.end);changes.push({start:at,end:n.end,text:'{ '+names.result+' = '+value+tail+'; break '+names.exit+'; }'});return;}ts.forEachChild(n,returns);};returns(selected.body);let body=selected.body.getText(sf);for(const e of changes.sort((a,b)=>b.start-a.start))body=body.slice(0,e.start-selected.body.getStart(sf))+e.text+body.slice(e.end-selected.body.getStart(sf));replacement+=names.exit+': '+body+line+'}'+line+'}'+line;
   let result=names.result;if(selected.type)result='('+result+' as ('+text.slice(selected.type.pos,selected.type.end)+'))';else if(resultHint)result=/\.[cm]?jsx?$/.test(file)?'/** @type {'+resultHint+'} */ ('+result+')':'('+result+' as ('+resultHint+'))';replacement+=text.slice(statement.getStart(sf),call.getStart(sf))+result+text.slice(call.end,statement.end)+(wrap?line+'}':'');
   const st=span(sf,statement.getStart(sf),statement.end),at=span(sf,call.getStart(sf),call.end);frames.push({statementStart:st.startByte,statementEnd:st.endByte,callStart:at.startByte,callEnd:at.endByte,names,resultType:resultHint});edits.push({path:intent.path,startByte:st.startByte,endByte:st.endByte,text:replacement});outputBytes+=Buffer.byteLength(replacement);assert(outputBytes<=4_000_000,'Body expanded text capacity');
  }
  if(!intent.keepDefinition)edits.push({path:intent.path,startByte:intent.startByte,endByte:intent.endByte,text:''});edits.sort((a,b)=>a.startByte-b.startByte);assert(edits.every((e,i)=>i===0||edits[i-1].endByte<=e.startByte),'Body call statements overlap');let result=Buffer.from(text);for(const e of [...edits].reverse())result=Buffer.concat([result.subarray(0,e.startByte),Buffer.from(e.text),result.subarray(e.endByte)]);assert(result.length<=4_000_000&&Buffer.from(result.toString('utf8')).equals(result),'Body result capacity/UTF-8');source.set(file,result.toString('utf8'));let after,afterDiagnostics;const afterService=makeService();try{const p=afterService.getProgram();afterDiagnostics=ts.getPreEmitDiagnostics(p).map(d=>diagnostic(d,context.name));assert(afterDiagnostics.length<=20_000&&!afterDiagnostics.some(d=>d.severity==='error'),'Body context has errors after expansion');after=observe(p);}finally{afterService.dispose();source.set(file,text);}
  const libraries=Object.fromEntries([...libCache].filter(([,v])=>v!==undefined).map(([p,v])=>[path.basename(p),sha(v)]).sort(([a],[b])=>a.localeCompare(b))),options=JSON.parse(JSON.stringify(context.parsed.options,(k,v)=>k==='configFile'?undefined:typeof v==='string'?v.replaceAll(root,'[snapshot]'):v)),contextId=sha(JSON.stringify([input.snapshotId,context.name,options,context.parsed.fileNames.map(rel).sort(),ts.version,compilerHash,libraries]));
  return{schema:1,operation:'inlineBody',snapshotId:input.snapshotId,inputHash:process.argv[2],compiler:ts.version,compilerHash,sourceBindings:input.sourceBindings,sourceExecuted:false,sourceTreeWritten:false,exhaustive:false,status:'passed',contexts:[],diagnostics:[],outputs:[],standardLibraryHashes:libraries,limitations:[...limitations,'Body frames are finite captured strict-module expansions; dynamic consumers, reflective call stacks and general runtime equivalence remain unverified.'],bodyInline:{schema:1,path:intent.path,keepDefinition:intent.keepDefinition,config:context.name,contextId,actionKind:'native.inline.function.body',frames,references,before,after,beforeDiagnostics,afterDiagnostics,edits,resultHash:sha(result)}};
 }finally{service.dispose();source.set(file,text);}
}
