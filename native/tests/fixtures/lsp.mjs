// Deliberately labeled protocol fixture, not a real compiler or language server.
import { pathToFileURL, fileURLToPath } from 'node:url';
import path from 'node:path';
const adversary = process.argv[2];
let buffer = Buffer.alloc(0), root, current;
const opened = new Map();
function send(value) { const bytes=Buffer.from(JSON.stringify(value)); process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`); process.stdout.write(bytes); }
function handle(message) {
  if(message.method==='initialize') {root=message.params.rootUri;send({jsonrpc:'2.0',id:message.id,result:{capabilities:{positionEncoding:'utf-16',definitionProvider:true,referencesProvider:true,implementationProvider:true,hoverProvider:true,renameProvider:true,textDocumentSync:1}}});if(adversary==='blocked'){process.stdin.pause();send({jsonrpc:'2.0',method:'fixture/blocked',params:{paused:true}});setTimeout(()=>process.exit(0),3000);}return;}
  if(message.method==='textDocument/didOpen') {current=message.params.textDocument.uri;opened.set(current,message.params.textDocument.languageId);return;}
  if(message.method==='textDocument/references' && adversary==='many-large-locations') {const range={start:{line:0,character:0},end:{line:1,character:0}};send({jsonrpc:'2.0',id:message.id,result:Array.from({length:128},()=>({uri:current,range}))});return;}
  if(message.method==='textDocument/definition') {
    if(adversary==='react-language-ids') {
      const extension=path.extname(fileURLToPath(current));
      const targetLanguage=extension==='.tsx'?'typescriptreact':'javascriptreact';
      const siblingLanguage=extension==='.tsx'?'typescript':'javascript';
      const sibling=pathToFileURL(path.join(process.cwd(),`support${extension==='.tsx'?'.ts':'.js'}`)).href;
      if(opened.get(current)!==targetLanguage || opened.get(sibling)!==siblingLanguage || opened.size!==2) {send({jsonrpc:'2.0',id:message.id,error:{code:-32000,message:'Fixture document languages or provider siblings differ'}});return;}
    }
    const range={start:{line:1,character:3},end:{line:1,character:9}}; const result=adversary==='outside-selection'?{targetUri:current,targetRange:{start:{line:0,character:0},end:{line:0,character:2}},targetSelectionRange:range}:{uri:current,range};send({jsonrpc:'2.0',id:message.id,result});return;
  }
  if(message.method==='textDocument/rename') {
    let uri=current;
    if(adversary==='outside') uri=pathToFileURL(path.resolve(process.cwd(),'..','outside.rs')).href;
    const edits=[{range:{start:{line:1,character:3},end:{line:1,character:9}},newText:message.params.newName},{range:{start:{line:2,character:12},end:{line:2,character:18}},newText:message.params.newName}];
    const result=adversary==='version'?{documentChanges:[{textDocument:{uri,version:99},edits}]}:{changes:{[uri]:edits}};
    send({jsonrpc:'2.0',id:message.id,result});return;
  }
  if(message.method==='shutdown') {send({jsonrpc:'2.0',id:message.id,result:null});return;}
  if(message.method==='exit') { if(adversary==='slow-exit') {send({jsonrpc:'2.0',method:'fixture/slow-exit',params:{delayed:true}});setTimeout(()=>process.exit(0),3000);} else process.exit(0); }
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const message=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(message);}});
