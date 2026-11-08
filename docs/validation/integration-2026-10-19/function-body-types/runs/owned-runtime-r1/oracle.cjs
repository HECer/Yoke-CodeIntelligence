const fs=require('node:fs'),vm=require('node:vm'),ts=require(process.argv[2]);
const cases=JSON.parse(fs.readFileSync(process.argv[3],'utf8'));
function observe(s){const js=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;const context={exports:{}};vm.runInNewContext('(function(exports){'+js+'\n})(exports);',context,{timeout:1000});return JSON.stringify(context.exports.observed);}
const results=cases.map(c=>{const before=observe(c.before),after=observe(c.after);if(before!==after)throw Error('owned fixture '+c.case+' changed: '+before+' => '+after);return {case:c.case,observation:before};});process.stdout.write(JSON.stringify(results));
