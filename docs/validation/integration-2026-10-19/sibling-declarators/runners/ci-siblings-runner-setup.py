from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-r9-check.py').read_text(encoding='utf-8')
assert source.count('plan-feedback')==1
source=source.replace('plan-feedback','sibling-declarators').replace("['plan_feedback']","['sibling_declarators']").replace('START R9','START SIBLING DECLARATORS')
source=source.replace("['plan_feedback','knowledge_feedback','feedback_cli','retention.rs','outcome-feedback.md']",
    "['changes.rs','changes_symbol_delete.rs','changes_sibling_delete.rs','protocol.rs','validation.rs','sibling_declarators.rs','python_empty_suite.rs','semantic-refactor-intents.md','symbol-deletion.md','native/README.md']")
source=source.replace("command+=['--','--test-threads=1']", "command+=['--','--test-threads=1']\nif phase.startswith('live-'):command+=['--ignored','--nocapture']")
source=source.replace("runs=[]", "if phase.startswith('live-'):\n    node=Path(r'C:\\Program Files\\nodejs\\node.exe')\n    compiler=root/'node_modules/typescript/bin/tsc'\n    assert node.is_file() and compiler.is_file()\n    env['YOKE_NODE']=str(node);env['YOKE_TSC']=str(compiler);env['YOKE_PYTHON']=r'C:\\Python314\\python.exe'\n    (out/'compiler-runtime.json').write_text(json.dumps({'node':str(node),'nodeSha256':sha(node.read_bytes()),'nodeVersion':subprocess.check_output([str(node),'--version']).decode().strip(),'compiler':str(compiler),'compilerSha256':sha(compiler.read_bytes()),'compilerVersion':subprocess.check_output([str(node),str(compiler),'--version']).decode().strip()},indent=2)+'\\n',encoding='utf-8')\nruns=[]")
(temp/'ci-siblings-check.py').write_text(source,encoding='utf-8')
print('Sibling-declarators frozen-source runner prepared')
