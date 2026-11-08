const ts=require(process.argv[2]),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),vm=require('node:vm');const dir=fs.mkdtempSync(path.join(os.tmpdir(),'ci-return-owned-'));const cases=[
'let result: ({run:(n:number)=>number}) | undefined; result=({run:n=>n+1});export const observed=result.run(3);',
'let result: ((n:number)=>number)|undefined; result=(void 0,n=>n+1);export const observed=[result(3),result.name];',
'let result: ((n:number)=>number)|undefined; result=(0,n=>n+1);export const observed=[result(3),result.name];',
'let result;result=(void 0,function(){});export const observed=result.name;',
'let result;result=(void 0,class{});export const observed=result.name;',
'let result;result=(function(){});export const observed=result.name;'
];const options={strict:true,target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,noEmit:true,allowUnreachableCode:false,allowUnusedLabels:false};const results=[];for(let i=0;i<cases.length;i++){const file=path.join(dir,'main.ts');fs.writeFileSync(file,cases[i]);const p=ts.createProgram([file],options);const d=ts.getPreEmitDiagnostics(p).map(d=>({code:d.code,message:ts.flattenDiagnosticMessageText(d.messageText,' ')}));const c={exports:{}};vm.runInNewContext(ts.transpileModule(cases[i],{compilerOptions:options}).outputText,c);results.push({i,source:cases[i],diagnostics:d,observed:c.exports.observed});}process.stdout.write(JSON.stringify(results,null,2));
