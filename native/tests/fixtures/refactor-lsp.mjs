// Independent protocol fixture; this is not a compiler or language server.
import {pathToFileURL} from 'node:url';
import path from 'node:path';
const mode = process.argv[2] ?? '';
let buffer=Buffer.alloc(0), uri, source;
function reply(m,result){const body=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:m.id,result}));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function handle(m){
 if(m.method==='initialize'){reply(m,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,referencesProvider:true,codeActionProvider:{codeActionKinds:['refactor.inline','refactor.rewrite','quickfix']},diagnosticProvider:{interFileDependencies:false,workspaceDiagnostics:false},workspace:{fileOperations:{willRename:{filters:[{scheme:'file',pattern:{glob:'**/*.rs',matches:'file'}}]}}}}});return;}
 if(m.method==='textDocument/didOpen'){uri=m.params.textDocument.uri;source=m.params.textDocument.text;return;}
 if(m.method==='textDocument/references'){const range={start:{line:0,character:3},end:{line:0,character:9}};reply(m,mode==='foreign-reference'?[{uri:pathToFileURL(path.resolve(process.cwd(),'..','outside.rs')).href,range}]:mode==='provider-use'?[{uri,range}]:[]);return;}
 if(m.method==='textDocument/diagnostic'){reply(m,{kind:'full',items:[{range:{start:{line:0,character:3},end:{line:0,character:9}},severity:2,code:'unused',source:'owned-refactor-fixture',message:'fixture warning'}]});return;}
 if(m.method==='textDocument/codeAction'){const kind=mode==='wrong-action-kind'?'refactor.rename':'refactor.inline';const action={title:'Inline helper',kind};if(mode==='command-only')action.command={title:'Untrusted action',command:'fixture/neverExecute'};else action.edit={changes:{[uri]:[{range:{start:{line:0,character:0},end:{line:0,character:source.split('\n')[0].length}},newText:'// independently planned inline'}]}};reply(m,[action]);return;}
 if(m.method==='workspace/willRenameFiles'){reply(m,{changes:{[uri]:[{range:{start:{line:0,character:0},end:{line:0,character:0}},newText:'// rename prepared\n'}]}});return;}
 if(m.method==='shutdown'){reply(m,null);return;}
 if(m.method==='exit'){process.exit(0);}
 if(m.id!==undefined){reply(m,null);}
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const n=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+n)return;const m=JSON.parse(buffer.subarray(end+4,end+4+n));buffer=buffer.subarray(end+4+n);handle(m);}});
