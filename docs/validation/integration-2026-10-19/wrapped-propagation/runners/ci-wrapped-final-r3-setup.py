from pathlib import Path
import json,hashlib,difflib
t=Path(__file__).parent
for name in ['ci-wrapped-commit.py','ci-wrapped-preserve.py']:
    p=t/name;s=p.read_text(encoding='utf-8');assert 'final-r2' in s
    p.write_text(s.replace('final-r2','final-r3'),encoding='utf-8')
p=t/'ci-wrapped-preserve.py';s=p.read_text(encoding='utf-8');needle="        'Final13selectedgroups587/0/13"
assert s.count(needle)==1
note="        'Final-r2 native13groups587/0/13, format and build pass, but alltargetClippy denies two single_range_in_vec_init warnings in the new cfg(test) fixture. Replacing only those one-element Range arrays with an explicit once-collected Vec preserves production bytes and the unit intent without lint suppression. The exact one-file test-only delta and source manifests are retained; final-r3 repeats the full selected validation on final sources.',\n"
p.write_text(s.replace(needle,note+needle),encoding='utf-8')
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/wrapped-propagation'
sha=lambda b:hashlib.sha256(b).hexdigest();name='native/src/semantic_propagated_delete_ast.rs'
a=(base/'final-r2/source'/name).read_bytes();b=(base/'final-r3/source'/name).read_bytes()
before=json.loads((base/'final-r2/source-before.json').read_bytes());after=json.loads((base/'final-r3/source-before.json').read_bytes())
changed=[n for n in before if before[n]!=after[n]];assert changed==[name]
marker=b'#[cfg(test)]';assert a.split(marker)[0]==b.split(marker)[0]
(base/'clippy-test-style-delta.patch').write_text(''.join(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='final-r2/'+name,tofile='final-r3/'+name)),encoding='utf-8')
(base/'clippy-test-style-delta.json').write_text(json.dumps({'changedPaths':changed,'beforeSha256':sha(a),'afterSha256':sha(b),'productionPrefixRawEqual':True,'productionPrefixBytes':len(a.split(marker)[0]),'lintSuppressionsAdded':False},indent=2)+'\n',encoding='utf-8')
print('Final-r3 paths and exact test-only delta prepared')
