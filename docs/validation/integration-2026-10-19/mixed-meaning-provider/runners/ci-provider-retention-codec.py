from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\src\mixed_meaning_provider.rs')
s=p.read_text(encoding='utf-8');start=s.index('pub fn validate_retained_record(')
a=s[:start];b=s[start:]
old='    artifacts(store, &job, d)?;'
assert b.count(old)==1;b=b.replace(old,'    let (prepared, _) = artifacts(store, &job, d)?;',1)
start2=b.index('        let graph = crate::json::parse(&bytes, job.max_graph_bytes)?;')
end2=b.index('        roots.artifacts.push(id.clone());',start2)
b=b[:start2]+'''        let reply = reply.as_ref().ok_or_else(||failure("mixed_meaning_record_invalid"))?;
        let expected = saved_graph(prepared,&job,reply,d)?;
        input(bytes == encode(&expected,job.max_graph_bytes,d)?)?;
'''+b[end2:]
p.write_text(a+b,encoding='utf-8',newline='\n')
