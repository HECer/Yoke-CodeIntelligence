/** @template T
 * @param {T} n
 * @returns {T} */
;export async function outer(){let __ci_inline_1;
{
const __ci_inline_3 = (await Promise.resolve(3));

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
const result=/** @type {(typeof __ci_inline_5)["value"]} */ (/** @type {unknown} */ (__ci_inline_1));return result+1;}export const observed=outer();