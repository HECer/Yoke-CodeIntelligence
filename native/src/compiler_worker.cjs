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
 assert(input.schema===1 && ['analyze','build','inline','move','inlineBody','hierarchy'].includes(input.operation),'Compiler request contract');
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
 if(input.operation==='analyze'||input.operation==='hierarchy'){
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
   const context={config:p.name,contextId:sha(JSON.stringify([input.snapshotId,p.name,options,files,ts.version,compilerHash,libraries])),options,files,standardLibraryHashes:libraries,status:errors.some(d=>d.severity==='error')?'failed':'passed',diagnostics:errors,bindings};
   if(input.operation==='hierarchy')context.hierarchy=compilerHierarchy({input,ts,checker,program,span,source,slash,rel,root,assert});
   contexts.push(context);
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
