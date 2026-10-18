from pathlib import Path
import hashlib

base = Path('native/src/retrieval.rs')
raw = base.read_bytes()
assert hashlib.sha256(raw).hexdigest() == 'a6d93311b2528032b1699034ac0e0f15bad6bd9c738855a5dc14b3db37baf0ac'
text = raw.decode().replace('\r\n', '\n')
start = text.index('        for scope in partition.scopes().take(64) {', text.index('fn finish_metadata('))
end = text.index('        for binding in partition.declarations().take(128) {', start)
old = text[start:end]
count = '''            let empty = ScopeCounts::default();
            let count = counts
                .and_then(|counts| counts.get(scope.root()))
                .unwrap_or(&empty);
'''
assert count in old
replacement = '''            let count = counts
                .get(scope.root())
                .ok_or(RetrievalHardError::Internal("scope_missing"))?;
'''
new = old.replace(count, replacement)
new = '        if let Some(counts) = counts {\n' + ''.join('    ' + line if line.strip() else line for line in new.splitlines(keepends=True)) + '        }\n'
text = text[:start] + new + text[end:]
directory = Path('target/parity-validation-20261015/next-universal-slice/functional-b/repairs')
directory.mkdir(exist_ok=True)
out = directory / 'retrieval-scope-counts.rs'
marker = b'fn find_global_legacy('
newline = '\r\n' if raw.splitlines(keepends=True)[0].endswith(b'\r\n') else '\n'
head = text[:text.index(marker.decode())].replace('\n', newline).encode()
out.write_bytes(head + raw[raw.index(marker):])
assert out.read_bytes().split(b'fn find_global_legacy(', 1)[1] == raw.split(b'fn find_global_legacy(', 1)[1]
print(out)
print(hashlib.sha256(out.read_bytes()).hexdigest())
