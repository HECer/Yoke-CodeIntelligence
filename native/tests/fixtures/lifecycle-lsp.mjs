// Independent lifecycle protocol fixture; it is not a compiler.
import fs from 'node:fs';
const [log, mode] = process.argv.slice(2);
let buffer = Buffer.alloc(0);
function reply(message, result) {
 const body = Buffer.from(JSON.stringify({jsonrpc:'2.0', id:message.id, result}));
 process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`); process.stdout.write(body);
}
function handle(message) {
 if (message.method === 'initialize') {
  fs.appendFileSync(log, JSON.stringify({event:'initialize',pid:process.pid})+'\n');
  reply(message,{capabilities:mode==='bad-capabilities'?null:{positionEncoding:'utf-16',textDocumentSync:1,codeActionProvider:{codeActionKinds:['quickfix']}}}); return;
 }
 if (message.method === 'shutdown') {
  fs.appendFileSync(log,JSON.stringify({event:'shutdown',pid:process.pid})+'\n');reply(message,null);return;
 }
 if (message.method === 'exit') {process.exit(0);}
 if (message.id !== undefined) reply(message,null);
}
process.stdin.on('data', chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const n=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+n)return;const message=JSON.parse(buffer.subarray(end+4,end+4+n));buffer=buffer.subarray(end+4+n);handle(message);}});
