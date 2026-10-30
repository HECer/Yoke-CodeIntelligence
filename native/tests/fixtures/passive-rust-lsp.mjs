// Independent passive Rust provider-copy boundary fixture.
import fs from 'node:fs';let buffer=Buffer.alloc(0);
function reply(m,result){const body=Buffer.from(JSON.stringify({jsonrpc:'2.0',id:m.id,result}));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function handle(m){
 if(m.method==='initialize'){
   const selectors=['.cargo/config.toml','rust-toolchain.toml','rust-project.json','nested/RuSt-PrOjEcT.JsOn'];
   if(process.argv.includes('authorized')){
     if(selectors.some(path=>!fs.existsSync(path)))throw Error('authorized source selectors lost');
   }else{
     if(selectors.slice(0,2).some(path=>fs.existsSync(path)))throw Error('source execution selectors reached provider');
     if(!process.env.CARGO_HOME||fs.existsSync(process.env.CARGO_HOME+'/config.toml'))throw Error('inherited cargo configuration admitted');
     if(process.env.RUSTC_WRAPPER||process.env.RUSTC_WORKSPACE_WRAPPER)throw Error('inherited compiler wrapper admitted');
   }
   if(!fs.existsSync('rust-project.json.backup'))throw Error('ordinary similarly named captured data lost');
   reply(m,{capabilities:{positionEncoding:'utf-16',textDocumentSync:1,codeActionProvider:{codeActionKinds:selectors.filter(path=>fs.existsSync(path))}}});return;
 }
 if(m.method==='shutdown'){reply(m,null);return;}if(m.method==='exit')process.exit(0);
 if(m.id!==undefined)reply(m,null);
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const n=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+n)return;const m=JSON.parse(buffer.subarray(end+4,end+4+n));buffer=buffer.subarray(end+4+n);handle(m);}});
