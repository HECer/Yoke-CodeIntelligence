// Independent contract fixture, not a production language server.
let buffer=Buffer.alloc(0),text='',uri;
function send(value){const bytes=Buffer.from(JSON.stringify(value));process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);process.stdout.write(bytes);}
function handle(message){
 const reply=result=>send({jsonrpc:'2.0',id:message.id,result});
 if(message.method==='initialize'){reply({capabilities:{diagnosticProvider:{interFileDependencies:false,workspaceDiagnostics:false}}});return;}
 if(message.method==='textDocument/didOpen'){({text,uri}=message.params.textDocument);return;}
 if(message.method==='textDocument/diagnostic'){
  const lines=text.split('\n'),at={line:lines.length-1,character:lines.at(-1).length};
  const items=[{range:{start:at,end:at},severity:1,message:'Missing expression at EOF'}];
  if(lines.length>2){const next={line:1,character:0};items.push({range:{start:next,end:next},severity:2,message:'Next line declaration warning'});}
  reply({kind:'full',items});return;
 }
 if(message.method==='shutdown'){reply(null);return;}
 if(message.method==='exit'){process.exit(0);}
 if(message.id!==undefined)send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Unimplemented fixture operation'}});
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const message=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(message);}});
