import fs from 'node:fs';
let bytes = Buffer.alloc(0);
function send(value) {
  const body = Buffer.from(JSON.stringify(value));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}
process.stdin.on('data', chunk => {
  bytes = Buffer.concat([bytes, chunk]);
  for (;;) {
    const end = bytes.indexOf('\r\n\r\n');
    if (end < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(bytes.subarray(0,end).toString())[1]);
    if (bytes.length < end + 4 + length) return;
    const message = JSON.parse(bytes.subarray(end+4,end+4+length));
    bytes = bytes.subarray(end+4+length);
    if (message.method === 'initialize') send({jsonrpc:'2.0',id:message.id,result:{capabilities:{hoverProvider:true}}});
    else if (message.method === 'textDocument/hover') fs.appendFileSync(process.argv[2], 'hover\n');
    else if (message.method === 'shutdown') send({jsonrpc:'2.0',id:message.id,result:null});
    else if (message.method === 'exit') process.exit(0);
  }
});
