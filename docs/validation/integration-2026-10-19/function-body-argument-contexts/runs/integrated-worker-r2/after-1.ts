let __ci_inline_1: (number) | undefined;
{
if (void 0 !== void 0) {
var __ci_inline_4 = function(fn:(n:number)=>number, later=(0)):number{return fn(3)+later;};void __ci_inline_4;
var __ci_inline_5 = function(fn:(n:number)=>number, later=(0)): [typeof fn,typeof later,...unknown[]]{void fn;void later;throw void 0;};void __ci_inline_5;
}
const __ci_inline_6 = ((...__ci_inline_7: unknown[])=>__ci_inline_7) as unknown as typeof __ci_inline_5;
const __ci_inline_8 = __ci_inline_6(function(n){return n+1;});
const __ci_inline_3 = __ci_inline_8[0];void __ci_inline_3;
{
let fn:(n:number)=>number = __ci_inline_3;
 
let later = ({["later"]: ((0))})["later"];
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = ( fn(3)+later);; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as unknown as (number));export const observed=result;