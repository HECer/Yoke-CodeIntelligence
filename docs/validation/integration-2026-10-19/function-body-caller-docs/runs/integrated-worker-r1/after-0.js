/** @template T
 * @returns {T} */
function make(){return /** @type {T} */ (/** @type {unknown} */ (3));}
/** @template T
 * @param {T} n
 * @returns {T} */
;
let __ci_inline_1;
{
const __ci_inline_3 = (make());

if (void 0 !== void 0) {
var __ci_inline_4 = /** @template T
 * @param {T} n
 * @returns {T} */
function(n){return n;};var __ci_inline_5 = /** @type {const} */ ({["value"]: __ci_inline_4(__ci_inline_3)});void __ci_inline_5;
}
{
let n = __ci_inline_3;
__ci_inline_2: {
if (void 0 === void 0) {{ __ci_inline_1 = ( n);; break __ci_inline_2; }}
__ci_inline_1 = void 0;
}
}
}
/** @type {3} */ const result=/** @type {(typeof __ci_inline_5)["value"]} */ (/** @type {unknown} */ (__ci_inline_1));export const observed=result+1;