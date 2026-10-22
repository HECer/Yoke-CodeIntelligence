// Own compiler observations over the same immutable program as ordinary analysis.
function compilerHierarchy({input,ts,checker,program,span,source,slash,rel,root,assert}){
 const limits=input.intent;
 for(const [key,max] of [['maxNodes',5000],['maxRelations',20000],['maxPairVisits',200000]])assert(Number.isSafeInteger(limits[key])&&limits[key]>=0&&limits[key]<=max,'Hierarchy capacity contract');
 const nodes=[],relations=[],unavailable=[],unavailableSeen=new Set(),byDecl=new Map(),types=[],calls=[],witnessNodes=new Set(),instances=[],instancesByOwner=new Map(),typesBySymbol=new Map();let ast=0,pairVisits=0,witnessBytes=0;
 const captured=node=>node&&source.has(slash(node.getSourceFile().fileName));
 const isMethod=node=>ts.isMethodDeclaration(node)||ts.isMethodSignature(node);
 const contractSlot=node=>ts.isPropertyDeclaration(node)||ts.isPropertySignature(node)||ts.isGetAccessorDeclaration(node)||ts.isSetAccessorDeclaration(node);
 const oppositeAccessor=(a,b)=>(ts.isGetAccessorDeclaration(a)&&ts.isSetAccessorDeclaration(b))||(ts.isSetAccessorDeclaration(a)&&ts.isGetAccessorDeclaration(b));
 const staticMember=node=>!!(ts.getCombinedModifierFlags(node)&ts.ModifierFlags.Static);
 const privateMember=node=>ts.isPrivateIdentifier(node.name)||!!(ts.getCombinedModifierFlags(node)&ts.ModifierFlags.Private);
 function tick(){assert(++pairVisits<=limits.maxPairVisits,'Hierarchy pair capacity');}
 function addNode(node){
  if(!captured(node))return null;
  if(byDecl.has(node))return byDecl.get(node);
  assert(nodes.length<limits.maxNodes,'Hierarchy node capacity');
  const sf=node.getSourceFile(),name=node.name||(ts.isConstructorDeclaration(node)?node.getChildren(node.getSourceFile()).find(token=>token.kind===ts.SyntaxKind.ConstructorKeyword):undefined);
  const declaration=span(sf,ts.isSourceFile(node)?0:node.getStart(sf),node.getEnd());
  const selection=name?span(sf,name.getStart(sf),name.getEnd()):span(sf,declaration.startByte===0?0:node.getStart(sf),declaration.startByte===0?0:node.getStart(sf));
  const id=nodes.length;nodes.push({kind:ts.SyntaxKind[node.kind],name:name?name.getText(sf):ts.isSourceFile(node)?rel(sf.fileName):'(anonymous)',declaration,selection,static:staticMember(node),hasBody:!!node.body});byDecl.set(node,id);return id;
 }
 function unknown(reason,node){const anchor=span(node.getSourceFile(),node.getStart(),node.getEnd()),key=JSON.stringify([reason,anchor.path,anchor.startByte,anchor.endByte]);if(unavailableSeen.has(key))return;assert(unavailable.length<limits.maxRelations,'Hierarchy coverage capacity');unavailableSeen.add(key);unavailable.push({reason,anchor});}
 function edge(relation,from,to,proof,extra={}){
  if(from===null||to===null){unknown('declaration_outside_capture',proof);return;}
  assert(relations.length<limits.maxRelations,'Hierarchy relation capacity');relations.push({relation,source:from,target:to,proof:span(proof.getSourceFile(),proof.getStart(),proof.getEnd()),...extra});
 }
 for(const sf of program.getSourceFiles()){
  if(!source.has(slash(sf.fileName)))continue;
  addNode(sf);
  function walk(node){
   assert(++ast<=500000,'Hierarchy AST capacity');
   if(ts.isClassDeclaration(node)||ts.isInterfaceDeclaration(node)){
    if(node.name){const symbol=checker.getSymbolAtLocation(node.name);if(symbol){const type=checker.getDeclaredTypeOfSymbol(symbol),item={node,symbol,type,id:addNode(node),generic:!!type.typeParameters?.length};types.push(item);if(!typesBySymbol.has(symbol)||ts.isClassDeclaration(node))typesBySymbol.set(symbol,item);}}
    else unknown('anonymous_type_not_enumerated',node);
   }else if(ts.isClassExpression(node))unknown('class_expression_not_enumerated',node);
   if(ts.isFunctionLike(node)&&!ts.isFunctionTypeNode(node))addNode(node);
   if(ts.isCallExpression(node)||ts.isNewExpression(node)||ts.isTaggedTemplateExpression(node))calls.push(node);
   if(ts.isTypeNode(node)||ts.isExpressionWithTypeArguments(node)||ts.isCallExpression(node)||ts.isNewExpression(node)||ts.isTaggedTemplateExpression(node))witnessNodes.add(node);
   if((ts.isVariableDeclaration(node)||ts.isParameter(node)||ts.isPropertyDeclaration(node)||ts.isPropertySignature(node)||ts.isBindingElement(node))&&node.name)witnessNodes.add(node.name);
   ts.forEachChild(node,walk);
  }walk(sf);
 }
 for(const call of calls){const expression=ts.isTaggedTemplateExpression(call)?call.tag:call.expression;if(ts.isPropertyAccessExpression(expression)||ts.isElementAccessExpression(expression))witnessNodes.add(expression.expression);}
 function display(type,node){const text=checker.typeToString(type,node,ts.TypeFormatFlags.NoTruncation|ts.TypeFormatFlags.UseFullyQualifiedType).replaceAll(root,'[snapshot]');assert(text.length&&Buffer.byteLength(text)<=4096,'Hierarchy type display capacity');return text;}
 for(const node of witnessNodes){
  tick();const observed=checker.getTypeAtLocation(node),branches=[observed],recorded=new Map();
  while(branches.length){
   tick();const effective=branches.pop();if(effective.isUnion()){branches.push(...[...effective.types].reverse());continue;}
   const parts=[effective],seenParts=new Set();
   while(parts.length){
    tick();const part=parts.pop();if(seenParts.has(part))continue;seenParts.add(part);
    if(part.isIntersection()){parts.push(...[...part.types].reverse());continue;}
    if(!(part.flags&ts.TypeFlags.Object)||!(part.objectFlags&ts.ObjectFlags.Reference))continue;
    const item=typesBySymbol.get(part.getSymbol());if(!item||!item.generic||!ts.isClassDeclaration(item.node))continue;
    const args=checker.getTypeArguments(part);if(!args.length)continue;
    const seen=recorded.get(effective)||new Set();if(seen.has(part))continue;seen.add(part);recorded.set(effective,seen);
    assert(instances.length<limits.maxNodes&&args.length<=limits.maxNodes,'Hierarchy instance witness capacity');
    const anchor=span(node.getSourceFile(),node.getStart(),node.getEnd());
    const raw={owner:item.id,typeDisplay:display(effective,node),classTypeDisplay:display(part,node),observedTypeDisplay:display(observed,node),arguments:args.map(arg=>{tick();assert(Number.isSafeInteger(arg.flags)&&arg.flags>=0&&arg.flags<=4294967295,'Hierarchy type flags');return{display:display(arg,node),flags:arg.flags};}),anchor,syntaxKind:ts.SyntaxKind[node.kind]};
    witnessBytes+=Buffer.byteLength(JSON.stringify(raw));assert(witnessBytes<=4000000,'Hierarchy instance witness byte capacity');
    const id=instances.length;instances.push(raw);const list=instancesByOwner.get(item.id)||[];list.push({id,type:effective});instancesByOwner.set(item.id,list);
   }
  }
 }
 for(const item of types)if(item.generic&&ts.isClassDeclaration(item.node))unknown('unobserved_generic_instantiations_not_enumerated',item.node);
 for(const item of types){
  for(const member of item.node.members){
   if(isMethod(member))edge('declaresMember',item.id,addNode(member),member.name);
   else if(ts.isConstructorDeclaration(member))edge('declaresMember',item.id,addNode(member),member);
   else if(member.name&&!contractSlot(member))unknown('non_method_member_not_override_enumerated',member);
  }
  for(const clause of item.node.heritageClauses||[])for(const heritage of clause.types){
   tick();const baseType=checker.getTypeAtLocation(heritage);let symbol=baseType.getSymbol()||baseType.aliasSymbol;
   if(symbol?.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
   const declarations=(symbol?.declarations||[]).filter(d=>ts.isClassDeclaration(d)||ts.isInterfaceDeclaration(d));
   if(!declarations.length)unknown('heritage_type_not_captured_declaration',heritage);
   const implementing=clause.token===ts.SyntaxKind.ImplementsKeyword;
   for(const base of declarations)edge(implementing?'implements':'extends',item.id,addNode(base),heritage);
   const capturedHeritage=declarations.some(captured);
   for(const member of item.node.members){
    if(!isMethod(member)&&!contractSlot(member))continue;
    if(privateMember(member)){unknown('private_member_not_override_enumerated',member);continue;}
    const own=checker.getSymbolAtLocation(member.name);if(!own)continue;
    const baseObject=staticMember(member)&&symbol?checker.getTypeOfSymbolAtLocation(symbol,heritage):baseType;
    const inherited=checker.getPropertyOfType(baseObject,own.getName());
    for(const target of inherited?.declarations||[]){
     tick();if((!isMethod(target)&&!contractSlot(target))||staticMember(target)!==staticMember(member)||privateMember(target)||oppositeAccessor(member,target))continue;
     const propertyContract=!isMethod(member)||!isMethod(target),interfaceContract=implementing||ts.isInterfaceDeclaration(target.parent);
     if(propertyContract&&!capturedHeritage){unknown('property_contract_heritage_not_captured',heritage);continue;}
     edge(propertyContract?(interfaceContract?'implementsProperty':'overridesProperty'):(interfaceContract?'implementsMember':'overrides'),addNode(member),addNode(target),heritage,{owner:item.id});
    }
   }
  }
 }
 for(const [declaration,id] of [...byDecl]){
  if((isMethod(declaration)||ts.isFunctionDeclaration(declaration))&&!declaration.body&&declaration.name){
   const symbol=checker.getSymbolAtLocation(declaration.name);
   for(const implementation of symbol?.declarations||[]){tick();if(implementation!==declaration&&implementation.body&&captured(implementation))edge('signatureImplementation',id,addNode(implementation),declaration.name);}
  }
  if(ts.isConstructorDeclaration(declaration)&&!declaration.body&&declaration.parent.name){
   const symbol=checker.getSymbolAtLocation(declaration.parent.name)?.members?.get(ts.InternalSymbolName.Constructor);
   for(const implementation of symbol?.declarations||[]){tick();if(ts.isConstructorDeclaration(implementation)&&implementation.body&&captured(implementation))edge('signatureImplementation',id,addNode(implementation),declaration,{owner:addNode(declaration.parent)});}
  }
 }
 function fixedClassReference(expression){
  while(ts.isParenthesizedExpression(expression)||ts.isNonNullExpression(expression)||ts.isAsExpression(expression)||ts.isTypeAssertionExpression(expression)||ts.isSatisfiesExpression(expression))expression=expression.expression;
  if(!ts.isIdentifier(expression)&&!ts.isPropertyAccessExpression(expression))return false;
  let symbol=checker.getSymbolAtLocation(expression);if(symbol?.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
  return !!(symbol?.flags&ts.SymbolFlags.Class);
 }
 function receiverType(expression,call){
  const type=checker.getNonNullableType(checker.getTypeAtLocation(expression));
  if(type.flags&(ts.TypeFlags.Any|ts.TypeFlags.Unknown|ts.TypeFlags.TypeParameter)){unknown('receiver_type_not_enumerated',call);return null;}return type;
 }
 for(const call of calls){
  const signature=checker.getResolvedSignature(call),target=signature?.declaration;
  let caller=call.parent;while(caller&&!ts.isFunctionLike(caller)&&!ts.isSourceFile(caller))caller=caller.parent;
  const constructing=ts.isNewExpression(call),tagged=ts.isTaggedTemplateExpression(call),expression=tagged?call.tag:call.expression;
  const invocationKind=constructing?'new':tagged?'taggedTemplate':expression.kind===ts.SyntaxKind.SuperKeyword?'super':'call';
  const conditional=!!(ts.isOptionalChain(call)||ts.isOptionalChain(expression)),from=addNode(caller),to=target&&captured(target)?addNode(target):null;
  if(to!==null)edge('calls',from,to,call,{invocationKind,conditional});
  else unknown(constructing&&signature&&!target?'construct_signature_has_no_source_declaration':'call_target_not_captured_declaration',call);
  if(constructing){
   const seen=new Set(),pending=[checker.getTypeAtLocation(call)];let resultTypes=0;
   while(pending.length){
    tick();const type=pending.pop();if(seen.has(type))continue;seen.add(type);
    if(type.isUnionOrIntersection()){pending.push(...type.types);continue;}
    let symbol=type.getSymbol()||type.aliasSymbol;if(symbol?.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
    for(const declaration of symbol?.declarations||[]){tick();if(!ts.isClassDeclaration(declaration)||!captured(declaration))continue;resultTypes++;edge('constructResult',from,addNode(declaration),call,{resolvedTarget:to,basis:'compiler_construct_result_type',invocationKind,conditional:false});}
   }
   if(!resultTypes)unknown('construct_result_not_captured_class_type',call);
   if(fixedClassReference(expression))continue;
   const receiver=receiverType(expression,call);if(!receiver)continue;
   for(const candidate of types){
    tick();if(!ts.isClassDeclaration(candidate.node))continue;
    const type=checker.getTypeOfSymbolAtLocation(candidate.symbol,expression);if(!checker.isTypeAssignableTo(type,receiver))continue;
    const signatures=checker.getSignaturesOfType(type,ts.SignatureKind.Construct);
    if(signatures.length)edge('possibleConstructValue',from,candidate.id,call,{resolvedTarget:to,candidateOwner:candidate.id,basis:'compiler_constructor_type_assignability',invocationKind,conditional:false});
    const boundTargets=new Set();
    for(const candidateSignature of signatures){
     tick();const declaration=candidateSignature.declaration;
     if(declaration&&!captured(declaration)){unknown('candidate_constructor_outside_capture',candidate.node);continue;}
     const symbol=declaration&&ts.isConstructorDeclaration(declaration)&&declaration.parent.name?checker.getSymbolAtLocation(declaration.parent.name)?.members?.get(ts.InternalSymbolName.Constructor):undefined;
     const implementation=symbol?.declarations?.find(d=>ts.isConstructorDeclaration(d)&&d.body&&captured(d));
     if(!implementation)unknown(declaration?'candidate_constructor_body_not_captured':'candidate_constructor_has_no_source_declaration',candidate.node);
     const bound=implementation?addNode(implementation):candidate.id;
     if(boundTargets.has(bound))continue;boundTargets.add(bound);
     edge('possibleConstruct',from,bound,call,{resolvedTarget:to,candidateOwner:candidate.id,basis:'compiler_constructor_type_assignability',invocationKind,conditional:false});
    }
   }
   continue;
  }
  if(to===null)continue;
  if(!isMethod(target)||privateMember(target))continue;
  if(!ts.isPropertyAccessExpression(expression)&&!ts.isElementAccessExpression(expression))continue;
  if(expression.expression.kind===ts.SyntaxKind.SuperKeyword)continue;
  if(fixedClassReference(expression.expression))continue;
  const receiver=receiverType(expression.expression,call);if(!receiver)continue;
  const name=checker.getSymbolAtLocation(target.name)?.getName();if(!name){unknown('method_symbol_unavailable',call);continue;}
  for(const candidate of types){
   tick();if(!ts.isClassDeclaration(candidate.node))continue;
   const variants=candidate.generic?(instancesByOwner.get(candidate.id)||[]).map(w=>[false,w.type,'compiler_captured_instance_assignability',w.id]):[[false,candidate.type,'compiler_type_assignability',undefined]];
   variants.push([true,checker.getTypeOfSymbolAtLocation(candidate.symbol,expression),'compiler_constructor_type_assignability',undefined]);
   for(const [staticSide,type,basis,candidateInstance] of variants){
    tick();
    if(!checker.isTypeAssignableTo(type,receiver))continue;
    const property=checker.getPropertyOfType(type,name);
    for(const declaration of property?.declarations||[]){
     tick();if(!ts.isMethodDeclaration(declaration)||!declaration.body||staticMember(declaration)!==staticSide||privateMember(declaration)||(declaration===target&&candidateInstance===undefined))continue;
     edge('possibleDispatch',from,addNode(declaration),call,{resolvedTarget:to,candidateOwner:candidate.id,basis,invocationKind,conditional,...(candidateInstance===undefined?{}:{candidateInstance})});
    }
   }
  }
 }
 compilerProperties({ts,checker,program,source,slash,types,instancesByOwner,addNode,edge,unknown,tick,captured,staticMember,privateMember,fixedClassReference});
 return{schema:1,nodes,instances,relations,unavailable,pairVisits,astNodes:ast,complete:false,runtimeDispatchEstablished:false};
}
