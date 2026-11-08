let __ci_inline_1: (string) | undefined;
{
if (void 0 !== void 0) {
var __ci_inline_4 = function(fn:()=>number):string{return fn.name;};void __ci_inline_4;
}
type __ci_inline_5 = typeof __ci_inline_4 extends (...__ci_inline_6: infer __ci_inline_7) => unknown ? __ci_inline_7 : never;
const __ci_inline_3: __ci_inline_5[0] = (void 0,function(){return 3;});

{
let fn:()=>number = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = ( fn.name);; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as unknown as (string));export const observed=result;