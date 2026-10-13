'use strict';
const vscode=require('vscode'),{spawn}=require('node:child_process'),path=require('node:path');
const instances=new Map(),pendingStarts=new Map();let subscriptions=[];
function userSetting(key,fallback){return vscode.workspace.getConfiguration('codeIntelligence').inspect(key)?.globalValue??fallback;}
function projectFor(doc){if(doc.uri.scheme!=='file')return;return vscode.workspace.getWorkspaceFolder(doc.uri);}
function relative(folder,doc){const r=path.relative(folder.uri.fsPath,doc.uri.fsPath).replaceAll('\\','/');if(!r||r.startsWith('../')||path.isAbsolute(r))throw Error('Document is outside the workspace');return r;}
async function post(instance,data){const r=await fetch(instance.endpoint,{method:'POST',headers:{authorization:'Bearer '+instance.token,'Content-Type':'application/json'},body:JSON.stringify(data),signal:AbortSignal.timeout(120000),redirect:'error'});const value=await r.json();if(!r.ok||!value.ok)throw Error(value.error?.code??'Editor request failed');return value.data;}
function enqueue(instance,fn){const next=instance.queue.catch(()=>{}).then(fn);instance.queue=next;return next;}
async function start(folder,context){
 if(!folder)throw Error('Open a local workspace first');const key=folder.uri.toString();if(pendingStarts.has(key))return pendingStarts.get(key);
 const promise=startInstance(folder,context);pendingStarts.set(key,promise);try{return await promise;}finally{pendingStarts.delete(key);}
}
async function startInstance(folder,context){
 if(!vscode.workspace.isTrusted)throw Error('Workspace trust is required');if(!folder||folder.uri.scheme!=='file')throw Error('A local filesystem workspace is required');
 if(instances.has(folder.uri.toString()))return instances.get(folder.uri.toString());
 const node=userSetting('nodeExecutable','node'),cli=userSetting('cliPath','')||path.join(context.extensionPath,'runtime','dist','cli.js');
 const child=spawn(node,[cli,'editor','--root',folder.uri.fsPath,'--timeout','120000'],{shell:false,windowsHide:true,stdio:['ignore','pipe','pipe']});
 let output='',errorText='';child.stderr.on('data',d=>{errorText=(errorText+d.toString()).slice(-1000);});
 const connection=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{child.kill();reject(Error('Editor bridge startup timed out'));},20000);let finished=false;
 const finish=(err,v)=>{if(finished)return;finished=true;clearTimeout(timer);err?reject(err):resolve(v);};
 child.on('error',()=>finish(Error('Cannot start configured Node executable')));child.once('exit',()=>finish(Error('Editor bridge exited before startup')));
 child.stdout.on('data',d=>{output+=d.toString();if(output.length>16000){child.kill();finish(Error('Invalid bridge handshake'));return;}try{const v=JSON.parse(output);const u=new URL(v.endpoint);if(u.protocol!=='http:'||u.hostname!=='127.0.0.1'||u.pathname!=='/editor'||!/^\w{64}$/.test(v.token))throw Error('Invalid endpoint');finish(null,v);}catch{}});
 });
 const instance={...connection,child,folder,queue:Promise.resolve(),sessionId:null};
 try{instance.sessionId=(await post(instance,{operation:'open'})).id;instances.set(folder.uri.toString(),instance);for(const doc of vscode.workspace.textDocuments)if(projectFor(doc)?.uri.toString()===folder.uri.toString())await post(instance,{operation:'buffer',sessionId:instance.sessionId,buffer:{path:relative(folder,doc),version:doc.version,text:doc.getText()}});}
 catch(e){child.kill();throw e;}
 child.once('exit',()=>{instances.delete(folder.uri.toString());});return instance;
}
async function render(value){const doc=await vscode.workspace.openTextDocument({content:JSON.stringify(value,null,2),language:'json'});await vscode.window.showTextDocument(doc,{preview:true});}
async function stop(){for(const instance of instances.values()){try{await post(instance,{operation:'close',sessionId:instance.sessionId});}catch{}instance.child.kill();}instances.clear();}
function activate(context){
 const guard=fn=>async(...args)=>{try{return await fn(...args);}catch(e){vscode.window.showErrorMessage('Code Intelligence: '+e.message);}};
 const activeFolder=()=>{const doc=vscode.window.activeTextEditor?.document;return doc?projectFor(doc):vscode.workspace.workspaceFolders?.[0];};
 const handleDoc=(doc,closed=false)=>{const folder=projectFor(doc),i=folder&&instances.get(folder.uri.toString());if(!i)return;const request=closed?{operation:'closeFile',sessionId:i.sessionId,path:relative(folder,doc),version:doc.version}:{operation:'buffer',sessionId:i.sessionId,buffer:{path:relative(folder,doc),version:doc.version,text:doc.getText()}};void enqueue(i,()=>post(i,request)).catch(e=>vscode.window.showWarningMessage('Code Intelligence buffer update: '+e.message));};
 subscriptions=[
 vscode.commands.registerCommand('codeIntelligence.start',guard(async()=>{await start(activeFolder(),context);vscode.window.showInformationMessage('Code Intelligence editor bridge started. Source writes remain disabled.');})),
 vscode.commands.registerCommand('codeIntelligence.context',guard(async()=>{const query=await vscode.window.showInputBox({prompt:'Find code context across saved and unsaved source',ignoreFocusOut:true});if(!query)return;const i=await start(activeFolder(),context);await render(await enqueue(i,()=>post(i,{operation:'tool',sessionId:i.sessionId,tool:'ci_context',arguments:{query,maxBytes:32000}})));})),
 vscode.commands.registerCommand('codeIntelligence.snapshot',guard(async()=>{const i=await start(activeFolder(),context);await render(await enqueue(i,()=>post(i,{operation:'capture',sessionId:i.sessionId})));})),
 vscode.commands.registerCommand('codeIntelligence.stop',guard(stop)),
 vscode.workspace.onDidOpenTextDocument(doc=>handleDoc(doc)),vscode.workspace.onDidChangeTextDocument(e=>handleDoc(e.document)),vscode.workspace.onDidCloseTextDocument(doc=>handleDoc(doc,true)),
 vscode.workspace.onDidSaveTextDocument(doc=>{const folder=projectFor(doc),i=folder&&instances.get(folder.uri.toString());if(i)void enqueue(i,()=>post(i,{operation:'capture',sessionId:i.sessionId})).catch(e=>vscode.window.showWarningMessage('Code Intelligence disk conflict: '+e.message));})];
 context.subscriptions.push(...subscriptions,{dispose:()=>{for(const i of instances.values())i.child.kill();instances.clear();}});
}
module.exports={activate,deactivate:stop};
