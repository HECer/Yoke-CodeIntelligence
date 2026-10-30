'use strict';
// Type inference uses original declarations in an unreachable witness block.
// Only fixed typeof syntax and fresh names are generated; initializers stay inert.
function bodyVarTypePlan(ts,vars,indices,sf,text,assert,tick){
 const sites=[],byDeclaration=new Map(),groupSites=new Map();
 const declarationOf=id=>{let at=id;for(let depth=0;at&&depth<2000;depth++,at=at.parent){tick();if(ts.isVariableDeclaration(at))return at;}assert(false,'Body var type declaration unavailable');};
 for(const i of indices){
  const g=vars.groups[i];assert(g&&!g.annotation&&!g.parameter&&!g.function,'Body var type group unsupported');
  const declaration=g.ids.map(declarationOf).find(d=>d.initializer||d.type||ts.isForInStatement(d.parent.parent)||ts.isForOfStatement(d.parent.parent));
  assert(declaration,'Body var source type witness unavailable');
  let site=byDeclaration.get(declaration);
  if(!site){const list=declaration.parent,parent=list.parent;assert(ts.isVariableDeclarationList(list),'Body var type list unavailable');
   const loop=(ts.isForInStatement(parent)||ts.isForOfStatement(parent))?parent:undefined;
   let placement=ts.isVariableStatement(parent)||ts.isForStatement(parent)||loop?parent:undefined;
   assert(placement,'Body var type statement unavailable');
   let labels=0;while(ts.isLabeledStatement(placement.parent)&&placement.parent.statement===placement){tick();assert(++labels<2000,'Body var type label capacity');placement=placement.parent;}
   site={declaration,placement,loop};byDeclaration.set(declaration,site);sites.push(site);
  }
  groupSites.set(i,site);
 }
 sites.sort((a,b)=>a.declaration.getStart(sf)-b.declaration.getStart(sf));
 let count=0;const bound=name=>{tick();if(ts.isIdentifier(name))return[name];const result=[];for(const e of name.elements){if(ts.isOmittedExpression(e))continue;result.push(...bound(e.name));}return result;};
 for(const site of sites){site.ids=bound(site.declaration.name);site.bindings=new Map();for(const id of site.ids)if(!site.bindings.has(id.text))site.bindings.set(id.text,count++);}
 assert(count<=2000&&sites.length<=512,'Body var type witness capacity');
 const seedForGroup=new Map([...groupSites].map(([i,s])=>[i,s.bindings.get(vars.groups[i].name)]));
 function nameFor(i,names){const at=seedForGroup.get(i);return at===undefined?undefined:names.varSeeds[at];}
 function changes(names,line){assert((names.varSeeds||[]).length===count,'Body var type witness name count');const placements=new Map();
  for(const site of sites){const d=site.declaration;let pattern='',cursor=d.name.getStart(sf);
   for(const id of site.ids){const owner=id.parent,shorthand=ts.isBindingElement(owner)&&owner.name===id&&!owner.dotDotDotToken&&!owner.propertyName&&ts.isObjectBindingPattern(owner.parent);pattern+=text.slice(cursor,id.getStart(sf))+(shorthand?JSON.stringify(id.text)+': ':'')+names.varSeeds[site.bindings.get(id.text)];cursor=id.end;}
   pattern+=text.slice(cursor,d.name.end);
   const reads=[...site.bindings.values()].map(i=>'void '+names.varSeeds[i]+';').join('');let witness;
   if(site.loop)witness='for (var '+pattern+(ts.isForOfStatement(site.loop)?' of (':' in (')+site.loop.expression.getText(sf)+')) {'+reads+'}';
   else witness='var '+pattern+text.slice(d.name.end,d.initializer?d.initializer.end:d.end)+(d.initializer?'':' = void 0 as any')+';'+reads;
   const existing=placements.get(site.placement)||[];existing.push(witness);placements.set(site.placement,existing);
  }
  const result=[];for(const [placement,witnesses]of [...placements].sort(([a],[b])=>a.getStart(sf)-b.getStart(sf))){result.push({start:placement.getStart(sf),end:placement.getStart(sf),text:'{'+line+'if (void 0 !== void 0) {'+line+witnesses.join(line)+line+'}'+line});result.push({start:placement.end,end:placement.end,text:line+'}'});}
  return result;
 }
 return{count,nameFor,changes};
}
