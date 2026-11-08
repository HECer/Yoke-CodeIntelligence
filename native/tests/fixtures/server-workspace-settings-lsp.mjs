// Owned protocol contract fixture. This does not certify a language backend.
import fs from 'node:fs';
const [log] = process.argv.slice(2);
if (!log) process.exit(2);
let input = Buffer.alloc(0), settings = null, initializationOptions, pending;
function send(message) {
  const body = Buffer.from(JSON.stringify(message));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}
const reply = (message, result) => send({jsonrpc:'2.0', id:message.id, result});
function handle(message) {
  fs.appendFileSync(log, JSON.stringify(message)+'\n');
  if (message.id === 'owned-workspace-settings' && !message.method) {
    if (!pending || message.error) process.exit(6);
    reply(pending, {contents:{kind:'plaintext', value:JSON.stringify({settings, pulled:message.result, initializationOptions})}});
    pending = undefined;
  } else if (message.method === 'initialize') {
    initializationOptions = message.params.initializationOptions;
    reply(message, {capabilities:{hoverProvider:true, positionEncoding:'utf-16', codeActionProvider:{codeActionKinds:['refactor.rewrite']}}});
  } else if (message.method === 'workspace/didChangeConfiguration') {
    settings = message.params.settings;
  } else if (message.method === 'textDocument/hover') {
    if (pending) process.exit(7);
    pending = message;
    send({jsonrpc:'2.0', id:'owned-workspace-settings', method:'workspace/configuration', params:{items:[{}, {section:'nested'}, {section:'replace'}, {section:'missing'}, {section:'nested.selected'}]}});
  } else if (message.method === 'textDocument/codeAction') {
    reply(message, [{title:settings?.actionTitle ?? 'owned-action', kind:'refactor.rewrite', edit:{changes:{[message.params.textDocument.uri]:[{range:{start:{line:1,character:3},end:{line:1,character:10}},newText:'selected_name'}]}}}]);
  } else if (message.method === 'shutdown') reply(message, null);
  else if (message.method === 'exit') process.exit(0);
  else if (message.id !== undefined) send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Owned method unavailable'}});
}
process.stdin.on('data', chunk => {
  input = Buffer.concat([input,chunk]);
  if (input.length > 4000000) process.exit(3);
  for (;;) {
    const end = input.indexOf('\r\n\r\n');
    if (end < 0) return;
    const header = /Content-Length: (\d+)/i.exec(input.subarray(0,end).toString());
    if (!header) process.exit(4);
    const size = Number(header[1]);
    if (size > 4000000) process.exit(5);
    if (input.length < end+4+size) return;
    const message = JSON.parse(input.subarray(end+4,end+4+size));
    input = input.subarray(end+4+size);
    handle(message);
  }
});
