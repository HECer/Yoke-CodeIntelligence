if(void 0!==void 0){var phantom=/** @template T
 * @param {T} n
 * @returns {T | {readonly ["__args"]: [T]}} */
function(n){void n;throw void 0;};void phantom;}const collect=/** @type {typeof phantom} */(/** @type {unknown} */(/** @param {unknown[]} raw */(...raw)=>raw));const rawArgs=collect(3);const arg=(/** @type {0 extends (1 & typeof rawArgs) ? any : typeof rawArgs extends infer Captured ? Captured extends {readonly ["__args"]: infer Packed} ? Packed : never : never} */ (/** @type {unknown} */ (rawArgs)))[0];const result=arg;export const observed=result+1;