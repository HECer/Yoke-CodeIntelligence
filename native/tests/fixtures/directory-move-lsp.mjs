// Owned standard-LSP protocol fixture. No IDE, compiler or project-code execution.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const transcript=process.argv[2],mode=process.argv[3]||'edits';
let buffer=Buffer.alloc(0);
function reply(message,result){
 const bytes=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:message.id,result}));
 process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);process.stdout.write(bytes);
}
function edit(text){return [{range:{start:{line:0,character:0},end:{line:0,character:0}},newText:text}];}
function handle(message){
 fs.appendFileSync(transcript,JSON.stringify({method:message.method,params:message.params})+'\n');
 if(message.method==='initialize'){
  const matches=mode==='file-only'?'file':'folder';
  reply(message,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,workspace:{fileOperations:{willRename:{filters:[{scheme:'file',pattern:{glob:'**',matches}}]}}}}});
 }else if(message.method==='workspace/willRenameFiles'){
  const directory=fileURLToPath(message.params.files[0].oldUri);
  if(!fs.statSync(directory).isDirectory())throw new Error('Fixture requires a real copied source folder');
  const source=pathToFileURL(path.join(directory,'a.rs')).href;
  const outside=pathToFileURL(path.resolve(directory,'..','use.rs')).href;
  const changes={[source]:edit('// provider prepared folder move\r\n'),[outside]:edit('// provider folder reference\r\n')};
  if(mode==='null')reply(message,null);
  else if(mode==='bad-resource')reply(message,{documentChanges:[{kind:'create',uri:pathToFileURL(path.join(directory,'injected.rs')).href}]});
  else if(mode==='outside')reply(message,{changes:{[pathToFileURL(path.resolve(directory,'../../..','owned-outside-source.rs')).href]:edit('// should refuse\n')}});
  else if(mode==='bad-version')reply(message,{documentChanges:[{textDocument:{uri:source,version:'fixture_private_value'},edits:edit('// should refuse\n')}]});
  else if(mode==='unknown-root')reply(message,{changes,unexpected:'fixture_private_value'});
  else reply(message,{changes});
 }else if(message.method==='shutdown')reply(message,null);
 else if(message.method==='exit')process.exit(0);
 else if(message.id!==undefined)reply(message,null);
}
process.stdin.on('data',chunk=>{
 buffer=Buffer.concat([buffer,chunk]);if(buffer.length>4_000_000)throw new Error('Fixture frame bound');
 for(;;){
  const end=buffer.indexOf('\r\n\r\n');if(end<0)return;
  const match=/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString());if(!match)throw new Error('Fixture framing');
  const length=Number(match[1]);if(length>4_000_000)throw new Error('Fixture payload bound');
  if(buffer.length<end+4+length)return;
  const message=JSON.parse(buffer.subarray(end+4,end+4+length));buffer=buffer.subarray(end+4+length);handle(message);
 }
});
