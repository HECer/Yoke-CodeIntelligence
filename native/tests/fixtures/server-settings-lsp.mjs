// Owned layered-settings contract fixture, not an implementation of a language runtime.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const [log, actor, delayText='0'] = process.argv.slice(2);
const delay=Number(delayText);
if(!log||!actor||!Number.isInteger(delay)||delay<0||delay>30000)process.exit(2);
let input=Buffer.alloc(0), root, initializationOptions;
const documents=new Map();
function send(message){const body=Buffer.from(JSON.stringify(message));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function reply(message,result){send({jsonrpc:'2.0',id:message.id,result});}
function handle(message){
  const method=message.method;
  fs.appendFileSync(log,JSON.stringify({actor,method,id:message.id,uri:message.params?.textDocument?.uri})+'\n');
  if(method==='initialize'){
    root=fileURLToPath(message.params.rootUri);initializationOptions=message.params.initializationOptions;
    fs.appendFileSync(log,JSON.stringify({actor,event:'initialization-options',options:initializationOptions,argv:process.argv.slice(2)})+'\n');
    reply(message,{capabilities:{hoverProvider:true,textDocumentSync:1,positionEncoding:'utf-16',codeActionProvider:{codeActionKinds:['refactor.rewrite']}}});
  }else if(method==='textDocument/didOpen'){
    documents.set(message.params.textDocument.uri,message.params.textDocument.languageId);
  }else if(method==='textDocument/hover'){
    const uri=message.params.textDocument.uri;
    setTimeout(()=>reply(message,{contents:{kind:'plaintext',value:JSON.stringify({actor,initializationOptions,argv:process.argv.slice(2),path:path.relative(root,fileURLToPath(uri)).replaceAll('\\','/'),languageId:documents.get(uri)})}}),delay);
  }else if(method==='textDocument/codeAction'){
    reply(message,[{title:initializationOptions?.actionTitle??'owned-action',kind:'refactor.rewrite',edit:{changes:{[message.params.textDocument.uri]:[{range:{start:{line:1,character:3},end:{line:1,character:10}},newText:'selected_name'}]}}}]);
  }else if(method==='shutdown'){reply(message,null);
  }else if(method==='exit'){process.exit(0);
  }else if(message.id!==undefined){send({jsonrpc:'2.0',id:message.id,error:{code:-32601,message:'Owned method unavailable'}});}
}
process.stdin.on('data',chunk=>{
  input=Buffer.concat([input,chunk]);if(input.length>4000000)process.exit(3);
  for(;;){const boundary=input.indexOf('\r\n\r\n');if(boundary<0)return;
    const header=/Content-Length: (\d+)/i.exec(input.subarray(0,boundary).toString());if(!header)process.exit(4);
    const size=Number(header[1]);if(size>4000000)process.exit(5);if(input.length<boundary+4+size)return;
    const message=JSON.parse(input.subarray(boundary+4,boundary+4+size));input=input.subarray(boundary+4+size);handle(message);
  }
});
