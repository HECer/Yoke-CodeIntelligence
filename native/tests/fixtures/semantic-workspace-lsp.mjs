// Owned protocol fixture: registered semantic roots control cross-package observations.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const [log,mode=''] = process.argv.slice(2);
if(!log)process.exit(2);
let buffer=Buffer.alloc(0),root,folders,info,pending;
const documents=new Map();
function send(message){const bytes=Buffer.from(JSON.stringify(message));process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);process.stdout.write(bytes);}
function reply(message,result){send({jsonrpc:'2.0',id:message.id,result});}
function answer(message){
  if(!info.callbackVerified){pending=message;return;}
  if(message.method==='textDocument/hover')reply(message,{contents:{kind:'plaintext',value:JSON.stringify(info)}});
  else if(message.method==='textDocument/references'){
    const target=path.join(root,'shared','\u00e9.rs');
    reply(message,info.folders.includes('shared')?[{uri:pathToFileURL(target).href,range:{start:{line:1,character:7},end:{line:1,character:13}}}]:[]);
  }else if(message.method==='textDocument/codeAction')reply(message,[{title:'owned semantic roots',kind:'refactor.rewrite',edit:{changes:{[message.params.textDocument.uri]:[{range:{start:{line:1,character:3},end:{line:1,character:8}},newText:'local_changed'}]}}}]);
}
function handle(message){
  fs.appendFileSync(log,JSON.stringify({method:message.method,id:message.id})+'\n');
  if(message.method==='initialize'){
    root=fileURLToPath(message.params.rootUri);folders=message.params.workspaceFolders;
    const relative=folders.map(folder=>path.relative(root,fileURLToPath(folder.uri)).replaceAll('\\','/')||'.');
    info={folders:relative,fullCapturedExtraPresent:fs.existsSync(path.join(root,'shared','\u00e9.rs')),callbackVerified:false,callbackAdvertised:message.params.capabilities.workspace.workspaceFolders===true};
    fs.appendFileSync(log,JSON.stringify({event:'initialize',...info})+'\n');
    reply(message,{capabilities:{hoverProvider:true,referencesProvider:true,textDocumentSync:1,positionEncoding:'utf-16',codeActionProvider:{codeActionKinds:['refactor.rewrite']}}});
  }else if(message.method==='initialized'){
    if(info.callbackAdvertised)send({jsonrpc:'2.0',id:'owned-semantic-roots',method:'workspace/workspaceFolders',params:mode==='bad-callback'?{unexpected:true}:null});
    else info.callbackVerified=true;
  }else if(message.method===undefined&&message.id==='owned-semantic-roots'){
    if(message.error||JSON.stringify(message.result)!==JSON.stringify(folders))process.exit(3);
    info.callbackVerified=true;fs.appendFileSync(log,JSON.stringify({event:'callback',...info})+'\n');if(pending){const current=pending;pending=undefined;answer(current);}
  }else if(message.method==='textDocument/didOpen')documents.set(message.params.textDocument.uri,message.params.textDocument.text);
  else if(['textDocument/hover','textDocument/references','textDocument/codeAction'].includes(message.method))answer(message);
  else if(message.method==='shutdown')reply(message,null);
  else if(message.method==='exit')process.exit(0);
  else if(message.id!==undefined)send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Owned method unavailable'}});
}
process.stdin.on('data',chunk=>{
  buffer=Buffer.concat([buffer,chunk]);if(buffer.length>4000000)process.exit(4);
  for(;;){const boundary=buffer.indexOf('\r\n\r\n');if(boundary<0)return;
    const header=/Content-Length: (\d+)/i.exec(buffer.subarray(0,boundary).toString());if(!header)process.exit(5);
    const size=Number(header[1]);if(size>4000000)process.exit(6);if(buffer.length<boundary+4+size)return;
    const message=JSON.parse(buffer.subarray(boundary+4,boundary+4+size));buffer=buffer.subarray(boundary+4+size);handle(message);
  }
});
