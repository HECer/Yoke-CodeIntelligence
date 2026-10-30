'use strict';
const ts=require('C:/Users/Lomi/AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules/typescript/lib/typescript.js');
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(process.cwd(),'virtual').replaceAll('\\','/'),abs=n=>root+'/'+n;
const library=path.dirname(ts.getDefaultLibFilePath({target:ts.ScriptTarget.ES2022})).replaceAll('\\','/');
for(const entries of [
 {'source.ts':"import {delta as step} from './dep';\nimport './effect';\nexport function helper(n:number){return n+step;}\nexport const keep=3;\n",'target.ts':'const step=20;\nexport const other=step;\n','consumer.ts':"export {helper as renamed,keep} from './source';\n",'dep.ts':'export const delta=2;\n','effect.ts':'export const observed=1;\n'},
]) {
 const sources=new Map(Object.entries(entries).map(([n,s])=>[abs(n),s]));
 const read=n=>{n=path.resolve(n).replaceAll('\\','/');if(sources.has(n))return sources.get(n);if(path.dirname(n).replaceAll('\\','/')===library&&/^lib(?:\.[a-z0-9.-]+)?\.d\.ts$/i.test(path.basename(n)))return fs.readFileSync(n,'utf8');return undefined;};
 const host={getScriptFileNames:()=>[...sources.keys()],getScriptVersion:()=> '1',getScriptSnapshot:n=>{const s=read(n);return s===undefined?undefined:ts.ScriptSnapshot.fromString(s);},getCompilationSettings:()=>({target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,moduleResolution:ts.ModuleResolutionKind.Bundler,noEmit:true,strict:true}),getCurrentDirectory:()=>root,getDefaultLibFileName:o=>ts.getDefaultLibFilePath(o),readFile:read,fileExists:n=>read(n)!==undefined,directoryExists:n=>[root,library].includes(path.resolve(n).replaceAll('\\','/')),readDirectory:()=>[],getDirectories:()=>[],useCaseSensitiveFileNames:()=>true};
 const service=ts.createLanguageService(host),file=abs('source.ts'),source=entries['source.ts'],program=service.getProgram();
 const sf=program.getSourceFile(file),selected=sf.statements.find(n=>ts.isFunctionDeclaration(n)||ts.isClassDeclaration(n));
 const range={pos:selected.getStart(sf),end:selected.end};
 const prefs={allowTextChangesInNewFiles:true,provideRefactorNotApplicableReason:true};
 console.log(JSON.stringify({entries,diagnostics:ts.getPreEmitDiagnostics(program).map(d=>ts.flattenDiagnosticMessageText(d.messageText,'\n')),range,actions:service.getApplicableRefactors(file,range,prefs,'invoked','refactor.move.file',true)}));
 const edits=service.getEditsForRefactor(file,{newLineCharacter:'\n'},range,'Move to file','Move to file',prefs,{targetFile:abs('target.ts')});
 console.log(JSON.stringify(edits));
 if(edits)for(const change of edits.edits){let s=read(change.fileName);for(const edit of [...change.textChanges].reverse())s=s.slice(0,edit.span.start)+edit.newText+s.slice(edit.span.start+edit.span.length);console.log(JSON.stringify({file:change.fileName,after:s}));sources.set(change.fileName.replaceAll('\\','/'),s);}
 const afterService=ts.createLanguageService(host); console.log(JSON.stringify({afterDiagnostics:ts.getPreEmitDiagnostics(afterService.getProgram()).map(d=>({file:d.file?.fileName,start:d.start,text:ts.flattenDiagnosticMessageText(d.messageText,'\n')}))})); afterService.dispose();service.dispose();
}
