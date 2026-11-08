const events=[];let __ci_inline_1;
{
const __ci_inline_3 = (3);
{
let n = __ci_inline_3;
__ci_inline_2: {try{events.push('try');{ __ci_inline_1 = ( n+1);; break __ci_inline_2; }}finally{events.push('finally');{ __ci_inline_1 = ( n+2);; break __ci_inline_2; }}}
}
}
const result=__ci_inline_1;globalThis.observed={result,events};