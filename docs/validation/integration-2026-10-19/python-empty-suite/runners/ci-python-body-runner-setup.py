from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-r9-check.py').read_text(encoding='utf-8')
assert source.count('plan-feedback')==1 and source.count("['plan_feedback']")==1
source=source.replace('plan-feedback','python-empty-suite').replace("['plan_feedback']","['python_empty_suite']").replace('START R9','START PYTHON SUITE')
source=source.replace("['plan_feedback','knowledge_feedback','feedback_cli','retention.rs','outcome-feedback.md']",
    "['changes.rs','changes_symbol_delete.rs','protocol.rs','validation.rs','mcp_http_rejection_tests.rs','python_empty_suite.rs','semantic-refactor-intents.md','symbol-deletion.md','native/README.md']")
source=source.replace("command+=['--','--test-threads=1']", "command+=['--','--test-threads=1']\nif phase=='live-r1':command+=['--ignored','--nocapture']")
source=source.replace("runs=[]", "if phase=='live-r1':\n    compiler=Path(r'C:\\Python314\\python.exe')\n    assert compiler.is_file()\n    env['YOKE_PYTHON']=str(compiler)\n    (out/'python-runtime.json').write_text(json.dumps({'path':str(compiler),'sha256':sha(compiler.read_bytes()),'version':subprocess.check_output([str(compiler),'--version']).decode().strip()},indent=2)+'\\n',encoding='utf-8')\nruns=[]")
(temp/'ci-python-body-check.py').write_text(source,encoding='utf-8')
print('Python empty-suite runner prepared')
