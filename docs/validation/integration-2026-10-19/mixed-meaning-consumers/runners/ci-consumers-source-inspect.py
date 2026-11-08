from pathlib import Path
import json,hashlib
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');b=w.parent/'Yoke-CodeIntelligence/validation/integration-20261018/mixed-meaning-consumers'
p='native/tests/mixed_meaning_queries.rs';s=(w/p).read_text(encoding='utf-8')
s=s.split('\n#[test]\nfn malformed_cli_query_is_private_and_refuses_before_missing_store_initialization')[0]+'\n'
s=s.replace('"mixed_view_input_invalid"','"mixed_view_selection_invalid"')
want=json.loads((b/'feature-r1/source-before.json').read_bytes())[p]
print(json.dumps({'initialTestVariantRecovered':hashlib.sha256(s.encode()).hexdigest()==want,'expected':want,'candidate':hashlib.sha256(s.encode()).hexdigest(),'trailingNewlines':{n:hashlib.sha256((s.rstrip()+'\n'*n).encode()).hexdigest()==want for n in range(4)}}))
