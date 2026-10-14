'use strict';
// Actual VS Code extension-host integration, using an isolated user-data profile.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),{execFileSync}=require('node:child_process');
const vscode=require('vscode');
exports.run=async function(){
 const executable=process.env.YOKE_VSCODE_NATIVE,resultPath=process.env.YOKE_VSCODE_RESULT;
 const bundled=process.env.YOKE_VSCODE_BUNDLED==='1';
 assert(executable&&resultPath,'Test launcher must provide isolated runtime/result paths');
 const folder=vscode.workspace.workspaceFolders?.[0];assert(folder,'Local test folder required');
 const source=path.join(folder.uri.fsPath,'main.rs'),before=fs.readFileSync(source,'utf8');
 const config=vscode.workspace.getConfiguration('codeIntelligence');
 if(bundled){
  assert.equal(config.inspect('runtime').globalValue,undefined,'Bundled fixture must use the runtime default');
  assert.equal(config.inspect('nativeExecutable').globalValue,undefined,'Bundled fixture must not override executable authority');
 }else{
  await config.update('runtime','rust',vscode.ConfigurationTarget.Global);
  await config.update('nativeExecutable',executable,vscode.ConfigurationTarget.Global);
 }
 // Workspace-scoped executable settings must never override user authority.
 const workspaceSettings=JSON.parse(fs.readFileSync(path.join(folder.uri.fsPath,'.vscode','settings.json'),'utf8'));
 assert.equal(path.resolve(workspaceSettings['codeIntelligence.nativeExecutable']).toLowerCase(),path.join(folder.uri.fsPath,'must-not-run.exe').toLowerCase());
 const extension=vscode.extensions.getExtension('hecer.code-intelligence-local');assert(extension,'Development extension not loaded');
 if(bundled)assert.equal(path.resolve(executable).toLowerCase(),path.join(extension.extensionPath,'runtime','native','yoke.exe').toLowerCase());
 await extension.activate();
 const doc=await vscode.workspace.openTextDocument(source);await vscode.window.showTextDocument(doc);
 fs.writeFileSync(path.join(folder.uri.fsPath,'.env'),'TEST_FIXTURE=excluded\n');
 await vscode.workspace.openTextDocument(path.join(folder.uri.fsPath,'.env'));
 const edit=new vscode.WorkspaceEdit();edit.insert(doc.uri,new vscode.Position(0,0),'// Unsaved editor fixture\nfn editor_unsaved_symbol() {}\n');assert(await vscode.workspace.applyEdit(edit));
 assert(doc.isDirty,'Fixture must remain unsaved');
 await vscode.commands.executeCommand('codeIntelligence.start');
 await vscode.commands.executeCommand('codeIntelligence.snapshot');
 const rendered=vscode.workspace.textDocuments.filter(document=>document.isUntitled&&document.languageId==='json').map(document=>{try{return JSON.parse(document.getText());}catch{return null;}}).find(value=>value?.kind==='overlay');
 assert(rendered,'Actual extension command did not render an overlay snapshot');
 assert(rendered.editorBufferFailures?.some(item=>item.path==='.env'),'Excluded buffer must be visible without failing startup');
 assert(!rendered.files.some(file=>file.path==='.env'),'Protected source entered overlay');
 const file=rendered.files.find(file=>file.path==='main.rs');assert(file);
 assert.equal(file.hash,crypto.createHash('sha256').update(doc.getText(),'utf8').digest('hex'));
 const found=JSON.parse(execFileSync(executable,['--root',folder.uri.fsPath,'--snapshot',rendered.id,'find','editor_unsaved_symbol'],{encoding:'utf8',timeout:10000,windowsHide:true}));
 assert.equal(found.status,'ok');assert(found.data.items.some(item=>item.name==='editor_unsaved_symbol'));
 assert.equal(fs.readFileSync(source,'utf8'),before,'Extension wrote original source');
 await vscode.commands.executeCommand('codeIntelligence.stop');
 fs.writeFileSync(resultPath,JSON.stringify({kind:'actual-vscode-extension-host',passed:true,runtime:'rust',bundledRuntime:bundled,vscodeVersion:vscode.version,unsavedSourceBound:true,workspaceExecutableIgnored:true,protectedOpenBufferExcluded:true,savedSourceUnchanged:true})+'\n');
};
