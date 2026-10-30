// Owned protocol fixture only; it is not a compiler or production language server.
import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const [log,mode='ok']=process.argv.slice(2);
let input=Buffer.alloc(0), uri, source;
function reply(message,result){const body=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:message.id,result}));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function range(message){const {line,character}=message.params.position;const name=/^[$_\p{ID_Start}][$\u200c\u200d_\p{ID_Continue}]*/u.exec(source.split('\n')[line].slice(character))[0];return {start:{line,character},end:{line,character:character+name.length}};}
function handle(message){
 fs.appendFileSync(log,JSON.stringify(message)+'\n');
 if(message.method==='initialize'){reply(message,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,definitionProvider:true,referencesProvider:mode!=='missing-references'}});return;}
 if(message.method==='textDocument/didOpen'){uri=message.params.textDocument.uri;source=message.params.textDocument.text;return;}
 if(message.method==='textDocument/definition'){reply(message,mode==='empty-definition'?[]:[{uri,range:range(message)}]);return;}
 if(message.method==='textDocument/references'){
  if(mode==='empty-references'){reply(message,[]);return;}
  if(mode==='foreign-reference'){reply(message,[{uri,range:range(message)},{uri:pathToFileURL(path.resolve(process.cwd(),'other.ts')).href,range:{start:{line:0,character:13},end:{line:0,character:22}}}]);return;}
  if(mode==='overflow-references'){reply(message,Array.from({length:2001},()=>({uri,range:range(message)})));return;}
  reply(message,[{uri,range:range(message)}]);return;
 }
 if(message.method==='shutdown'){reply(message,null);return;}
 if(message.method==='exit'){process.exit(0);}
 if(message.id!==undefined)reply(message,null);
}
process.stdin.on('data',chunk=>{input=Buffer.concat([input,chunk]);if(input.length>4_000_000)process.exit(2);for(;;){const header=input.indexOf('\r\n\r\n');if(header<0)return;const length=Number(/Content-Length: (\d+)/i.exec(input.subarray(0,header).toString())[1]);if(input.length<header+4+length)return;const message=JSON.parse(input.subarray(header+4,header+4+length));input=input.subarray(header+4+length);handle(message);}});
