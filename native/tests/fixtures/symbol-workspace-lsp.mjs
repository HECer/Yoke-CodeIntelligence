// Owned fixture: inspect actual initialize roots and captured configuration.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const [log, mode=''] = process.argv.slice(2);
let buffer=Buffer.alloc(0), info, initializedFolders, root, pendingHover;
function reply(m,result) {
  const body=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:m.id,result}));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`); process.stdout.write(body);
}
function handle(m) {
  if(m.method==='initialize') {
    root=fileURLToPath(m.params.rootUri);
    if(m.params.capabilities.workspace.workspaceFolders!==true)process.exit(5);
    initializedFolders=m.params.workspaceFolders;
    info={folders:m.params.workspaceFolders.map(f=>path.relative(root,fileURLToPath(f.uri)).replaceAll('\\','/')||'.'),
      rootConfig:fs.readFileSync(path.join(root,'project-config.txt'),'utf8')};
    reply(m,{capabilities:{hoverProvider:true,positionEncoding:'utf-16',textDocumentSync:1}});
  } else if(m.method==='initialized') {
    const message=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:'owned-workspace-roots',method:'workspace/workspaceFolders',params:mode==='bad-callback'?{unexpected:true}:null}));
    process.stdout.write(`Content-Length: ${message.length}\r\n\r\n`);process.stdout.write(message);
  } else if(m.method===undefined&&m.id==='owned-workspace-roots') {
    if(m.error||JSON.stringify(m.result)!==JSON.stringify(initializedFolders))process.exit(6);
    info.callbackVerified=true;fs.appendFileSync(log,JSON.stringify(info)+'\n');
    if(pendingHover)reply(pendingHover,{contents:{kind:'plaintext',value:JSON.stringify(info)}});
  } else if(m.method==='textDocument/hover') {
    if(info.callbackVerified)reply(m,{contents:{kind:'plaintext',value:JSON.stringify(info)}});
    else pendingHover=m;
  } else if(m.method==='shutdown') {reply(m,null);
  } else if(m.method==='exit') {process.exit(0);
  } else if(m.id!==undefined) {reply(m,null);}
}
process.stdin.on('data', chunk=>{
  buffer=Buffer.concat([buffer,chunk]); if(buffer.length>1000000)process.exit(2);
  for(;;){
    const end=buffer.indexOf('\r\n\r\n');if(end<0)return;
    const match=/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString());if(!match)process.exit(3);
    const size=Number(match[1]);if(size>1000000)process.exit(4);
    if(buffer.length<end+4+size)return;
    const m=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(m);
  }
});
