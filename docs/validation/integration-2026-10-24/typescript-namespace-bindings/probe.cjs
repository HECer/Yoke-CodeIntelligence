'use strict';
// Runs only the selected trusted compiler and the owned literal cases below.
// This direct worker diagnostic does not perform independent Rust admission.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process'),assert=require('node:assert/strict'),vm=require('node:vm');
const repo=process.cwd(),revision=process.argv[2],output=path.resolve(process.argv[3]);
assert(revision==='worktree'||/^[a-f0-9]{40}$/.test(revision));
fs.mkdirSync(output,{recursive:true});
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const read=relative=>{
 if(revision==='worktree')return fs.readFileSync(path.join(repo,relative));
 const r=spawnSync('git',['show',`${revision}:${relative}`],{cwd:repo,timeout:10000,maxBuffer:2000000,windowsHide:true});
 assert.equal(r.status,0);assert(!r.error);return r.stdout;
};
const assembly=read('native/src/projects.rs').toString('utf8').split('pub(crate) fn worker_source() -> String {')[1].split('\npub fn analyze')[0];
const pieces=[...assembly.matchAll(/include_str!\("(compiler[^"/]+\.cjs)"\)/g)].map(m=>m[1]);assert.equal(pieces.length,10);
const sources=pieces.map(name=>({name,bytes:read('native/src/'+name)}));
const worker=sources.map(p=>p.bytes.toString('utf8')).join('\n'),workerPath=path.join(output,'compiler.cjs');
fs.writeFileSync(workerPath,worker,{flag:'wx'});
const compiler=path.join(repo,'node_modules/typescript/lib/typescript.js'),ts=require(compiler);assert.equal(ts.version,'5.8.3');
const cases=[
 {name:'return-type-under-value',binding:'Result',positive:true,source:'type Result={run:(n:number)=>number};function helper(n:number):Result{return {run:value=>value+n};}export function outer(Result:number){const result=helper(2);return result.run(Result);}export const observed=outer(3);'},
 {name:'value-under-type',binding:'BASE',positive:true,source:'const BASE=2;function helper(n:number):number{return n+BASE;}export function outer(){type BASE={unused:true};const extra:BASE={unused:true};const result=helper(3);return [result,extra.unused];}export const observed=outer();'},
 {name:'qualified-type-under-value',binding:'Types',positive:true,source:'namespace Types{export type Result={value:number};}function helper(n:number):Types.Result{return {value:n};}export function outer(Types:number){const result=helper(2);return result.value+Types;}export const observed=outer(3);'},
 {name:'typeof-value-under-type',binding:'Witness',positive:true,source:'const Witness:number=3;function helper(n:typeof Witness):typeof Witness{return n;}export function outer(){type Witness={unused:true};const extra:Witness={unused:true};const result=helper(3);return [result,extra.unused];}export const observed=outer();'},
 {name:'shorthand-value-under-type',binding:'BASE',positive:true,source:'const BASE=2;function helper():{BASE:number}{return {BASE};}export function outer(){type BASE={unused:true};const extra:BASE={unused:true};const result=helper();return [result.BASE,extra.unused];}export const observed=outer();'},
 {name:'unicode-crlf-type-under-value',binding:'Résultat',positive:true,source:'// 🧪\r\ntype Résultat={value:number};\r\nfunction helper(n:number):Résultat{return {value:n};}\r\nexport function outer(Résultat:number){const result=helper(2);return result.value+Résultat;}\r\nexport const observed=outer(3);\r\n'},
 {name:'true-type-shadow',positive:false,source:'type Result={run:(n:number)=>number};function helper():Result{return {run:n=>n+1};}export function outer(){type Result={value:number};const unused:Result={value:3};const result=helper();return result.run(unused.value);}'},
 {name:'true-value-shadow',positive:false,source:'const BASE=2;function helper(n:number):number{return n+BASE;}export function outer(BASE:number){const result=helper(3);return result;}export const observed=outer(4);'},
 {name:'typeof-value-shadow',positive:false,source:'const Witness:number=3;function helper(n:typeof Witness):typeof Witness{return n;}export function outer(Witness:number){const value=helper(3);return value+Witness;}export const observed=outer(2);'},
 {name:'qualified-namespace-shadow',positive:false,source:'namespace Types{export type Result={value:number};}function helper(n:number):Types.Result{return {value:n};}namespace Consumer{namespace Types{export type Result={value:string};}export function outer(){const extra:Types.Result={value:"x"};const result=helper(2);return [result.value,extra.value];}}export const observed=Consumer.outer();'},
 {name:'shorthand-value-shadow',positive:false,source:'const BASE=2;function helper():{BASE:number}{return {BASE};}export function outer(BASE:number){const result=helper();return result.BASE;}export const observed=outer(4);'},
];
const execute=source=>{
 const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const context={exports:{}};vm.runInNewContext(js,context,{timeout:1000});
 assert(Object.hasOwn(context.exports,'observed'));return JSON.stringify(context.exports.observed);
};
const references=(text,name)=>{
 const sf=ts.createSourceFile('main.ts',text,ts.ScriptTarget.ES2022,true),starts=new Set();
 function visit(n){if(ts.isIdentifier(n)&&n.text===name){const p=n.parent,shorthand=ts.isShorthandPropertyAssignment(p);if((shorthand||!ts.isDeclarationName(n))&&!(ts.isPropertyAccessExpression(p)&&p.name===n)&&!(ts.isQualifiedName(p)&&p.right===n))starts.add(Buffer.byteLength(text.slice(0,n.getStart(sf))));}ts.forEachChild(n,visit);}visit(sf);return starts;
};
const results=[];
for(const fixture of cases)for(const keepDefinition of [false,true]){
 const {source}=fixture,sf=ts.createSourceFile('main.ts',source,ts.ScriptTarget.ES2022,true);let selected;
 function visit(n){if(ts.isFunctionDeclaration(n)&&n.name?.text==='helper')selected=n;ts.forEachChild(n,visit);}visit(sf);assert(selected);
 const offset=p=>Buffer.byteLength(source.slice(0,p)),contents={'main.ts':source,'tsconfig.json':JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'ESNext',moduleResolution:'Bundler',noEmit:true},files:['main.ts']})};
 const sourceBindings=Object.fromEntries(Object.entries(contents).map(([p,t])=>[p,sha(t)]));
 const intent={path:'main.ts',name:'helper',startByte:offset(selected.getStart(sf)),endByte:offset(selected.end),nameStartByte:offset(selected.name.getStart(sf)),nameEndByte:offset(selected.name.end),keepDefinition};
 const input={schema:1,operation:'inlineBody',snapshotId:'a'.repeat(64),compiler,contents,sourceBindings,intent},raw=JSON.stringify(input);
 const started=Date.now(),r=spawnSync(process.execPath,['--max-old-space-size=512',workerPath,sha(raw)],{cwd:output,input:raw,encoding:'utf8',timeout:30000,maxBuffer:32000000,windowsHide:true});
 assert(!r.error,`${fixture.name}: ${r.error}`);assert.equal(r.signal,null);
 const row={name:fixture.name,keepDefinition,expected:fixture.positive?'accepted':'refused',exit:r.status,stderr:r.stderr,elapsedMs:Date.now()-started,sourceHash:sha(source),inputHash:sha(raw)};
 if(r.status===0){
  const reply=JSON.parse(r.stdout),body=reply.bodyInline;
  assert.equal(reply.schema,1);assert.equal(reply.operation,'inlineBody');assert.equal(reply.snapshotId,input.snapshotId);assert.equal(reply.inputHash,sha(raw));assert.equal(reply.compiler,'5.8.3');assert.equal(reply.compilerHash,sha(fs.readFileSync(compiler)));assert.deepEqual(reply.sourceBindings,sourceBindings);
  assert.equal(reply.sourceExecuted,false);assert.equal(reply.sourceTreeWritten,false);assert.equal(reply.exhaustive,false);assert.equal(reply.status,'passed');assert.equal(body.keepDefinition,keepDefinition);
  assert.equal(body.beforeDiagnostics.filter(d=>d.severity==='error').length,0);assert.equal(body.afterDiagnostics.filter(d=>d.severity==='error').length,0);
  const bytes=Buffer.from(source);let result=Buffer.from(bytes),previous=0,delta=0;const inserted=[];
  for(const e of body.edits){assert.equal(e.path,'main.ts');assert(Number.isInteger(e.startByte)&&Number.isInteger(e.endByte)&&e.startByte>=previous&&e.endByte>=e.startByte&&e.endByte<=bytes.length);assert.equal(Buffer.byteLength(bytes.subarray(0,e.startByte).toString('utf8')),e.startByte);assert.equal(Buffer.byteLength(bytes.subarray(0,e.endByte).toString('utf8')),e.endByte);inserted.push([e.startByte+delta,e.startByte+delta+Buffer.byteLength(e.text)]);delta+=Buffer.byteLength(e.text)-(e.endByte-e.startByte);previous=e.endByte;}
  for(const e of [...body.edits].reverse())result=Buffer.concat([result.subarray(0,e.startByte),Buffer.from(e.text),result.subarray(e.endByte)]);
  assert(result.length<=4000000);assert.equal(sha(result),body.resultHash);assert.equal(Buffer.byteLength(result.toString('utf8')),result.length);
  assert.equal(result.includes(Buffer.from('function helper')),keepDefinition);
  const beforeReferences=references(source,fixture.binding),afterReferences=references(result.toString('utf8'),fixture.binding);
  const originals=body.before.bindings.filter(b=>b.span.text===fixture.binding&&beforeReferences.has(b.span.startByte)&&b.span.startByte>=intent.startByte&&b.span.endByte<=intent.endByte);
  assert(originals.length>0,fixture.name);const targets=originals[0].targets;assert(targets.length>0);assert(originals.every(b=>JSON.stringify(b.targets)===JSON.stringify(targets)),fixture.name);
  // The named top-level declaration precedes every edit, including deletion.
  assert(targets.every(t=>t.kind==='captured'&&t.span.endByte<body.edits[0].startByte));
  const copied=body.after.bindings.filter(b=>b.span.text===fixture.binding&&afterReferences.has(b.span.startByte)&&inserted.some(([a,z])=>b.span.startByte>=a&&b.span.endByte<=z));
  assert(copied.length>0,fixture.name);assert(copied.every(b=>b.resolution==='resolved'&&JSON.stringify(b.targets)===JSON.stringify(targets)),fixture.name);
  const resultTypes=observations=>observations.types.filter(t=>t.span.text==='result').map(t=>t.type);
  assert(resultTypes(body.before).length>0);assert.deepEqual(resultTypes(body.before),resultTypes(body.after));
  const beforeObserved=execute(source),afterObserved=execute(result.toString('utf8'));assert.equal(beforeObserved,afterObserved);
  Object.assign(row,{accepted:true,copiedBindings:copied.length,resultTypes:resultTypes(body.after),observed:afterObserved,resultHash:sha(result),identityChecked:true,byteEditsChecked:true,bindingsChecked:true,typesChecked:true,ownedRuntimeCompared:true});
 }else{
  assert.equal(r.status,1);assert.equal(r.stdout,'');assert.equal(r.stderr,'Body free binding would change at consumer');row.accepted=false;
 }
 row.matchesExpected=row.accepted===fixture.positive;results.push(row);
}
const report={schema:1,revision,workerHash:sha(worker),pieces:sources.map(p=>({name:p.name,hash:sha(p.bytes)})),compiler:ts.version,compilerHash:sha(fs.readFileSync(compiler)),node:process.version,results,matchesExpected:results.every(r=>r.matchesExpected),nativeAdmission:false,ownedLiteralFixturesExecuted:true,analyzedRepositoryCodeExecuted:false,independentRustReconstruction:'not run'};
fs.writeFileSync(path.join(output,'results.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({revision,cases:results.length,matchesExpected:report.matchesExpected,accepted:results.filter(r=>r.accepted).length,nativeAdmission:false}));
if(revision==='worktree'&&!report.matchesExpected)process.exitCode=1;
