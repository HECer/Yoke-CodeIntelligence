// Independent contract fixture, not a production language server.
import dgram from 'node:dgram';
// Optional host-owned observer: fixed scalar datagrams only. Never a reply,
// authority, completion proof or source/URI log. No acknowledgement is awaited.
let phaseObserver;
if(process.argv.length===4 && /^[1-9][0-9]{0,4}$/.test(process.argv[2])
   && Number(process.argv[2])<=65535 && /^[a-f0-9]{32}$/.test(process.argv[3])){
 let socket;
 try{
  socket=dgram.createSocket('udp4');socket.unref();
  phaseObserver={socket,port:Number(process.argv[2]),nonce:Buffer.from(process.argv[3],'hex'),start:performance.now(),seen:new Uint8Array(10)};
  socket.on('error',()=>{phaseObserver=undefined;try{socket.close();}catch{}});
 }catch{try{socket?.close();}catch{}}
}
function observe(phase){
 const observer=phaseObserver;if(!observer || observer.seen[phase])return;
 observer.seen[phase]=1;
 try{
  const frame=Buffer.alloc(22);frame[0]=1;observer.nonce.copy(frame,1);frame[17]=phase;
  frame.writeUInt32LE(Math.min(600000,Math.max(0,Math.trunc(performance.now()-observer.start))),18);
  observer.socket.send(frame,observer.port,'127.0.0.1',()=>{});
 }catch{}
}
observe(0);
let buffer=Buffer.alloc(0),text='',uri;
function send(value){const bytes=Buffer.from(JSON.stringify(value));process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);process.stdout.write(bytes);}
function handle(message){
 const reply=result=>send({jsonrpc:'2.0',id:message.id,result});
 if(message.method==='initialize'){observe(2);reply({capabilities:{diagnosticProvider:{interFileDependencies:false,workspaceDiagnostics:false}}});observe(3);return;}
 if(message.method==='textDocument/didOpen'){observe(4);({text,uri}=message.params.textDocument);return;}
 if(message.method==='textDocument/diagnostic'){
  observe(5);
  const lines=text.split('\n'),at={line:lines.length-1,character:lines.at(-1).length};
  const items=[{range:{start:at,end:at},severity:1,message:'Missing expression at EOF'}];
  if(lines.length>2){const next={line:1,character:0};items.push({range:{start:next,end:next},severity:2,message:'Next line declaration warning'});}
  reply({kind:'full',items});observe(6);return;
 }
 if(message.method==='shutdown'){observe(7);reply(null);observe(8);return;}
 if(message.method==='exit'){observe(9);process.exit(0);}
 if(message.id!==undefined)send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Unimplemented fixture operation'}});
}
process.stdin.on('data',chunk=>{observe(1);buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const message=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(message);}});
