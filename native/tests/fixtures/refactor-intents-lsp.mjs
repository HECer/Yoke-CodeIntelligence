// Independently owned protocol fixture, not a compiler or IDE implementation.
import fs from 'node:fs';
const mode = process.argv[2] ?? 'rename';
const transcript = process.argv[3];
let buffer = Buffer.alloc(0), uri, source;
function reply(message, result) {
  const bytes = Buffer.from(JSON.stringify({jsonrpc:'2.0', id:message.id, result}));
  process.stdout.write(`Content-Length: ${bytes.length}\r\n\r\n`);
  process.stdout.write(bytes);
}
function position(offset) {
  const prefix = source.slice(0, offset).split('\n');
  return {line:prefix.length - 1, character:prefix.at(-1).length};
}
function edit(start, end, newText) {
  return {range:{start:position(start), end:position(end)}, newText};
}
function handle(message) {
  if (transcript) fs.appendFileSync(transcript, `${message.method}\n`);
  if (message.method === 'initialize') {
    reply(message, {capabilities:{positionEncoding:'utf-16', textDocumentSync:1,
      renameProvider:true, codeActionProvider:{codeActionKinds:['refactor.inline']}}});
    return;
  }
  if (message.method === 'textDocument/didOpen') {
    uri = message.params.textDocument.uri; source = message.params.textDocument.text; return;
  }
  if (message.method === 'textDocument/rename') {
    if (mode === 'rename-unrelated' || mode === 'rename-unrelated-wholefile') {
      const at = source.indexOf('caller');
      reply(message, {changes:{[uri]:[mode === 'rename-unrelated'
        ? edit(at, at + 6, 'another') : edit(0, source.length, source.replace('caller', 'another'))]}}); return;
    }
    const replacements = [];
    const oldName = mode === 'rename-raw' ? 'r' : mode.startsWith('rename-literal-') ? mode.at(-1) : 'helper';
    let base = 0;
    for (const line of source.split('\n')) {
      if ((!line.trimStart().startsWith('//') || mode === 'rename-extra-comment')
          && (!line.includes('const LABEL') || mode === 'rename-extra-string')) {
        for (const match of line.matchAll(new RegExp(`\\b${oldName}\\b`, 'g'))) {
          replacements.push(edit(base + match.index, base + match.index + oldName.length, message.params.newName));
        }
      }
      base += line.length + 1;
    }
    reply(message, {changes:{[uri]:replacements}}); return;
  }
  if (message.method === 'textDocument/codeAction') {
    const lineEnd = source.indexOf('\n');
    const call = source.lastIndexOf('helper()');
    const edits = [edit(0, lineEnd, ''), edit(call, call + 8, '(7)')];
    const action = {title:'Owned inline candidate', kind:'refactor.inline', edit:{changes:{[uri]:edits}}};
    if (mode === 'inline-unrelated') {
      const at = source.indexOf('caller');
      action.edit.changes[uri] = [edit(0, lineEnd, ''), edit(at, at + 6, 'another')];
    }
    if (mode === 'inline-wholefile') action.edit.changes[uri] = [edit(0, source.length,
      source.slice(lineEnd).replace('helper()', '(7)'))];
    if (mode === 'inline-command') action.command = {title:'Never execute', command:'fixture/neverExecute'};
    const actions = [action];
    if (mode === 'inline-ambiguous') actions.push({...action, title:'Second valid alternative'});
    if (mode === 'inline-batch-noop') actions.push({title:'No changes', kind:'refactor.inline', edit:{changes:{[uri]:[]}}});
    if (mode === 'inline-batch-nul' || mode === 'inline-batch-secret') actions.push({title:'Invalid final source', kind:'refactor.inline',
      edit:{changes:{[uri]:[edit(call, call + 8, mode === 'inline-batch-nul' ? '\0' : '-----BEGIN PRIVATE KEY-----')]}}});
    if (mode === 'inline-amplified') {
      for (let i = 0; i < 34; i++) actions.push({...action, title:`Alternative ${i}`});
    }
    if (mode === 'inline-batch-bad') actions.push({title:'Invalid second candidate', kind:'refactor.inline',
      edit:{changes:{[uri]:[{range:{start:{line:999,character:0},end:{line:999,character:1}},newText:'bad'}]}}});
    reply(message, actions); return;
  }
  if (message.method === 'shutdown') {reply(message, null); return;}
  if (message.method === 'exit') process.exit(0);
  if (message.id !== undefined) reply(message, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const end = buffer.indexOf('\r\n\r\n');
    if (end < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
    if (buffer.length < end + 4 + length) return;
    const message = JSON.parse(buffer.subarray(end + 4, end + 4 + length));
    buffer = buffer.subarray(end + 4 + length);
    handle(message);
  }
});
