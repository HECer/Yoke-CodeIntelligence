// Independent public-contract fixture; not a production language server.
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
const mode=process.argv[2]??'';
let buffer=Buffer.alloc(0),uri,resolveNegotiated=false;
const action={title:'Inline helper',kind:'refactor.inline',isPreferred:true,data:{providerOpaque:['retained',{unicode:'🌍',sequence:7}]}};
function send(value){const bytes=Buffer.from(JSON.stringify(value));process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);process.stdout.write(bytes);}
function handle(message){
 const reply=result=>send({jsonrpc:'2.0',id:message.id,result});
 const fail=reason=>send({jsonrpc:'2.0',id:message.id,error:{code:-32602,message:reason}});
 if(message.method==='initialize'){
  const capability=message.params.capabilities?.textDocument?.codeAction;
  resolveNegotiated=capability?.dataSupport===true&&capability?.resolveSupport?.properties?.includes('edit');
  const capabilities={codeActionProvider:{codeActionKinds:['refactor.inline'],resolveProvider:mode!=='unadvertised'}};
  if(mode.startsWith('diagnostic-context'))capabilities.diagnosticProvider={interFileDependencies:false,workspaceDiagnostics:false};
  reply({capabilities});return;
 }
 if(message.method==='textDocument/didOpen'){uri=message.params.textDocument.uri;return;}
 if(message.method==='textDocument/codeAction'){
  if(mode.startsWith('diagnostic-context')&&message.params.context.diagnostics?.[0]?.data?.opaque!=='diagnostic-retained'){fail('Source-bound diagnostic context was omitted');return;}
  if(mode==='command-only'){reply([{title:'Untrusted command',command:'fixture/must-not-execute'}]);return;}
  if(mode==='deferred-command'){reply([{...action,command:{title:'Untrusted follow-up',command:'fixture/must-not-execute'}}]);return;}
  if(mode==='disabled'){reply([{...action,disabled:{reason:'Unavailable fixture intent'}}]);return;}
  if(mode==='unrequested-kind'){reply([{...action,kind:'source.fixAll'}]);return;}
  if(mode==='metadata-only'){reply([{title:'Inline helper',kind:'refactor.inline'}]);return;}
  reply([action]);return;
 }
 if(message.method==='textDocument/diagnostic'){
  const range=mode==='diagnostic-context-invalid'?{start:{line:99,character:0},end:{line:99,character:1}}:{start:{line:0,character:3},end:{line:0,character:9}};
  reply({kind:'full',items:[{range,severity:1,message:'Independent diagnostic context',code:'context-code',data:{opaque:'diagnostic-retained'}}]});return;
 }
 if(message.method==='codeAction/resolve'){
  if(!resolveNegotiated){fail('Client did not negotiate edit resolve and opaque data support');return;}
  if(['disabled','command-only','unrequested-kind','unadvertised'].includes(mode)){fail('Action must not be resolved');return;}
  const original=mode==='metadata-only'?{title:'Inline helper',kind:'refactor.inline'}:mode==='deferred-command'?{...action,command:{title:'Untrusted follow-up',command:'fixture/must-not-execute'}}:action;
  if(!isDeepStrictEqual(message.params,original)){fail('Original action or opaque provider data was changed');return;}
  let target=uri,range={start:{line:0,character:3},end:{line:0,character:9}};
  if(mode==='foreign-edit')target=pathToFileURL(path.resolve(process.cwd(),'..','foreign.rs')).href;
  if(mode==='invalid-range')range={start:{line:99,character:0},end:{line:99,character:1}};
  if(mode==='source-mutation')fs.writeFileSync(path.join(process.cwd(),'main.rs'),'fn malicious() {}\n');
  const resolved={...original,edit:{changes:{[target]:[{range,newText:'renamed'}]}}};
  if(mode==='changed-title')resolved.title='Different intent';
  if(mode==='changed-kind')resolved.kind='refactor.extract';
  if(mode==='changed-data')resolved.data={providerOpaque:'different'};
  reply(resolved);return;
 }
 if(message.method==='shutdown'){reply(null);return;}
 if(message.method==='exit'){process.exit(0);}
 if(message.id!==undefined)fail('Unimplemented fixture operation');
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const message=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(message);}});
