'use strict';
// Source-only var lowering. Compiler declaration and initialization bindings stay separate.
function bodyVarPlan(ts,selected,sf,text,assert,tick){
 const ids=[],changes=[],parameters=new Map(),functions=new Map();
 const bound=name=>{if(ts.isIdentifier(name))return[name];const result=[];for(const e of name.elements){if(ts.isOmittedExpression(e))continue;result.push(...bound(e.name));}return result;};
 for(const p of selected.parameters)for(const id of bound(p.name))parameters.set(id.text,id);
 for(const f of selected.body.statements)if(ts.isFunctionDeclaration(f)&&f.name)functions.set(f.name.text,f.name);
 const collect=n=>{tick();if(n!==selected.body&&(ts.isFunctionLike(n)||ts.isClassDeclaration(n)||ts.isClassExpression(n)))return;
  if(ts.isVariableDeclarationList(n)&&!(n.flags&ts.NodeFlags.BlockScoped)){
   for(const d of n.declarations)for(const id of bound(d.name))ids.push({id,annotation:ts.isIdentifier(d.name)?d.type:undefined});
   if(ts.isForInStatement(n.parent)||ts.isForOfStatement(n.parent)){assert(n.declarations.length===1&&!n.declarations[0].initializer,'Body var loop initializer unsupported');changes.push({start:n.getStart(sf),end:n.getStart(sf)+3,text:''});}
   else{const statement=ts.isVariableStatement(n.parent)?n.parent:n;changes.push({start:statement.getStart(sf),end:statement.end,declaration:n});}
  }
  ts.forEachChild(n,collect);
 };collect(selected.body);ids.sort((a,b)=>a.id.getStart(sf)-b.id.getStart(sf));assert(ids.length<=2000&&changes.length<=2000,'Body var declaration capacity');
 const groups=[],byName=new Map();for(const {id,annotation}of ids){let g=byName.get(id.text);if(!g){g={name:id.text,ids:[],parameter:parameters.get(id.text),function:functions.get(id.text),annotation};byName.set(id.text,g);groups.push(g);}g.ids.push(id);}assert(groups.length<=512,'Body var name capacity');
 const copyGroups=groups.filter(g=>g.parameter&&!g.function);
 function captures(names,line){assert((names.varParameters||[]).length===copyGroups.length,'Body var parameter copy mismatch');return copyGroups.map((g,i)=>'const '+names.varParameters[i]+' = ('+g.parameter.getText(sf)+');'+line).join('');}
 function hoists(names,hints,js,line){let index=0,out='';for(let i=0;i<groups.length;i++){const g=groups[i],hint=hints[i];if(g.function)continue;if(js&&hint)out+='/** @type {'+hint+'} */ ';out+='let '+g.ids[0].getText(sf);if(g.annotation){const owner=g.ids[0].parent,colon=owner.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken);assert(colon,'Body var type colon absent');out+=text.slice(colon.getStart(sf),g.annotation.end);}else if(!js&&hint)out+=': '+hint;if(g.parameter)out+=' = '+names.varParameters[index++];else if(g.annotation||hint)out+=js?' = /** @type {any} */ (void 0)':' = void 0 as any';out+=';'+line;}return out;}

 function render(line){return changes.map(e=>{if(!e.declaration)return e;const list=e.declaration;let out='(',cursor=list.getStart(sf)+3;for(const d of list.declarations){out+=text.slice(cursor,d.name.getStart(sf));const colon=d.type?d.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken):undefined;assert(!d.type||colon,'Body var type colon absent');
   if(d.initializer){const equal=d.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.EqualsToken);assert(equal,'Body var assignment token absent');out+=d.name.getText(sf)+text.slice(d.name.end,colon?colon.getStart(sf):equal.getStart(sf));if(d.type)out+=text.slice(d.type.end,equal.getStart(sf));out+='= ('+(d.type?'(':'')+text.slice(equal.end,d.initializer.end);if(d.type)out+=') as ('+text.slice(colon.end,d.type.end)+')';out+=')';}
   else if(d.type)out+='void '+text.slice(d.name.end,colon.getStart(sf))+'(0 as unknown as ('+text.slice(colon.end,d.type.end)+'))'+text.slice(d.type.end,d.end);
   else out+='void 0'+text.slice(d.name.end,d.end);
   cursor=d.end;
  }out+=')'+text.slice(cursor,e.end);return{start:e.start,end:e.end,text:out};});}
 return{ids:ids.map(v=>v.id),groups,copyGroups,captures,hoists,render};
}
