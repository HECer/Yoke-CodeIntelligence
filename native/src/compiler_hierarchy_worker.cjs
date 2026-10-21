// Own compiler observations over the same immutable program as ordinary analysis.
function compilerHierarchy({input,ts,checker,program,span,source,slash,rel,assert}){
 const limits=input.intent;
 for(const [key,max] of [['maxNodes',5000],['maxRelations',20000],['maxPairVisits',200000]])assert(Number.isSafeInteger(limits[key])&&limits[key]>=0&&limits[key]<=max,'Hierarchy capacity contract');
 const nodes=[],relations=[],unavailable=[],unavailableSeen=new Set(),byDecl=new Map(),types=[],calls=[];let ast=0,pairVisits=0;
 const captured=node=>node&&source.has(slash(node.getSourceFile().fileName));
 const isMethod=node=>ts.isMethodDeclaration(node)||ts.isMethodSignature(node);
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
    if(node.name){const symbol=checker.getSymbolAtLocation(node.name);if(symbol){const type=checker.getDeclaredTypeOfSymbol(symbol);types.push({node,symbol,type,id:addNode(node)});}}
    else unknown('anonymous_type_not_enumerated',node);
   }else if(ts.isClassExpression(node))unknown('class_expression_not_enumerated',node);
   if(ts.isFunctionLike(node)&&!ts.isFunctionTypeNode(node))addNode(node);
   if(ts.isCallExpression(node)||ts.isNewExpression(node)||ts.isTaggedTemplateExpression(node))calls.push(node);
   ts.forEachChild(node,walk);
  }walk(sf);
 }
 for(const item of types){
  for(const member of item.node.members){
   if(isMethod(member))edge('declaresMember',item.id,addNode(member),member.name);
   else if(ts.isConstructorDeclaration(member))edge('declaresMember',item.id,addNode(member),member);
   else if(member.name)unknown('non_method_member_not_override_enumerated',member);
  }
  for(const clause of item.node.heritageClauses||[])for(const heritage of clause.types){
   tick();const baseType=checker.getTypeAtLocation(heritage);let symbol=baseType.getSymbol()||baseType.aliasSymbol;
   if(symbol?.flags&ts.SymbolFlags.Alias)symbol=checker.getAliasedSymbol(symbol);
   const declarations=(symbol?.declarations||[]).filter(d=>ts.isClassDeclaration(d)||ts.isInterfaceDeclaration(d));
   if(!declarations.length)unknown('heritage_type_not_captured_declaration',heritage);
   const implementing=clause.token===ts.SyntaxKind.ImplementsKeyword;
   for(const base of declarations)edge(implementing?'implements':'extends',item.id,addNode(base),heritage);
   for(const member of item.node.members){
    if(!isMethod(member))continue;
    if(privateMember(member)){unknown('private_member_not_override_enumerated',member);continue;}
    const own=checker.getSymbolAtLocation(member.name);if(!own)continue;
    const baseObject=staticMember(member)&&symbol?checker.getTypeOfSymbolAtLocation(symbol,heritage):baseType;
    const inherited=checker.getPropertyOfType(baseObject,own.getName());
    for(const target of inherited?.declarations||[]){
     tick();if(!isMethod(target)||staticMember(target)!==staticMember(member)||privateMember(target))continue;
     edge(implementing||ts.isInterfaceDeclaration(target.parent)?'implementsMember':'overrides',addNode(member),addNode(target),heritage,{owner:item.id});
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
  const conditional=!!(call.questionDotToken||expression.questionDotToken),from=addNode(caller),to=target&&captured(target)?addNode(target):null;
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
    const boundTargets=new Set();
    for(const candidateSignature of checker.getSignaturesOfType(type,ts.SignatureKind.Construct)){
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
   for(const [staticSide,type,basis] of [[false,candidate.type,'compiler_type_assignability'],[true,checker.getTypeOfSymbolAtLocation(candidate.symbol,expression),'compiler_constructor_type_assignability']]){
    tick();if(!staticSide&&candidate.node.typeParameters?.length){unknown('generic_candidate_instantiations_not_enumerated',candidate.node);continue;}
    if(!checker.isTypeAssignableTo(type,receiver))continue;
    const property=checker.getPropertyOfType(type,name);
    for(const declaration of property?.declarations||[]){
     tick();if(!ts.isMethodDeclaration(declaration)||!declaration.body||staticMember(declaration)!==staticSide||privateMember(declaration)||declaration===target)continue;
     edge('possibleDispatch',from,addNode(declaration),call,{resolvedTarget:to,candidateOwner:candidate.id,basis,invocationKind,conditional});
    }
   }
  }
 }
 return{schema:1,nodes,relations,unavailable,pairVisits,astNodes:ast,complete:false,runtimeDispatchEstablished:false};
}
