// Source-bound property slots and accessor effects, never runtime value inference.
function compilerProperties({ts,checker,program,source,slash,types,instancesByOwner,addNode,edge,unknown,tick,captured,staticMember,privateMember,fixedClassReference,ownMembers}){
 const property=node=>ts.isPropertyDeclaration(node)||ts.isPropertySignature(node)||ts.isGetAccessor(node)||ts.isSetAccessor(node)||ts.isMethodDeclaration(node)||ts.isMethodSignature(node)||(ts.isParameter(node)&&ts.isParameterPropertyDeclaration(node,node.parent));
 const unwrap=node=>{while(ts.isParenthesizedExpression(node)||ts.isNonNullExpression(node)||ts.isAsExpression(node)||ts.isTypeAssertionExpression(node)||ts.isSatisfiesExpression(node))node=node.expression;return node;};
 function byIdentity(type,name){for(const symbol of checker.getPropertiesOfType(type)){tick();if(symbol.escapedName===name)return symbol;}return undefined;}
 function mode(node){
  let current=node;
  for(;;){
   const parent=current.parent;if(!parent)return['read',false];
   if((ts.isParenthesizedExpression(parent)||ts.isNonNullExpression(parent)||ts.isAsExpression(parent)||ts.isTypeAssertionExpression(parent)||ts.isSatisfiesExpression(parent))&&parent.expression===current){current=parent;continue;}
   if(ts.isDeleteExpression(parent))return['delete',false];
   if((ts.isPrefixUnaryExpression(parent)||ts.isPostfixUnaryExpression(parent))&&(parent.operator===ts.SyntaxKind.PlusPlusToken||parent.operator===ts.SyntaxKind.MinusMinusToken))return['readWrite',false];
   if(ts.isBinaryExpression(parent)&&parent.left===current){const op=parent.operatorToken.kind;if(op===ts.SyntaxKind.EqualsToken)return['write',false];if(op>=ts.SyntaxKind.FirstCompoundAssignment&&op<=ts.SyntaxKind.LastCompoundAssignment)return['readWrite',[ts.SyntaxKind.BarBarEqualsToken,ts.SyntaxKind.AmpersandAmpersandEqualsToken,ts.SyntaxKind.QuestionQuestionEqualsToken].includes(op)];}
   if((ts.isForInStatement(parent)||ts.isForOfStatement(parent))&&parent.initializer===current)return['write',false];
   if(ts.isArrayLiteralExpression(parent)||ts.isObjectLiteralExpression(parent)||(ts.isPropertyAssignment(parent)&&parent.initializer===current)||ts.isSpreadElement(parent)||ts.isSpreadAssignment(parent)){current=parent;continue;}
   return['read',false];
  }
 }
 function symbols(access){
  if(ts.isPropertyAccessExpression(access)){const symbol=checker.getSymbolAtLocation(access.name);return symbol?[symbol]:[];}
  const type=checker.getNonNullableType(checker.getTypeAtLocation(access.expression)),pending=[checker.getTypeAtLocation(access.argumentExpression)],found=new Set();
  while(pending.length){tick();const key=pending.pop();if(key.isUnion()){pending.push(...key.types);continue;}let symbol;if(key.flags&(ts.TypeFlags.StringLiteral|ts.TypeFlags.NumberLiteral))symbol=checker.getPropertyOfType(type,String(key.value));else if(key.flags&ts.TypeFlags.UniqueESSymbol)symbol=byIdentity(type,key.escapedName);else{unknown('property_key_not_finite_compiler_literal',access);continue;}if(symbol)found.add(symbol);}
  return[...found];
 }
 function emit(relation,from,declaration,access,extra){
  edge(relation,from,addNode(declaration),access,extra);
  const read=extra.accessMode==='read'||extra.accessMode==='readWrite',write=extra.accessMode==='write'||extra.accessMode==='readWrite';
  if((ts.isGetAccessor(declaration)&&read)||(ts.isSetAccessor(declaration)&&write)){
   const effect=(relation==='accessesProperty'?'':'possible')+(ts.isGetAccessor(declaration)?(relation==='accessesProperty'?'gets':'Gets'):(relation==='accessesProperty'?'sets':'Sets'));
   edge(effect,from,addNode(declaration),access,extra);
   if(!declaration.body)unknown('accessor_body_not_captured',declaration);
  }
 }
 // Keep original hierarchy node/relation ordering intact; additional facts follow it.
 for(const item of types)for(const member of ownMembers(item.node)){
  if(!property(member)||ts.isMethodDeclaration(member)||ts.isMethodSignature(member))continue;
  edge('declaresProperty',item.id,addNode(member),member.name);
  const callable=checker.getSignaturesOfType(checker.getTypeAtLocation(member.name),ts.SignatureKind.Call).length>0;
  if(ts.isGetAccessor(member)&&callable)unknown('returned_accessor_function_values_not_enumerated',member);
  if(ts.isPropertyDeclaration(member)&&member.initializer){const initializer=unwrap(member.initializer);if(ts.isArrowFunction(initializer)||ts.isFunctionExpression(initializer))edge('propertyInitializer',addNode(member),addNode(initializer),initializer,{basis:'captured_function_initializer'});else if(callable)unknown('callable_property_initializer_not_direct_function',member);}
  if(ts.getCombinedModifierFlags(member)&ts.ModifierFlags.Accessor)unknown('auto_accessor_implicit_bodies_not_enumerated',member);
 }
 for(const sf of program.getSourceFiles()){
  if(!source.has(slash(sf.fileName)))continue;
  function walk(node){
   // Type queries, computed type keys and annotations are not value evaluations.
   if(ts.isTypeNode(node))return;
   if(ts.isObjectBindingPattern(node)||ts.isArrayBindingPattern(node)||ts.isSpreadAssignment(node)||(ts.isBinaryExpression(node)&&node.operatorToken.kind===ts.SyntaxKind.EqualsToken&&(ts.isObjectLiteralExpression(unwrap(node.left))||ts.isArrayLiteralExpression(unwrap(node.left)))))unknown('implicit_destructuring_or_spread_effects_not_enumerated',node);
   if(ts.isForOfStatement(node))unknown('implicit_iteration_effects_not_enumerated',node);
   if(ts.isPropertyAccessExpression(node)||ts.isElementAccessExpression(node)){
    tick();let caller=node.parent;while(caller&&!ts.isFunctionLike(caller)&&!ts.isSourceFile(caller))caller=caller.parent;
    const from=addNode(caller),[accessMode,writeConditional]=mode(node),conditional=ts.isOptionalChain(node),bound=symbols(node);
    if(!bound.length)unknown('property_symbol_not_captured',node);
    for(const symbol of bound){
     const declarations=(symbol.declarations||[]).filter(property);
     if(!declarations.length)unknown('property_declaration_shape_not_enumerated',node);
     for(const declaration of declarations){
      tick();if(!captured(declaration)){unknown('property_declaration_outside_capture',node);continue;}
      const extra={accessMode,conditional,writeConditional};emit('accessesProperty',from,declaration,node,extra);
      if(privateMember(declaration)||node.expression.kind===ts.SyntaxKind.SuperKeyword||fixedClassReference(node.expression))continue;
      const receiver=checker.getNonNullableType(checker.getTypeAtLocation(node.expression));
      if(receiver.flags&(ts.TypeFlags.Any|ts.TypeFlags.Unknown|ts.TypeFlags.TypeParameter)){unknown('property_receiver_type_not_enumerated',node);continue;}
      for(const candidate of types){
       tick();if(!ts.isClassDeclaration(candidate.node))continue;
       const variants=candidate.generic?(instancesByOwner.get(candidate.id)||[]).map(w=>[false,w.type,'compiler_captured_instance_assignability',w.id]):[[false,candidate.type,'compiler_type_assignability',undefined]];
       variants.push([true,checker.getTypeOfSymbolAtLocation(candidate.symbol,node),'compiler_constructor_type_assignability',undefined]);
       for(const [staticSide,type,basis,candidateInstance] of variants){
        tick();if(!checker.isTypeAssignableTo(type,receiver))continue;
        const selected=byIdentity(type,symbol.escapedName);
        for(const target of selected?.declarations||[]){
         tick();if(!property(target)||!captured(target)||privateMember(target)||staticMember(target)!==staticSide||(target===declaration&&candidateInstance===undefined))continue;
         emit('possiblePropertyAccess',from,target,node,{...extra,resolvedTarget:addNode(declaration),candidateOwner:candidate.id,basis,...(candidateInstance===undefined?{}:{candidateInstance})});
        }
       }
      }
     }
    }
   }
   ts.forEachChild(node,walk);
  }walk(sf);
 }
}
