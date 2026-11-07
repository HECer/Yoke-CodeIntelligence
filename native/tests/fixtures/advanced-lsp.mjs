// Own bounded protocol fixture. This is not an actual compiler/language server.
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import dgram from 'node:dgram';
const adversary = process.argv[2] ?? '';
// Explicit test-host correlation only. This channel cannot prove delivery or exit.
const observationPort = Number(process.argv[3]);
const observationNonce = process.argv[4];
let observationSocket, observationCount=0, observationBytes=0;
const observationStarted=performance.now();
if(Number.isInteger(observationPort)&&observationPort>0&&observationPort<=65535&&/^[a-f0-9]{64}$/.test(observationNonce??'')){
 observationSocket=dgram.createSocket('udp4');
 observationSocket.on('error',()=>{observationSocket=undefined;});
 observationSocket.unref();
}
const observationMethods=new Map([['startup','startup'],['initialize','initialize'],['initialized','initialized'],['textDocument/didOpen','did_open'],['textDocument/documentSymbol','document_symbol'],['textDocument/hover','hover'],['shutdown','shutdown'],['exit','exit']]);
function observe(phase,method){
 if(!observationSocket||observationCount>=64||!['started','received','reply_queued'].includes(phase))return;
 const timeMs=Math.floor(performance.now()-observationStarted);
 if(timeMs>600000)return;
 const bytes=Buffer.from(JSON.stringify({nonce:observationNonce,worker:process.pid,phase,method:observationMethods.get(method)??'other',timeMs}));
 if(bytes.length>=256||observationBytes+bytes.length>8192)return;
 observationCount++;observationBytes+=bytes.length;
 try{observationSocket.send(bytes,observationPort,'127.0.0.1',()=>{});}catch{observationSocket=undefined;}
}
observe('started','startup');
let buffer = Buffer.alloc(0), uri, settingsChecked=false, pendingTypeQuery;
const declaration = {start:{line:1,character:3},end:{line:1,character:9}};
const helper = {name:'helper',kind:12,uri:'',range:{start:{line:1,character:0},end:{line:1,character:14}},selectionRange:declaration,data:{providerOpaque:'retained'}};
const caller = {name:'main',kind:12,uri:'',range:{start:{line:2,character:0},end:{line:2,character:23}},selectionRange:{start:{line:2,character:3},end:{line:2,character:7}},data:{providerOpaque:'caller-retained'}};
const call = {start:{line:2,character:12},end:{line:2,character:18}};
function send(value) {const body=Buffer.from(JSON.stringify(value));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function reply(message,result){send({jsonrpc:'2.0',id:message.id,result});observe('reply_queued',message.method);}
function error(message,text){send({jsonrpc:'2.0',id:message.id,error:{code:-32602,message:text}});observe('reply_queued',message.method);}
function handle(m){
 observe('received',m.method);
 if(m.id==='fixture-config'){settingsChecked=m.result?.[0]?.enabled===true;if(pendingTypeQuery){if(settingsChecked)reply(pendingTypeQuery,{uri,range:declaration});else error(pendingTypeQuery,'Host settings did not reach workspace/configuration');pendingTypeQuery=undefined;}return;}
 if(m.method==='initialize'){const capabilities={positionEncoding:'utf-16',textDocumentSync:1,hoverProvider:true,declarationProvider:true,implementationProvider:true,typeDefinitionProvider:adversary!=='unsupported',callHierarchyProvider:true,typeHierarchyProvider:true,documentSymbolProvider:true,completionProvider:{resolveProvider:false},signatureHelpProvider:{triggerCharacters:['(']},diagnosticProvider:{interFileDependencies:false,workspaceDiagnostics:false},renameProvider:{prepareProvider:true},codeActionProvider:true};if(adversary.startsWith('push-'))delete capabilities.diagnosticProvider;reply(m,{capabilities});return;}
 if(m.method==='initialized'){if(adversary==='host-settings')send({jsonrpc:'2.0',id:'fixture-config',method:'workspace/configuration',params:{items:[{section:'feature'}]}});if(adversary==='configuration-flood')send({jsonrpc:'2.0',id:'fixture-flood',method:'workspace/configuration',params:{items:Array.from({length:1000},()=>({}))}});return;}
 if(m.method==='textDocument/didOpen'){uri=m.params.textDocument.uri;helper.uri=uri;caller.uri=uri;if(adversary.startsWith('push-'))send({jsonrpc:'2.0',method:'textDocument/publishDiagnostics',params:{uri:adversary==='push-upper-drive'?uri.replace(/^file:\/\/\/([a-z]):/,(_,drive)=>`file:///${drive.toUpperCase()}:`):uri,version:adversary==='push-stale'?99:1,diagnostics:[{range:call,severity:2,message:'Published fixture warning'}]}});return;}
 if(m.method==='textDocument/typeDefinition'){if(adversary==='diagnostic-flood')for(let n=0;n<20;n++)send({jsonrpc:'2.0',method:'textDocument/publishDiagnostics',params:{uri,version:1,diagnostics:[{range:call,severity:2,message:'x'.repeat(600000)}]}});if(adversary==='host-settings'&&!settingsChecked){pendingTypeQuery=m;return;}reply(m,{uri,range:declaration});return;}
 if(m.method==='textDocument/declaration'){reply(m,{uri,range:declaration});return;}
 if(m.method==='textDocument/hover'){reply(m,{contents:{kind:'markdown',value:'Independent fixture hover information'},range:declaration});return;}
 if(m.method==='textDocument/implementation'){reply(m,[{uri,range:caller.selectionRange}]);return;}
 if(m.method==='textDocument/prepareCallHierarchy'){reply(m,[helper]);return;}
 if(m.method==='callHierarchy/incomingCalls'){if(m.params.item.name==='main'){if(m.params.item.data?.providerOpaque!=='caller-retained'){error(m,'Caller hierarchy data was lost');return;}reply(m,adversary==='hierarchy-cycle'?[{from:helper,fromRanges:[declaration]}]:[]);return;}if(m.params.item.data?.providerOpaque!=='retained'){error(m,'Provider hierarchy data was lost');return;}const from={...caller,uri:adversary==='outside'?pathToFileURL(path.resolve(process.cwd(),'..','foreign.rs')).href:uri};reply(m,[{from,fromRanges:[call,call]}]);return;}
 if(m.method==='callHierarchy/outgoingCalls'){reply(m,[{to:helper,fromRanges:[call]}]);return;}
 if(m.method==='textDocument/prepareTypeHierarchy'){reply(m,[helper]);return;}
 if(m.method==='typeHierarchy/supertypes'||m.method==='typeHierarchy/subtypes'){reply(m,[caller]);return;}
 if(m.method==='textDocument/documentSymbol'){reply(m,[{name:'helper',kind:12,detail:'fn helper()',range:helper.range,selectionRange:adversary==='selection-outside'?call:declaration},{name:'main',kind:12,range:caller.range,selectionRange:caller.selectionRange}]);return;}
 if(m.method==='textDocument/completion'){const textEdit=adversary.startsWith('insert-replace-')?{insert:{start:{line:2,character:adversary==='insert-replace-start'?13:12},end:{line:2,character:adversary==='insert-replace-end'?19:17}},replace:call,newText:'helper'}:{range:adversary==='completion-outside'?{start:{line:99,character:0},end:{line:99,character:1}}:call,newText:'helper'};reply(m,{isIncomplete:false,items:[{label:'helper',kind:3,textEdit,additionalTextEdits:adversary==='completion-invalid-text'?[{range:declaration,newText:{provider:'invalid'}}]:[]}]});return;}
 if(m.method==='textDocument/signatureHelp'){reply(m,{signatures:[{label:'helper(value: string)',parameters:[{label:[7,20]}]}],activeSignature:0,activeParameter:0});return;}
 if(m.method==='textDocument/diagnostic'){const items=[{range:call,severity:2,code:'fixture-code',source:'owned-protocol-fixture',message:'A fixture warning 🌍'}];if(adversary==='reference-diagnostics')items.push({range:caller.selectionRange,severity:2,code:'unrelated',source:'owned-protocol-fixture',message:'Unrelated caller declaration warning'});reply(m,{kind:'full',items});return;}
 if(m.method==='textDocument/prepareRename'){reply(m,{range:declaration,placeholder:'helper'});return;}
 if(m.method==='textDocument/rename'){if(adversary==='vue-renamer'){const start=m.params.position;reply(m,{changes:{[uri]:[{range:{start,end:{line:start.line,character:start.character+6}},newText:m.params.newName}]}});return;}const target=pathToFileURL(path.join(process.cwd(),'renamed.rs')).href;reply(m,{documentChanges:[{kind:'rename',oldUri:uri,newUri:target},{textDocument:{uri:target,version:null},edits:[{range:declaration,newText:m.params.newName},{range:call,newText:m.params.newName}]}]});return;}
 if(m.method==='textDocument/codeAction'){
  if(adversary==='command-only'){reply(m,[{title:'External command',command:{title:'External command',command:'fixture/executeUntrusted',arguments:['never execute']}}]);return;}
  const edit=adversary==='resource-rename'?{documentChanges:[{kind:'rename',oldUri:uri,newUri:pathToFileURL(path.join(process.cwd(),'renamed.rs')).href}]}:{changes:{[uri]:[{range:declaration,newText:'renamed'},{range:call,newText:'renamed'}]}};
  reply(m,[{title:'Rename helper',kind:'refactor.rename',edit}]);return;
 }
 if(m.method==='shutdown'){reply(m,null);return;}
 if(m.method==='exit'){process.exit(0);}
 if(m.id!==undefined)error(m,'Unknown fixture method');
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const m=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(m);}});
