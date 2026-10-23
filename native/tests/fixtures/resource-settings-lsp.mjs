// Owned protocol fixture; no vendor-runtime certification or source execution.
import fs from 'node:fs';
const [log, mode = 'normal'] = process.argv.slice(2);
let input = Buffer.alloc(0), root, settings, pending;
const opened = new Map();
function send(message) {
  const body = Buffer.from(JSON.stringify(message));
  process.stdout.write('Content-Length: ' + body.length + '\r\n\r\n');
  process.stdout.write(body);
}
const reply = (message, result) => send({jsonrpc:'2.0', id:message.id, result});
function handle(message) {
  fs.appendFileSync(log, JSON.stringify(message) + '\n');
  if (message.id === 'owned-resource-pull' && !message.method) {
    if (!pending || message.error) process.exit(6);
    const alt = new URL('unicode/e\u0301-\u{1f9ed}.rs', root + '/').href;
    if (pending.method === 'textDocument/codeAction') {
      reply(pending, [{title:message.result[1]?.tag ?? 'base-action', kind:'refactor.rewrite',
        edit:{changes:{[pending.params.textDocument.uri]:[{
          range:{start:{line:1,character:3},end:{line:1,character:10}},newText:'selected_name'
        }]}}}]);
    } else {
      reply(pending, {contents:{kind:'plaintext', value:JSON.stringify({
        settings, pulled:message.result,
        openedMain:opened.get(pending.params.textDocument.uri), openedAlt:opened.get(alt)
      })}});
    }
    pending = undefined;
  } else if (message.method === 'initialize') {
    root = message.params.rootUri;
    reply(message, {capabilities:{hoverProvider:true, positionEncoding:'utf-16',
      codeActionProvider:{codeActionKinds:['refactor.rewrite']}}});
  } else if (message.method === 'textDocument/didOpen') {
    opened.set(message.params.textDocument.uri, message.params.textDocument.text);
  } else if (message.method === 'workspace/didChangeConfiguration') {
    settings = message.params.settings;
  } else if (message.method === 'textDocument/hover' || message.method === 'textDocument/codeAction') {
    pending = message;
    const alt = new URL('unicode/e\u0301-\u{1f9ed}.rs', root + '/').href;
    let items = [{}, {scopeUri:message.params.textDocument.uri}, {scopeUri:alt},
      {scopeUri:alt, section:'nested.selected'}, {scopeUri:alt, section:'base'},
      {scopeUri:new URL('.env', root + '/').href}, {scopeUri:'file:///outside-owned-tree.rs'},
      {scopeUri:'https://example.invalid/owned.rs'}, {scopeUri:root},
      {scopeUri:alt, section:'missing'}];
    if (mode === 'invalid-scope') items = [{scopeUri:42}];
    if (mode === 'oversized-scope') items = [{scopeUri:'file:///' + 'x'.repeat(8193)}];
    if (mode === 'invalid-item') items = [false];
    send({jsonrpc:'2.0', id:'owned-resource-pull', method:'workspace/configuration', params:{items}});
  } else if (message.method === 'shutdown') reply(message, null);
  else if (message.method === 'exit') process.exit(0);
  else if (message.id !== undefined) send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Owned method unavailable'}});
}
process.stdin.on('data', chunk => {
  input = Buffer.concat([input, chunk]);
  if (input.length > 4000000) process.exit(3);
  for (;;) {
    const end = input.indexOf('\r\n\r\n');
    if (end < 0) return;
    const header = /Content-Length: (\d+)/i.exec(input.subarray(0,end).toString());
    if (!header) process.exit(4);
    const size = Number(header[1]);
    if (size > 4000000) process.exit(5);
    if (input.length < end + 4 + size) return;
    const message = JSON.parse(input.subarray(end+4,end+4+size));
    input = input.subarray(end+4+size);
    handle(message);
  }
});
