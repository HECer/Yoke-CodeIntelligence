// Owned protocol fixture. It observes a real hook marker before initialization;
// this is process-order evidence, not language-server runtime certification.
import fs from 'node:fs';
const [marker, log] = process.argv.slice(2);
if (!marker || !log) process.exit(90);
let input = Buffer.alloc(0);
let hookLines = 0;
function reply(id, result) {
  const body = Buffer.from(JSON.stringify({jsonrpc: '2.0', id, result}));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}
function handle(message) {
  if (message.method === 'initialize') {
    if (!fs.existsSync(marker)) process.exit(91);
    hookLines = fs.readFileSync(marker, 'utf8').trim().split(/\r?\n/).length;
    fs.appendFileSync(log, JSON.stringify({event: 'initialize', hookLines}) + '\n');
    reply(message.id, {capabilities: {hoverProvider: true, textDocumentSync: 1, positionEncoding: 'utf-16'}});
  } else if (message.method === 'textDocument/hover') {
    reply(message.id, {contents: {kind: 'plaintext', value: JSON.stringify({hookLines})}});
  } else if (message.method === 'shutdown') {
    reply(message.id, null);
  } else if (message.method === 'exit') {
    process.exit(0);
  } else if (message.id !== undefined) {
    reply(message.id, null);
  }
}
process.stdin.on('data', chunk => {
  input = Buffer.concat([input, chunk]);
  if (input.length > 4_000_000) process.exit(92);
  for (;;) {
    const boundary = input.indexOf('\r\n\r\n');
    if (boundary < 0) return;
    const header = /Content-Length: (\d+)/i.exec(input.subarray(0, boundary).toString());
    if (!header) process.exit(93);
    const size = Number(header[1]);
    if (!Number.isSafeInteger(size) || size > 4_000_000) process.exit(94);
    if (input.length < boundary + 4 + size) return;
    const message = JSON.parse(input.subarray(boundary + 4, boundary + 4 + size));
    input = input.subarray(boundary + 4 + size);
    handle(message);
  }
});
