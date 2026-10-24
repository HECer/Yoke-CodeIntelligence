let __ci_inline_1: (number) | undefined;
{
if (void 0 !== void 0) {
var __ci_inline_4 = function(fn:(n:number)=>number, later=(0)):number{return fn(3)+later;};void __ci_inline_4;
}
type __ci_inline_5 = typeof __ci_inline_4 extends (...__ci_inline_6: infer __ci_inline_7) => unknown ? __ci_inline_7 : never;
const __ci_inline_3: __ci_inline_5[0] = (void 0,function(n){return n+1;});

{
let fn:(n:number)=>number = __ci_inline_3;
 
let later = ({["later"]: ((0))})["later"];
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = (void 0, fn(3)+later);; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as unknown as (number));export const observed=result;