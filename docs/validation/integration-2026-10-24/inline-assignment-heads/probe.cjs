'use strict';
// Trusted compiler diagnostic. The executed programs below are owned literals.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process'),assert=require('node:assert/strict'),vm=require('node:vm');
const repo=process.cwd(),revision=process.argv[2],output=path.resolve(process.argv[3]);
assert(revision==='worktree'||/^[a-f0-9]{40}$/.test(revision));fs.mkdirSync(output,{recursive:true});
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const read=relative=>{if(revision==='worktree')return fs.readFileSync(path.join(repo,relative));const r=spawnSync('git',['show',`${revision}:${relative}`],{cwd:repo,timeout:10000,maxBuffer:2000000,windowsHide:true});assert.equal(r.status,0);assert(!r.error);return r.stdout;};
const assembly=read('native/src/projects.rs').toString('utf8').split('pub(crate) fn worker_source() -> String {')[1].split('\npub fn analyze')[0];
const pieces=[...assembly.matchAll(/include_str!\("(compiler[^"/]+\.cjs)"\)/g)].map(m=>m[1]);assert.equal(pieces.length,10);
const sources=pieces.map(name=>({name,bytes:read('native/src/'+name)}));
const worker=sources.map(p=>p.bytes.toString('utf8')).join('\n'),workerPath=path.join(output,'compiler.cjs');fs.writeFileSync(workerPath,worker,{flag:'wx'});
const compiler=path.join(repo,'node_modules/typescript/lib/typescript.js'),ts=require(compiler);assert.equal(ts.version,'5.8.3');
const cases=[
 {name:'module-write-order',positive:true,source:"const events:string[]=[];let target=1;function helper(n:number):number{events.push('body:'+n);target=10;return n+target;}target=helper((events.push('arg:'+target),target));export const observed={target,events};"},
 {name:'unbraced-if-else',positive:true,source:"const events:string[]=[];function helper(n:number):number{events.push('body');return n*2;}function run(flag:boolean){let target=1;if(flag)target=helper((events.push('arg'),3));else target=5;return target;}export const observed=[run(false),run(true),events];"},
 {name:'throw-preserves-target',positive:true,source:"const events:string[]=[];function helper(n:number):number{events.push('body');if(n)throw 'owned';return n;}let target=7;try{target=helper((events.push('arg'),3));}catch(error){events.push(String(error));}finally{events.push('target:'+target);}export const observed={target,events};"},
 {name:'finally-overrides-return',positive:true,source:"const events:string[]=[];function helper(n:number):number{try{events.push('try');return n;}finally{events.push('finally');return n+2;}}let target=0;target=helper((events.push('arg'),3));export const observed={target,events};"},
 {name:'finally-cancels-return',positive:true,source:"function helper():number|undefined{exit:while(true){try{return 1;}finally{break exit;}}}let target:number|undefined=7;target=helper();export const observed={target,isUndefined:target===undefined};"},
 {name:'contextual-object-return',positive:true,source:"function helper():{run:(n:number)=>number}{return {run:n=>n+1};}let target:{run:(n:number)=>number}={run:n=>n};target=helper();export const observed=target.run(3);"},
 {name:'never-keeps-old-value',positive:true,source:"function helper():never{throw 'owned';}let target=7;const events:string[]=[];try{target=helper();}catch(error){events.push(String(error));}export const observed={target,events};"},
 {name:'wrapped-assignment-and-call',positive:true,source:"function helper(n:number):number{return n+1;}let target=0;(((target = ((helper(2))))));export const observed=target;"},
 {name:'unicode-crlf-comments',positive:true,source:"// 🧪\r\nfunction helper(valeurÉ:number):number{return valeurÉ+1;}\r\nlet target=0;\r\ntarget /* avant */ = /* appel */ ((helper(/* café */2)));\r\nexport const observed=target;\r\n"},
 {name:'javascript-jsdoc',positive:true,language:'js',source:"/** @param {number} n */\nfunction helper(n){return n+1;}\nlet target=0;target=helper(2);export const observed=target;\n"},
 ...[
  ['property-target','const box={value:0};box.value=helper();'],
  ['computed-target','const box=[0];function key(){return 0;}box[key()]=helper();'],
  ['effectful-base','const box={value:0};function base(){return box;}base().value=helper();'],
  ['compound-assignment','let target=0;target+=helper();'],
  ['logical-assignment','let target=0;target||=helper();'],
  ['chained-assignment','let target=0,other=0;other=target=helper();'],
  ['returned-assignment','let target=0;function run(){return target=helper();}run();'],
  ['nested-assignment','let target=0;function use(n:number){return n;}use(target=helper());'],
  ['loop-initializer','let target=0;for(target=helper();target<4;target++){}'],
  ['comma-expression','let target=0;((target=helper()),target++);'],
  ['asserted-target','let target=0;(target as number)=helper();'],
 ].map(([name,consumer])=>({name,positive:false,source:'function helper():number{return 3;}'+consumer+'export {};'})),
 {name:'destructuring-target',positive:false,source:'function helper():number[]{return [3];}let target=0;[target]=helper();export {};'},
];
const execute=(source,language)=>{const js=ts.transpileModule(source,{fileName:'main.'+language,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context={exports:{}};vm.runInNewContext(js,context,{timeout:1000});assert(Object.hasOwn(context.exports,'observed'));return JSON.stringify(context.exports.observed);};
const results=[];
for(const fixture of cases)for(const keepDefinition of [false,true]){
 const {source}=fixture,language=fixture.language||'ts',file='main.'+language,sf=ts.createSourceFile(file,source,ts.ScriptTarget.ES2022,true);let selected;
 function visit(n){if(ts.isFunctionDeclaration(n)&&n.name?.text==='helper')selected=n;ts.forEachChild(n,visit);}visit(sf);assert(selected);
 const offset=p=>Buffer.byteLength(source.slice(0,p)),contents={[file]:source,'tsconfig.json':JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'ESNext',moduleResolution:'Bundler',allowJs:true,checkJs:true,noEmit:true},files:[file]})};
 const sourceBindings=Object.fromEntries(Object.entries(contents).map(([p,t])=>[p,sha(t)])),intent={path:file,name:'helper',startByte:offset(selected.getStart(sf)),endByte:offset(selected.end),nameStartByte:offset(selected.name.getStart(sf)),nameEndByte:offset(selected.name.end),keepDefinition};
 const input={schema:1,operation:'inlineBody',snapshotId:'a'.repeat(64),compiler,contents,sourceBindings,intent},raw=JSON.stringify(input),started=Date.now();
 const r=spawnSync(process.execPath,['--max-old-space-size=512',workerPath,sha(raw)],{cwd:output,input:raw,encoding:'utf8',timeout:30000,maxBuffer:32000000,windowsHide:true});assert(!r.error,fixture.name);assert.equal(r.signal,null);
 const row={name:fixture.name,language,keepDefinition,expected:fixture.positive?'accepted':'refused',exit:r.status,stderr:r.stderr,elapsedMs:Date.now()-started,sourceHash:sha(source),inputHash:sha(raw)};
 if(r.status===0){
  const reply=JSON.parse(r.stdout),body=reply.bodyInline;assert.equal(reply.schema,1);assert.equal(reply.operation,'inlineBody');assert.equal(reply.snapshotId,input.snapshotId);assert.equal(reply.inputHash,sha(raw));assert.equal(reply.compiler,'5.8.3');assert.equal(reply.compilerHash,sha(fs.readFileSync(compiler)));assert.deepEqual(reply.sourceBindings,sourceBindings);assert.equal(reply.sourceExecuted,false);assert.equal(reply.sourceTreeWritten,false);assert.equal(reply.exhaustive,false);assert.equal(reply.status,'passed');assert.equal(body.keepDefinition,keepDefinition);
  assert.equal(body.beforeDiagnostics.filter(d=>d.severity==='error').length,0);assert.equal(body.afterDiagnostics.filter(d=>d.severity==='error').length,0);
  const bytes=Buffer.from(source);let result=Buffer.from(bytes),previous=0,delta=0;const inserted=[];
  for(const e of body.edits){assert.equal(e.path,file);assert(Number.isInteger(e.startByte)&&Number.isInteger(e.endByte)&&e.startByte>=previous&&e.endByte>=e.startByte&&e.endByte<=bytes.length);assert.equal(Buffer.byteLength(bytes.subarray(0,e.startByte).toString('utf8')),e.startByte);assert.equal(Buffer.byteLength(bytes.subarray(0,e.endByte).toString('utf8')),e.endByte);inserted.push([e.startByte+delta,e.startByte+delta+Buffer.byteLength(e.text)]);delta+=Buffer.byteLength(e.text)-(e.endByte-e.startByte);previous=e.endByte;}
  for(const e of [...body.edits].reverse())result=Buffer.concat([result.subarray(0,e.startByte),Buffer.from(e.text),result.subarray(e.endByte)]);
  assert(result.length<=4000000);assert.equal(sha(result),body.resultHash);assert.equal(Buffer.byteLength(result.toString('utf8')),result.length);assert.equal(result.includes(Buffer.from('function helper')),keepDefinition);
  const shift=position=>{let delta=0;for(const e of body.edits){assert(!(e.startByte<position&&position<e.endByte));if(e.endByte<=position)delta+=Buffer.byteLength(e.text)-(e.endByte-e.startByte);}return position+delta;};
  const target=body.before.bindings.find(b=>b.span.text==='target'&&body.frames.some(f=>b.span.startByte>=f.statementStart&&b.span.endByte<=f.callStart));assert(target,fixture.name);assert(target.targets.length>0);
  const expectedTargets=target.targets.map(t=>{assert.equal(t.kind,'captured');return{...t,span:{...t.span,startByte:shift(t.span.startByte),endByte:shift(t.span.endByte)}};});
  const copied=body.after.bindings.filter(b=>b.span.text==='target'&&inserted.some(([a,z])=>b.span.startByte>=a&&b.span.endByte<=z));assert(copied.length>0);assert(copied.every(b=>b.resolution==='resolved'&&JSON.stringify(b.targets)===JSON.stringify(expectedTargets)),fixture.name);
  const types=observations=>observations.types.filter(t=>t.span.text==='target').map(t=>t.type);assert(types(body.before).length>0);assert.deepEqual(types(body.before),types(body.after));
  const observed=execute(source,language);assert.equal(observed,execute(result.toString('utf8'),language));
  Object.assign(row,{accepted:true,resultHash:sha(result),observed,targetTypes:types(body.after),copiedBindings:copied.length,identitiesChecked:true,byteEditsChecked:true,targetsChecked:true,ownedRuntimeCompared:true});
 }else{assert.equal(r.status,1);assert.equal(r.stdout,'');assert.equal(r.stderr,'Body expression context needs sequencing',fixture.name);row.accepted=false;}
 row.matchesExpected=row.accepted===fixture.positive;results.push(row);
}
const report={schema:1,revision,workerHash:sha(worker),pieces:sources.map(p=>({name:p.name,hash:sha(p.bytes)})),compiler:ts.version,compilerHash:sha(fs.readFileSync(compiler)),node:process.version,results,matchesExpected:results.every(r=>r.matchesExpected),nativeAdmission:false,ownedLiteralFixturesExecuted:true,analyzedRepositoryCodeExecuted:false,independentRustReconstruction:'not run'};
fs.writeFileSync(path.join(output,'results.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({revision,cases:results.length,matchesExpected:report.matchesExpected,accepted:results.filter(r=>r.accepted).length,nativeAdmission:false}));if(revision==='worktree'&&!report.matchesExpected)process.exitCode=1;
