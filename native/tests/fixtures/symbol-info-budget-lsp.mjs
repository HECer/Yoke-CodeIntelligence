// Owned protocol fixture; no project modules or upstream application code run.
import fs from 'node:fs';
const [log, delayText = '100', mode = ''] = process.argv.slice(2);
const delay = Number(delayText);
if (!log || !Number.isFinite(delay) || delay < 0 || delay > 30000) process.exit(2);
let buffer = Buffer.alloc(0), uri;
const helper = {start:{line:1,character:3},end:{line:1,character:9}};
const main = {start:{line:2,character:3},end:{line:2,character:7}};
function send(m) {
  const body = Buffer.from(JSON.stringify(m));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}
function reply(m,result) { send({jsonrpc:'2.0',id:m.id,result}); }
function fail(m) { send({jsonrpc:'2.0',id:m.id,error:{code:-32603,message:'Owned hover refusal'}}); }
function handle(m) {
  fs.appendFileSync(log,JSON.stringify({method:m.method,position:m.params?.position})+'\n');
  if (m.method === 'initialize') {
    reply(m,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,
      documentSymbolProvider:true,hoverProvider:true,implementationProvider:true,declarationProvider:true}});
  } else if (m.method === 'textDocument/didOpen') {
    uri = m.params.textDocument.uri;
  } else if (m.method === 'textDocument/hover') {
    setTimeout(() => {
      if (mode === 'error') { fail(m); return; }
      const start=m.params.position;
      reply(m,{contents:{kind:'markdown',value:'Owned supplementary information 🧭'},
        range:{start,end:{line:start.line,character:start.character+1}}});
    },delay);
  } else if (m.method === 'textDocument/implementation') {
    reply(m,[{uri,range:helper},{uri,range:main}]);
  } else if (m.method === 'textDocument/declaration') {
    reply(m,{uri,range:helper});
  } else if (m.method === 'textDocument/documentSymbol') {
    reply(m,[
      {name:'helper',kind:12,range:{start:{line:1,character:0},end:{line:1,character:14}},selectionRange:helper},
      {name:'main',kind:12,range:{start:{line:2,character:0},end:{line:2,character:23}},selectionRange:main}
    ]);
  } else if (m.method === 'shutdown') { reply(m,null);
  } else if (m.method === 'exit') { process.exit(0);
  } else if (m.id !== undefined) { fail(m); }
}
process.stdin.on('data',chunk => {
  buffer=Buffer.concat([buffer,chunk]);
  if (buffer.length>1000000) process.exit(3);
  for (;;) {
    const end=buffer.indexOf('\r\n\r\n');if(end<0)return;
    const match=/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString());
    if(!match)process.exit(4);
    const size=Number(match[1]);if(size>1000000)process.exit(5);
    if(buffer.length<end+4+size)return;
    const message=JSON.parse(buffer.subarray(end+4,end+4+size));
    buffer=buffer.subarray(end+4+size);handle(message);
  }
});
