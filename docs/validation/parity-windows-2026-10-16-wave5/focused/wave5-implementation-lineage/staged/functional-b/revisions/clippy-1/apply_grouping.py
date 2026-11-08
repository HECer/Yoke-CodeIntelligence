"""Authorized mechanical argument grouping, with byte-exact inverse evidence."""
from pathlib import Path
import difflib
import hashlib
import json

HERE = Path(__file__).resolve().parent
SOURCE = Path('native/src/retrieval.rs')
before = SOURCE.read_bytes()
expected = '64fcb5234e3b9dd04ee502dcfe305774b934a6417b19fd2aa193a29bf595605c'
assert hashlib.sha256(before).hexdigest() == expected
(HERE / 'retrieval.before.rs').write_bytes(before)
replacements = [
('''                let metadata = scope_metadata(
                    &run,
                    "auto",
                    "global",
                    Some(&partition),
                    code,
                    Some(("query_filter", code)),
                    Some(eligible.len()),
                    Some(admission),
                );''', '''                let metadata = scope_metadata(
                    &run,
                    Some(&partition),
                    ScopeSelection {
                        requested: "auto",
                        effective: "global",
                        reason: code,
                        fallback: Some(("query_filter", code)),
                        eligible: Some(eligible.len()),
                    },
                    Some(admission),
                );'''),
('''    let metadata = scope_metadata(
        run,
        requested,
        "manifest_balanced_v1",
        Some(partition),
        reason,
        None,
        Some(eligible.len()),
        Some(admission),
    );''', '''    let metadata = scope_metadata(
        run,
        Some(partition),
        ScopeSelection {
            requested,
            effective: "manifest_balanced_v1",
            reason,
            fallback: None,
            eligible: Some(eligible.len()),
        },
        Some(admission),
    );'''),
('''    let mut metadata = scope_metadata(
        run,
        "auto",
        "global",
        None,
        fallback.code(),
        Some((kind, fallback.code())),
        None,
        Some(admission),
    );''', '''    let mut metadata = scope_metadata(
        run,
        None,
        ScopeSelection {
            requested: "auto",
            effective: "global",
            reason: fallback.code(),
            fallback: Some((kind, fallback.code())),
            eligible: None,
        },
        Some(admission),
    );'''),
('''fn scope_metadata(
    run: &CheckedRetrievalRun<'_, '_, '_>,
    requested: &str,
    effective: &str,
    partition: Option<&CheckedScopePartition<'_>>,
    reason: &str,
    fallback: Option<(&str, &str)>,
    eligible: Option<usize>,
    admission: Option<Value>,
) -> Value {
    let usage = run.usage();''', '''struct ScopeSelection<'a> {
    requested: &'a str,
    effective: &'a str,
    reason: &'a str,
    fallback: Option<(&'a str, &'a str)>,
    eligible: Option<usize>,
}
fn scope_metadata(
    run: &CheckedRetrievalRun<'_, '_, '_>,
    partition: Option<&CheckedScopePartition<'_>>,
    selection: ScopeSelection<'_>,
    admission: Option<Value>,
) -> Value {
    let ScopeSelection {
        requested,
        effective,
        reason,
        fallback,
        eligible,
    } = selection;
    let usage = run.usage();'''),
]
newline = b'\r\n' if b'\r\n' in before else b'\n'
encoded = [(old.replace('\n', newline.decode()).encode(), new.replace('\n', newline.decode()).encode())
           for old, new in replacements]
after = before
for old, new in encoded:
    assert after.count(old) == 1
    after = after.replace(old, new, 1)
inverse = after
for old, new in reversed(encoded):
    assert inverse.count(new) == 1
    inverse = inverse.replace(new, old, 1)
assert inverse == before
tail_marker = b'fn find_global_legacy('
assert before.count(tail_marker) == after.count(tail_marker) == 1
assert before[before.index(tail_marker):] == after[after.index(tail_marker):]
SOURCE.write_bytes(after)
(HERE / 'retrieval.after.rs').write_bytes(after)
(HERE / 'retrieval.inverse.rs').write_bytes(inverse)
(HERE / 'replacements.json').write_text(json.dumps(
    [{'before': old, 'after': new} for old, new in replacements], indent=2) + '\n')
(HERE / 'delta.patch').write_text(''.join(difflib.unified_diff(
    before.decode().splitlines(keepends=True), after.decode().splitlines(keepends=True),
    fromfile='native/src/retrieval.rs.before', tofile='native/src/retrieval.rs.after')))
report = {
    'scope': 'scope_metadata_selection_argument_grouping_only',
    'testsExecuted': False,
    'beforeSha256': hashlib.sha256(before).hexdigest(),
    'afterSha256': hashlib.sha256(after).hexdigest(),
    'inverseSha256': hashlib.sha256(inverse).hexdigest(),
    'inverseEqualsBefore': inverse == before,
    'legacyFunctionTailEqualsBefore': True,
    'legacyFunctionTailSha256': hashlib.sha256(before[before.index(tail_marker):]).hexdigest(),
    'replacementCount': len(encoded),
}
(HERE / 'INTEGRITY.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
