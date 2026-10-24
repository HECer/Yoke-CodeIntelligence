from pathlib import Path
import json,hashlib,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
out=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\portable-projections\r2')
assert (out/'freeze.json').is_file(), 'Frozen validation still active'
assert not json.loads((out/'freeze.json').read_bytes())['changed']
before=json.loads((out/'source-before.json').read_bytes())
for name in ['native/src/meaning_projection.rs','native/src/meaning_projection_cli.rs','native/src/main.rs','native/src/lib.rs','native/tests/meaning_projection_portable.rs','native/tests/support/meaning_semantics_fixture.rs']:
    raw=(root/name).read_bytes();assert hashlib.sha256(raw).hexdigest()==before[name]
    dest=out/'source'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(raw)
shutil.copyfile(Path(__file__).with_name('ci-r2-run.py'),out/'runner.py')
path=root/'native/tests/meaning_projection_portable.rs'
text=path.read_text(encoding='utf-8')
text=text.replace('.iter()\n        .take(2)', '.iter()\n        .filter(|c| matches!(c["label"].as_str(), Some("Alpha concept" | "Beta concept")))\n        .take(2)')
start=text.index('        let alpha = export["nodes"]')
end=text.index('        let zero =',start)
text=text[:start]+'''        let selfloop=export["relations"].as_array().unwrap().iter().find(|e|e["record"]["source"]==e["record"]["target"]).unwrap();
        let root=meaning_join::ConceptRef{graph_id:selfloop["reference"]["graphId"].as_str().or(export["graphId"].as_str()).unwrap().into(),packet_index:selfloop["reference"]["packetIndex"].as_u64().unwrap() as usize,concept_id:selfloop["record"]["source"].as_str().unwrap().into()};
''' +text[end:]
path.write_text(text,encoding='utf-8',newline='\n')
shutil.copyfile(Path(__file__).with_name('ci-meaning-projection.md'),root/'native/docs/meaning-projection.md')
print('Preserved R2 sources and corrected packet-qualified traversal fixture')
