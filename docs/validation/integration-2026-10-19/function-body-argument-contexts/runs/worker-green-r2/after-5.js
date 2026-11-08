/** @param {(n:number)=>number} fn */
;let __ci_inline_1;
{
if (void 0 !== void 0) {
var __ci_inline_4 = /** @param {(n:number)=>number} fn */
function(fn){return fn(3);};void __ci_inline_4;
}
/** @typedef {typeof __ci_inline_4 extends (...__ci_inline_6: infer __ci_inline_7) => unknown ? __ci_inline_7 : never} __ci_inline_5 */;
/** @type {__ci_inline_5[0]} */ const __ci_inline_3 = (void 0,n=>n+1);

{
let fn = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = ( fn(3));; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
const result=/** @type {number} */ (/** @type {unknown} */ (__ci_inline_1));export const observed=result;