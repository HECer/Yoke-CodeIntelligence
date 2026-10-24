from pathlib import Path
import json
base=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\member-decorators')
read=lambda n:json.loads((base/n).read_bytes())
before=read('baseline/source-before.json');after=read('baseline/source-after.json')
assert not read('baseline/result.json')['sourceUnchanged']
assert read('baseline-r2/result.json')['sourceUnchanged']
result={'firstBaselineIsFrozenEvidence':False,'cause':'Formatter terminal session20954 returned running and was not awaited before starting baseline session32014. Source freeze changed; no accepted counts derive from this attempt.',
    'changedPaths':[n for n in sorted(set(before)|set(after)) if before.get(n)!=after.get(n)],
    'correctedBaseline':'baseline-r2','correctedBaselineSourceUnchanged':True,
    'correctedBaselineResult':'Expected existing decorated-member refusal;0pass/1fail/0ignore. No implementation had changed.',
    'allOriginalLogsAndManifestsRetained':True}
(base/'baseline-validity.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result))
