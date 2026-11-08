'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const ts=require('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence/node_modules/typescript/lib/typescript.js');
const out=process.argv[2];fs.mkdirSync(out,{recursive:false});
const assert=(x,m)=>{if(!x)throw Error(m);};
function expand(source){
 const sf=ts.createSourceFile('main.js',source,ts.ScriptTarget.ES2022,true,ts.ScriptKind.JS);
 const selected=sf.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name.text==='helper');assert(selected,'function');
 const calls=[];function find(n){if(n===selected)return;if(ts.isCallExpression(n)&&ts.isIdentifier(n.expression)&&n.expression.text==='helper')calls.push(n);ts.forEachChild(n,find);}find(sf);
 let serial=1;const used=new Set();function ids(n){if(ts.isIdentifier(n))used.add(n.text);ts.forEachChild(n,ids);}ids(sf);
 const fresh=()=>{while(used.has('__ci_inline_'+serial))serial++;const n='__ci_inline_'+serial++;used.add(n);return n;};
 const edits=[],frames=[];
 for(const call of calls){
  let statement=call;while(!ts.isStatement(statement))statement=statement.parent;
  assert(ts.isVariableStatement(statement)&&statement.declarationList.declarations.length===1&&statement.declarationList.declarations[0].initializer===call,'prototype statement shape');
  assert(!call.arguments.some(ts.isSpreadElement),'spread');assert(selected.parameters.every(p=>ts.isIdentifier(p.name)&&!p.initializer&&!p.dotDotDotToken),'prototype parameters');
  const result=fresh(),label=fresh(),args=call.arguments.map(()=>fresh()),changes=[];
  function returns(n){if(n!==selected.body&&ts.isFunctionLike(n))return;if(ts.isReturnStatement(n)){const start=n.getStart(sf),gap=n.expression?source.slice(start+6,n.expression.getStart(sf)):'',expr=n.expression?'('+gap+n.expression.getText(sf)+')':'void 0',tail=source.slice(n.expression?n.expression.end:start+6,n.end);changes.push({start,end:n.end,text:'{ '+result+' = '+expr+tail+'; break '+label+'; }'});return;}ts.forEachChild(n,returns);}returns(selected.body);
  let body=selected.body.getText(sf);for(const e of changes.sort((a,b)=>b.start-a.start))body=body.slice(0,e.start-selected.body.getStart(sf))+e.text+body.slice(e.end-selected.body.getStart(sf));
  const prelude='let '+result+';\n{\n'+args.map((n,i)=>'const '+n+' = ('+call.arguments[i].getText(sf)+');\n').join('')+'{\n'+selected.parameters.map((p,i)=>'let '+p.name.text+' = '+(args[i]||'void 0')+';\n').join('')+label+': '+body+'\n}\n}\n';
  const replacement=prelude+source.slice(statement.getStart(sf),call.getStart(sf))+result+source.slice(call.end,statement.end);
  edits.push({start:statement.getStart(sf),end:statement.end,text:replacement});frames.push({statement:[statement.getStart(sf),statement.end],call:[call.getStart(sf),call.end],result,label,args,returns:changes.length});
 }
 edits.push({start:selected.getStart(sf),end:selected.end,text:''});let result=source;for(const e of edits.sort((a,b)=>b.start-a.start))result=result.slice(0,e.start)+e.text+result.slice(e.end);
 return{source:result,frames};
}
const cases=[
 {name:'argument_order_and_parameter_scope',source:`const events=[];let n=9;function arg(x){events.push(x);return x;}function helper(n,m){events.push(n,m);n++;return [n,m];}const result=helper(arg(n),arg(2));globalThis.observed={result,events,n};`},
 {name:'all_arguments_before_parameters',source:`let n=1;const events=[];function helper(n){events.push(n);return ()=>n;}const result=helper((events.push('first'),n), (events.push('extra'),n=3));globalThis.observed={value:result(),events,n};`},
 {name:'finally_return_overrides',source:`const events=[];function helper(n){try{events.push('try');return n+1;}finally{events.push('finally');return n+2;}}const result=helper(3);globalThis.observed={result,events};`},
 {name:'finally_keeps_pending_value',source:`let n=1;function helper(x){try{return n;}finally{n=7;}}const result=helper(3);globalThis.observed={result,n};`},
 {name:'nested_function_returns_stay_local',source:`function helper(n){const inner=()=>{return n+1;};return inner();}const result=helper(3);globalThis.observed=result;`},
 {name:'multiple_calls_and_early_exit',source:`const events=[];function helper(n){if(n){events.push(n);return n;}events.push(0);}const a=helper(2);const b=helper(0);globalThis.observed={a,b,events};`},
 {name:'return_comma_and_comments',source:`const events=[];function helper(n){return /* begin */ events.push(n),n+2 /* end */;}const result=helper(3);globalThis.observed={result,events};`},
];
const results=[];for(const item of cases){const changed=expand(item.source),observe=s=>{const context={};vm.runInNewContext(s,context,{timeout:1000});return JSON.stringify(context.observed);};const before=observe(item.source),after=observe(changed.source);assert(before===after,item.name+': '+before+' != '+after);fs.writeFileSync(path.join(out,item.name+'.before.js'),item.source);fs.writeFileSync(path.join(out,item.name+'.after.js'),changed.source);results.push({name:item.name,before,after,frames:changed.frames});}
fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({kind:'exploratory_owned_function_body_runtime_prototype',compiler:ts.version,compilerSha256:crypto.createHash('sha256').update(fs.readFileSync(require.resolve('C:/Users/Lomi/Documents/NN-Developed/Yoke-CodeIntelligence/node_modules/typescript/lib/typescript.js'))).digest('hex'),cases:results,nativeAdmission:false,productionIntegration:false,fullGoalComplete:false},null,2)+'\n');console.log(JSON.stringify({cases:results.length,allPassed:true,nativeAdmission:false,productionIntegration:false}));
