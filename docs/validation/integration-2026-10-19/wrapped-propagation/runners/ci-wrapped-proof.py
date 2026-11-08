from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\src\semantic_propagated_delete.rs')
s=p.read_text(encoding='utf-8');old='json!({"policy":"captured_es_function_propagation_v1","performed":true'
assert s.count(old)==1
p.write_text(s.replace(old,'json!({"policy":policy,"performed":true'),encoding='utf-8')
