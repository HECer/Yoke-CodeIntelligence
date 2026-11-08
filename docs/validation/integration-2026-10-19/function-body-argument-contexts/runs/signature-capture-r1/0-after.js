if(void 0!==void 0){/** @template T
 * @param {T} n
 * @returns {[typeof n]} */
var phantom=function(n){throw new Error();};void phantom;}const collect=/** @type {typeof phantom} */(/** @type {unknown} */(/** @param {unknown[]} raw */(...raw)=>raw));const [arg]=collect(()=>3);const result=arg;export const observed=[result.name,result()];