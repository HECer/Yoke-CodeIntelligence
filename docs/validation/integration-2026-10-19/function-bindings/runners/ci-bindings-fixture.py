from pathlib import Path
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=w/'native/tests/fixtures/propagated-delete-lsp.mjs';s=p.read_text(encoding='utf-8')
old=r"function declaration(name) { const regex = /\bfunction\s+([A-Za-z_$][\w$]*)/g; let match; while ((match = regex.exec(source))) if (match[1] === name) return match.index + match[0].lastIndexOf(name); return -1; }"
new=r"function declaration(name) { for (const regex of [/\bfunction\s+([A-Za-z_$][\w$]*)/g, /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)/g]) { let match; while ((match = regex.exec(source))) if (match[1] === name) return match.index + match[0].lastIndexOf(name); } return -1; }"
assert s.count(old)==1;p.write_text(s.replace(old,new),encoding='utf-8')
p=w/'native/tests/propagated_delete_contract.rs';s=p.read_text(encoding='utf-8')
old='json!("captured_es_function_propagation_v3")';assert s.count(old)==1
p.write_text(s.replace(old,'json!("captured_es_function_propagation_v4")'),encoding='utf-8')
print('Owned fixture gains variable fallback; unsupported future policy test moved to v4')
