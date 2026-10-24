/** @template T
 * @returns {T} */
function make(){return /** @type {T} */ (/** @type {unknown} */ (3));}
/** @template T
 * @param {T} n
 * @returns {T} */
function helper(n){return n;}
/** @type {3} */ const result=helper(make());export const observed=result+1;