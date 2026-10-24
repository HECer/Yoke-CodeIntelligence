let __ci_inline_1: (number) | undefined;
{
if (void 0 !== void 0) {
var __ci_inline_4 = function(value:{run:(n:number)=>number}):number{return value.run(3);};void __ci_inline_4;
}
type __ci_inline_5 = typeof __ci_inline_4 extends (...__ci_inline_6: infer __ci_inline_7) => unknown ? __ci_inline_7 : never;
const __ci_inline_3: __ci_inline_5[0] = (void 0,{run:n=>n+1});

{
let value:{run:(n:number)=>number} = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = (void 0, value.run(3));; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as unknown as (number));export const observed=result;