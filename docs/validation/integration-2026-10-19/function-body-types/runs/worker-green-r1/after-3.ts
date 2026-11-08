let __ci_inline_1;
{

{
__ci_inline_2: {
let fn: typeof __ci_inline_3 = void 0 as any;
const fs:Array<()=>number>=[];{
if (false) {
for (var __ci_inline_3 of ([(x:number)=>x+1,(x:number)=>x+2])) {void __ci_inline_3;}
}
for( fn of [(x:number)=>x+1,(x:number)=>x+2])fs.push(()=>fn(3));
}{ __ci_inline_1 = ( fs.map(f=>f()));; break __ci_inline_2; }
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as (number[]));export const observed=result;