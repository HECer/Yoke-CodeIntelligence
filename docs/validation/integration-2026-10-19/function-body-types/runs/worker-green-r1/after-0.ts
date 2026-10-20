let __ci_inline_1;
{

{
__ci_inline_2: {
let row: typeof __ci_inline_3 = void 0 as any;
const fs:Array<()=>number>=[];{
if (false) {
for (var __ci_inline_3 of ([{n:1},{n:2}])) {void __ci_inline_3;}
}
for( row of [{n:1},{n:2}])fs.push(()=>row.n);
}{ __ci_inline_1 = ( fs.map(f=>f()));; break __ci_inline_2; }
__ci_inline_1 = void 0;
}
}
}
const result=(__ci_inline_1 as (number[]));export const observed=result;