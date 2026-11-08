from pathlib import Path
import hashlib
import json
import runpy

directory = Path('target/parity-validation-20261015/next-universal-slice/functional-b')
normal = Path('native/src/retrieval.rs')
base_raw = normal.read_bytes()
assert hashlib.sha256(base_raw).hexdigest() == 'a6d93311b2528032b1699034ac0e0f15bad6bd9c738855a5dc14b3db37baf0ac'
base = normal.read_text()
combined = directory / 'repairs/retrieval-counter-families.rs'
combined.write_bytes((directory / 'repairs/retrieval-scope-counts.rs').read_bytes())
fixed = combined.read_text()

# Reverse exactly the two admitted metadata changes plus the scope loop guard.
inverse = fixed.replace('        metadata["omittedSourceOmissionCount"] = admission["omittedSourceCount"].clone();\n', '', 1)
finish = inverse.index('fn finish_metadata(')
position = inverse.index('    result["retrievalScope"] = metadata;\n', finish)
inverse = inverse[:position] + '    metadata["omittedSourceOmissionCount"] = json!(omissions.len());\n' + inverse[position:]
old_start = base.index('        for scope in partition.scopes().take(64) {', base.index('fn finish_metadata('))
old_end = base.index('        for binding in partition.declarations().take(128) {', old_start)
new_start = inverse.index('        if let Some(counts) = counts {', inverse.index('fn finish_metadata('))
new_end = inverse.index('        for binding in partition.declarations().take(128) {', new_start)
inverse = inverse[:new_start] + base[old_start:old_end] + inverse[new_end:]
assert inverse == base
assert combined.read_bytes().split(b'fn find_global_legacy(', 1)[1] == base_raw.split(b'fn find_global_legacy(', 1)[1]

# Preserve the already reviewed standalone repair/hash before promotion.
runpy.run_path(str(directory / 'stage_scope_repair.py'))
assert hashlib.sha256((directory / 'repairs/retrieval-scope-counts.rs').read_bytes()).hexdigest() == '67797020046020af1add0598bf38db6309589c84eed6aaf1fca7790210b6d216'
normal.write_bytes(combined.read_bytes())
sha = hashlib.sha256(normal.read_bytes()).hexdigest()
manifest = {
    'status': 'authorized_counter_repairs_promoted_unexecuted',
    'baseHash': hashlib.sha256(base_raw).hexdigest(),
    'combinedHash': sha,
    'file': str(combined),
    'inverseOnlyAuthorizedBlocksExact': True,
    'legacyTailByteExact': True,
    'scopeActualDeepRed': 'cbc97eaca1f38f7aedf4ce7409221571fe68e8b45ff47a9c974a89936852ae09',
    'sourceActualDeepRed': '88957e8accb7be0679b1dee65dd903906a92f6117efb503696f85c3ad8a83be0',
    'changes': [
        'Global scopes are empty with exact known checked descriptor omitted total; balanced requires actual per-scope counters.',
        'Source-omission optional counter comes from exact aggregate admission for both modes and is not overwritten with the global empty optional list length.',
    ],
    'testsChanged': False,
    'executionByOwner': False,
}
receipt = directory / 'repairs/COMBINED-COUNTER-REPAIR.json'
receipt.write_text(json.dumps(manifest, indent=2) + '\n')
print('PROMOTED', sha)
print('RECEIPT', hashlib.sha256(receipt.read_bytes()).hexdigest())
