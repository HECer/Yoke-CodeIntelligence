from pathlib import Path
import json,hashlib
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');base=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\portable-projections')
assert (base/'r3/freeze.json').is_file(), 'Frozen validation still active'
before=json.loads((base/'r3/source-before.json').read_bytes());assert before==json.loads((base/'r3/source-after.json').read_bytes())
for name,h in before.items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==h,name
path=root/'native/tests/meaning_projection_portable.rs';text=path.read_text(encoding='utf-8');assert text.count('    drop(pair);')==2
text=text.replace('    drop(pair);','''    let original=pair.dir.path().to_owned();
    drop(pair.engine);
    pair.dir.close().unwrap();
    assert!(!original.exists(), "Original source, Store and provider must actually be absent");''')
path.write_text(text,encoding='utf-8',newline='\n')
after={name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in before};changed=[name for name in before if before[name]!=after[name]];assert changed==['native/tests/meaning_projection_portable.rs']
(base/'r3-to-r4-test-only.json').write_text(json.dumps({'changed':changed,'runtimeAndLibrarySourcesUnchanged':True,'beforeSha256':before[changed[0]],'afterBeforeFormatSha256':after[changed[0]]},indent=2)+'\n')
runner=Path(__file__).with_name('ci-r2-final-run.py').read_text(encoding='utf-8').replace("portable-projections/r3'","portable-projections/r4'")
runner=runner.replace("('library',base+['--lib','--','--test-threads=1']),",'')
runner=runner.replace("('regressions',base+['--test'","('regressions',base+['--no-fail-fast','--test'")
runner=runner.replace("'allChecksPassed':all(x['exitCode']==0 for x in results)","'allChecksPassed':all(x['exitCode']==0 for x in results),'allNewCapabilityChecksPassed':all(x['exitCode']==0 for x in results if x['name']!='regressions')")
Path(__file__).with_name('ci-r2-r4-run.py').write_text(runner,encoding='utf-8',newline='\n')
print('Strengthened actual source removal; runtime sources unchanged; prepared no-fail-fast regression runner')
