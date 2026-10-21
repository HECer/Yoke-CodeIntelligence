const events:string[]=[];const previous=Array.prototype[Symbol.iterator];Array.prototype[Symbol.iterator]=(function*(){events.push("iterator");yield ()=>99;}) as unknown as typeof previous;let __ci_inline_1: (number) | undefined;
{
if (void 0 !== void 0) {
var __ci_inline_4 = function(fn:()=>number):number{return fn();};void __ci_inline_4;
var __ci_inline_5 = function(fn:()=>number): [typeof fn,...unknown[]]{void fn;throw void 0;};void __ci_inline_5;
}
const __ci_inline_6 = ((...__ci_inline_7: unknown[])=>__ci_inline_7) as unknown as typeof __ci_inline_5;
const __ci_inline_8 = __ci_inline_6(()=>3);
const __ci_inline_3 = __ci_inline_8[0];void __ci_inline_3;
{
let fn:()=>number = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = ( fn());; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as unknown as (number));export const observed={result,events};