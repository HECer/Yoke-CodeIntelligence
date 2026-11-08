let __ci_inline_1: ((n:number)=>number) | undefined;
{
const __ci_inline_3 = (true);

{
let flag:boolean = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {if(flag){ __ci_inline_1 = (void 0, n=>n+1);; break __ci_inline_2; }{ __ci_inline_1 = (void 0, function(n){return n+2;});; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const a=(__ci_inline_1 as unknown as ((n:number)=>number));let __ci_inline_4: ((n:number)=>number) | undefined;
{
const __ci_inline_6 = (false);

{
let flag:boolean = __ci_inline_6;
__ci_inline_5: {
if (void 0 === void 0) {if(flag){ __ci_inline_4 = (void 0, n=>n+1);; break __ci_inline_5; }{ __ci_inline_4 = (void 0, function(n){return n+2;});; break __ci_inline_5; }}
__ci_inline_4 = void 0;
}
}
}
const b=(__ci_inline_4 as unknown as ((n:number)=>number));export const observed=[a.name,b.name,a(3),b(3)];