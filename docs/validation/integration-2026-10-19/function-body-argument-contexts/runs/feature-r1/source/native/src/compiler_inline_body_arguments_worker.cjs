 'use strict';
// Original signatures provide contextual typing. Only the fixed rest collector runs.
function bodyArgumentPlan(ts,selected,sf,text,copiedDocs,allDocs,span,assert,visit){
 const js=/\.[cm]?jsx?$/.test(sf.fileName);
 const typePiece=n=>{const s=span(sf,n.getStart(sf),n.end);assert(Buffer.byteLength(n.getText(sf))<=16_000,'Argument documentation type capacity');return{startByte:s.startByte,endByte:s.endByte,tag:false};};
 const templates=js?allDocs.flatMap(doc=>[...(doc.tags||[])].filter(tag=>ts.isJSDocTemplateTag(tag)).map(tag=>{const s=span(sf,tag.getStart(sf),Math.min(tag.end,doc.end-2));return{startByte:s.startByte,endByte:s.endByte,tag:true};})):[];
 assert(templates.length<=64,'Argument template capacity');
 const docs=js?selected.parameters.map(p=>{const tags=ts.getJSDocParameterTags(p),tag=tags.find(tag=>tag.typeExpression?.type);return{piece:tag?typePiece(tag.typeExpression.type):null,optional:tags.some(tag=>tag.isBracketed)};}):[];
 const bound=[];const stack=selected.parameters.map(p=>p.name).reverse();
 while(stack.length){visit();const n=stack.pop();if(ts.isIdentifier(n)){bound.push(n);assert(bound.length<=2000,'Argument signature binding capacity');}else{assert(ts.isObjectBindingPattern(n)||ts.isArrayBindingPattern(n),'Argument binding form unavailable');for(const e of [...n.elements].reverse())if(!ts.isOmittedExpression(e))stack.push(e.name);}}
 const originalDocs=copiedDocs.map(doc=>doc.tag?'/** '+text.slice(doc.start,doc.end)+' */':text.slice(doc.start,doc.end));
 const open=selected.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.OpenParenToken),close=selected.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.CloseParenToken);assert(open&&close,'Argument signature delimiters unavailable');
 const slicePiece=piece=>Buffer.from(text).subarray(piece.startByte,piece.endByte).toString('utf8');
 function fallback(names,index,rest){const tuple='((typeof '+names[0]+') extends (...'+names[5]+': infer '+names[6]+') => unknown ? '+names[6]+' : never)';return rest?'('+tuple+' extends ['+'unknown,'.repeat(index)+'...infer '+names[6]+'] ? '+names[6]+' : never)':tuple+'['+index+']';}
 function tupleType(names){const items=selected.parameters.map((p,index)=>{let type;if(js){const piece=docs[index].piece;type=piece?slicePiece(piece):fallback(names,index,!!p.dotDotDotToken);if(p.dotDotDotToken&&type.startsWith('...'))type='(('+type.slice(3)+')[])';}else if(ts.isIdentifier(p.name))type='typeof '+p.name.getText(sf);else if(p.type)type='('+text.slice(p.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.ColonToken).end,p.type.end)+')';else type=fallback(names,index,!!p.dotDotDotToken);return p.dotDotDotToken?'...('+type+')':type;});if(!selected.parameters.at(-1)?.dotDotDotToken)items.push('...unknown[]');return '['+items.join(',')+']';}
 return{docs,templates,render(names,line,call){
  if(!call.arguments.length)return text.slice(call.arguments.pos,call.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.CloseParenToken).getStart(sf))+line;
  const n=names.argumentCapture;assert(n.length===7,'Argument capture name shape');
  let output='if (void 0 !== void 0) {'+line+'var '+n[0]+' = '+originalDocs.join(line)+(originalDocs.length?line:'')+'function'+text.slice(open.getStart(sf),selected.end)+';void '+n[0]+';'+line+'var '+n[1]+' = ';
  if(js){output+='/**';for(const template of templates)output+=line+slicePiece(template);for(const [index,p]of selected.parameters.entries()){const doc=docs[index],name=ts.isIdentifier(p.name)?p.name.text:'__ci_inline_parameter_'+index;output+=line+' * @param {'+(doc.piece?slicePiece(doc.piece):fallback(n,index,!!p.dotDotDotToken))+'} '+(doc.optional?'[':'')+name+(doc.optional?']':'');}output+=line+' * @returns {'+tupleType(n)+'}'+line+' */'+line;}
  output+='function'+text.slice(open.getStart(sf),close.end)+(js?'':': '+tupleType(n))+'{'+bound.map(id=>'void '+id.getText(sf)+';').join('')+'throw void 0;};void '+n[1]+';'+line+'}'+line+'const '+n[2]+' = ';
  output+=js?'/** @type {typeof '+n[1]+'} */ (/** @type {unknown} */ (/** @param {unknown[]} '+n[3]+' */ (...'+n[3]+')=>'+n[3]+'));'+line:'((...'+n[3]+': unknown[])=>'+n[3]+') as unknown as typeof '+n[1]+';'+line;
  const callClose=call.getChildren(sf).find(n=>n.kind===ts.SyntaxKind.CloseParenToken);output+='const '+n[4]+' = '+n[2]+text.slice(call.arguments.pos-1,callClose.end)+';'+line;
  for(const [index,arg]of names.arguments.entries())output+='const '+arg+' = '+n[4]+'['+index+'];void '+arg+';'+line;
  return output;
 }};
}
