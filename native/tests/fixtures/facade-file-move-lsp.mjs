// Independently owned standard-LSP fixture, not an IDE or compiler.
import fs from 'node:fs';
const transcript = process.argv[2];
let buffer = Buffer.alloc(0), sourceUri;
function reply(message, result) {
  const bytes = Buffer.from(JSON.stringify({jsonrpc:'2.0', id:message.id, result}));
  process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);
  process.stdout.write(bytes);
}
function handle(message) {
  fs.appendFileSync(transcript, `${JSON.stringify({method:message.method,params:message.params})}\n`);
  if (message.method === 'initialize') {
    reply(message, {capabilities:{positionEncoding:'utf-16',textDocumentSync:1,
      workspace:{fileOperations:{willRename:{filters:[{scheme:'file',pattern:{glob:'**/*.rs',matches:'file'}}]}}}}});
  } else if (message.method === 'textDocument/didOpen') {
    sourceUri = message.params.textDocument.uri;
  } else if (message.method === 'workspace/willRenameFiles') {
    reply(message, {changes:{[sourceUri]:[{range:{start:{line:0,character:0},end:{line:0,character:0}},newText:'// provider prepared move\n'}]}});
  } else if (message.method === 'shutdown') reply(message, null);
  else if (message.method === 'exit') process.exit(0);
  else if (message.id !== undefined) reply(message, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer,chunk]);
  while (true) {
    const end = buffer.indexOf('\r\n\r\n');
    if (end < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);
    if (buffer.length < end + 4 + length) return;
    const message = JSON.parse(buffer.subarray(end + 4,end + 4 + length));
    buffer = buffer.subarray(end + 4 + length);
    handle(message);
  }
});
