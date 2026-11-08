// Owned protocol fixture. This is not a compiler or an actual language server.
import fs from 'node:fs';
const [log, mode = '', original] = process.argv.slice(2);
let buffer = Buffer.alloc(0), uri, source, changed = false;
function reply(m, result) { const body = Buffer.from(JSON.stringify({jsonrpc:'2.0',id:m.id,result})); process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`); process.stdout.write(body); }
function position(at) { const prefix = source.slice(0, at).split('\n'); return {line:prefix.length-1,character:prefix.at(-1).length}; }
function location(at, name, selectedUri = uri) { return {uri:selectedUri,range:{start:position(at),end:position(at+name.length)}}; }
function declaration(name) { const regex = /\bfunction\s+([A-Za-z_$][\w$]*)/g; let match; while ((match = regex.exec(source))) if (match[1] === name) return match.index + match[0].lastIndexOf(name); return -1; }
function word(m) { const lines=source.split('\n');let at=m.params.position.character;for(let i=0;i<m.params.position.line;i++)at+=lines[i].length+1;const regex=/[A-Za-z_$][\w$]*/g;let match;while((match=regex.exec(source)))if(at>=match.index&&at<match.index+match[0].length)return match[0];throw Error('Fixture anchor absent'); }
function handle(m) {
 if (m.id !== undefined) fs.appendFileSync(log,JSON.stringify({method:m.method,includeDeclaration:m.params?.context?.includeDeclaration})+'\n');
 if (m.method==='initialize') { reply(m,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,definitionProvider:true,referencesProvider:true}}); return; }
 if (m.method==='textDocument/didOpen') { uri=m.params.textDocument.uri;source=m.params.textDocument.text;return; }
 if (m.method==='textDocument/definition') {
  const name=word(m), at=declaration(name);
  reply(m,mode==='no-definition'?null:mode==='foreign-definition'?[location(at,name,new URL('../outside.ts',uri).href)]:mode==='wrong-definition'?[location(declaration('keep'),'keep')]:[location(at,name)]);return;
 }
 if (m.method==='textDocument/references') {
  if(mode==='stall')return;
  const name=word(m), at=declaration(name);const regex=/[A-Za-z_$][\w$]*/g;let match,items=[];
  while((match=regex.exec(source)))if(match[0]===name)items.push(location(match.index,name));
  if(mode==='empty-references')items=[];
  if(mode==='omit-self')items=items.filter(item=>item.range.start.line!==position(at).line||item.range.start.character!==position(at).character);
  if(mode==='omit-use'&&name==='removeMe')items=[location(at,name)];
  if(mode==='truncated')items=Array.from({length:2001},()=>location(at,name));
  if(mode==='proof-overflow')items=Array.from({length:2000},()=>location(at,name));
  if(mode==='duplicates')items=[...items,...items];
  if(mode==='other-file-reference'&&name==='removeMe')items.push({uri:new URL('./other.ts',uri).href,range:{start:{line:0,character:0},end:{line:0,character:name.length}}});
  if(mode==='tamper-original'&&!changed){fs.writeFileSync(original,'// foreign edit\n');changed=true;}
  reply(m,mode==='malformed-references'?{}:items);return;
 }
 if(m.method==='shutdown'){reply(m,null);return;}
 if(m.method==='exit'){process.exit(0);}
 if(m.id!==undefined)reply(m,null);
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const size=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+size)return;const message=JSON.parse(buffer.subarray(end+4,end+4+size));buffer=buffer.subarray(end+4+size);handle(message);}});
