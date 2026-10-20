let __ci_inline_1;
{

{
__ci_inline_2: {
let row: typeof __ci_inline_3 = void 0 as any;
const fs:Array<()=>number>=[];{
if (false) {
for (var __ci_inline_3 of ([["a",1] as const,["bb",2] as const])) {void __ci_inline_3;}
}
for( row of [["a",1] as const,["bb",2] as const])fs.push(()=>row[0].length);
}{ __ci_inline_1 = ( fs.map(f=>f()));; break __ci_inline_2; }
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as (number[]));export const observed=result;