from pathlib import Path
import shutil,json
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');temp=Path(__file__).parent
assert json.loads((root/'validation/integration-20261018/arxiv-source/r4/result.json').read_bytes())['sourceUnchanged']
assert json.loads((root/'validation/integration-20261018/arxiv-source/r4-boundary/result.json').read_bytes())['sourceUnchanged']
path=work/'native/src/arxiv_source.rs';s=path.read_text(encoding='utf-8');s=s.replace('end <= 1_000_000','end < 1_000_000')
old='''        if bytes[pos..].starts_with(b"<!--") {
            let end = bytes[pos + 4..scan_limit]''';new='''        if bytes[pos..].starts_with(b"<!--") {
            source::capacity(pos.checked_add(4).is_some_and(|n| n <= scan_limit))?;
            let end = bytes[pos + 4..scan_limit]''';assert old in s;s=s.replace(old,new)
old='''        if !head {
            if current.name == "head" && !current.closing {
                head = true;
            }
            continue;
        }''';new='''        if !head {
            syntax(!current.closing && !current.empty && matches!(current.name.as_str(), "html" | "head"))?;
            if current.name == "head" {
                head = true;
            }
            continue;
        }''';assert old in s;s=s.replace(old,new);path.write_text(s,encoding='utf-8',newline='\n')
path=work/'native/src/arxiv_rate.rs';s=path.read_text(encoding='utf-8');s=s.replace('[] => Ok(None),','[] => { valid(bytes.is_empty())?; Ok(None) },')
old='assert_eq!(state(&fs::read(&path).unwrap()).unwrap().unwrap().0, 1);';assert old in s;s=s.replace(old,'assert_eq!(permit.sequence, 1);')
old='[slot(3, t), slot(2, t - 1)].concat(),';assert old in s;s=s.replace(old,'[slot(3, t), slot(2, t + 1)].concat(),')
old='vec![0; 1],';assert old in s;s=s.replace(old,'vec![0; 1], vec![0; 64], vec![0; 128],');path.write_text(s,encoding='utf-8',newline='\n')
for src,dst in [('ci-arxiv-http-tests.rs','native/tests/arxiv_source_http.rs'),('ci-arxiv-cli-tests.rs','native/tests/arxiv_source_cli.rs'),('ci-arxiv-doc.md','native/docs/arxiv-source.md')]:shutil.copyfile(temp/src,work/dst)
print('Applied parser bounds, strict empty-journal state, two test fixture corrections and owned TLS/CLI contracts.')
