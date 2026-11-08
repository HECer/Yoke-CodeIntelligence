// Independent DAP protocol fixture; no debuggee execution occurs here.
import fs from 'node:fs';import path from 'node:path';
const [log,mode]=process.argv.slice(2);let buffer=Buffer.alloc(0),seq=0,program,launch;
function send(message){const body=Buffer.from(JSON.stringify({seq:++seq,...message}));process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);process.stdout.write(body);}
function reply(m,body={}){send({type:'response',request_seq:m.seq,success:true,command:m.command,body});}
function event(event,body={}){send({type:'event',event,body});}
function handle(m){fs.appendFileSync(log,JSON.stringify({type:m.type,command:m.command,seq:m.seq,pid:process.pid})+'\n');
 if(m.type==='response')return;
 if(m.command==='initialize'){reply(m,{supportsConfigurationDoneRequest:mode!=='no-configuration-phase'});return;}
 if(m.command==='launch'){program=m.arguments.program;launch=m;
   if(mode==='reverse-run'){send({type:'request',command:'runInTerminal',arguments:{args:['unapproved.exe']}});return;}
   if(mode==='no-configuration-phase'){reply(m);event('stopped',{reason:'entry',threadId:1});return;}
   event('initialized');return;
 }
 if(m.command==='setBreakpoints'){reply(m,{breakpoints:m.arguments.breakpoints.map(b=>({verified:true,line:b.line}))});return;}
 if(m.command==='configurationDone'){reply(m);reply(launch);if(mode==='changed-source')fs.writeFileSync(program,'changed bytes\n');if(mode!=='not-stopped')event('stopped',{reason:'breakpoint',...(mode==='no-thread-id'?{}:{threadId:1}),allThreadsStopped:true});return;}
 if(m.command==='threads'){reply(m,{threads:[{id:1,name:'owned'}]});return;}
 if(m.command==='stackTrace'){reply(m,{stackFrames:[{id:7,name:'actual',source:{path:mode==='foreign-frame'?path.resolve(program,'../../outside.py'):program},line:2,column:1}],totalFrames:1});return;}
 if(m.command==='evaluate'){if(m.arguments.frameId!==7||m.arguments.context!=='repl'){process.exit(3);}if(mode==='stalled-evaluate')return;reply(m,{result:m.arguments.expression==='value + 2'?'42':'40',variablesReference:0});return;}
 if(m.command==='disconnect'){event('terminated');reply(m);process.exit(0);}
 reply(m);
}
process.stdin.on('data',chunk=>{buffer=Buffer.concat([buffer,chunk]);while(true){const end=buffer.indexOf('\r\n\r\n');if(end<0)return;const n=Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);if(buffer.length<end+4+n)return;const m=JSON.parse(buffer.subarray(end+4,end+4+n));buffer=buffer.subarray(end+4+n);handle(m);}});
