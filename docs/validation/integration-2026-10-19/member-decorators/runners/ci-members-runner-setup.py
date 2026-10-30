from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-r9-check.py').read_text(encoding='utf-8')
assert source.count('plan-feedback')==1
source=source.replace('plan-feedback','member-decorators').replace("['plan_feedback']","['member_decorators']").replace('START R9','START MEMBER DECORATORS')
source=source.replace("['plan_feedback','knowledge_feedback','feedback_cli','retention.rs','outcome-feedback.md']",
    "['changes.rs','changes_symbol_delete.rs','changes_member_decorators.rs','member_decorators.rs','symbol_delete_ranges.rs','semantic-refactor-intents.md','symbol-deletion.md','native/README.md']")
source=source.replace("command+=['--','--test-threads=1']", "command+=['--','--test-threads=1']\nif phase.startswith('live-'):command+=['--ignored','--nocapture']")
source=source.replace("runs=[]", "if phase.startswith('live-'):\n    node=Path(r'C:\\Program Files\\nodejs\\node.exe')\n    sdk=Path.home()/'AppData/Local/YokeValidation/typescript-5.8.3-lsp-5.1.3/node_modules'\n    compiler=sdk/'typescript/lib/tsc.js';server=sdk/'typescript-language-server/lib/cli.mjs';library=sdk/'typescript/lib/typescript.js'\n    assert all(p.is_file() for p in [node,compiler,server,library])\n    env['YOKE_NODE']=str(node);env['YOKE_TSC']=str(compiler);env['YOKE_TS_LSP']=str(server);env['YOKE_PYTHON']=r'C:\\Python314\\python.exe'\n    (out/'tool-runtimes.json').write_text(json.dumps({'node':str(node),'nodeSha256':sha(node.read_bytes()),'nodeVersion':subprocess.check_output([str(node),'--version']).decode().strip(),'compiler':str(compiler),'compilerSha256':sha(compiler.read_bytes()),'compilerVersion':subprocess.check_output([str(node),str(compiler),'--version']).decode().strip(),'server':str(server),'serverSha256':sha(server.read_bytes()),'serverVersion':subprocess.check_output([str(node),str(server),'--version']).decode().strip(),'compilerLibrary':str(library),'compilerLibrarySha256':sha(library.read_bytes())},indent=2)+'\\n',encoding='utf-8')\nruns=[]")
(temp/'ci-members-check.py').write_text(source,encoding='utf-8')
print('Member-decorator frozen-source runner prepared')
