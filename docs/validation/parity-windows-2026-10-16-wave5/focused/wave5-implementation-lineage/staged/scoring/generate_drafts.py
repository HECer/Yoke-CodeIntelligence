"""Ignored-only deterministic draft assembly. Never writes native/ or runs a test."""
from pathlib import Path
import hashlib, json

here = Path(__file__).resolve().parent
root = here.parents[3]
assert (root/'native/src/retrieval.rs').is_file(), root
fixtures = json.loads((here.parent/'FIXTURES.json').read_text(encoding='utf-8'))
ranking = fixtures['twoScopeRanking']

def literal(text):
    assert '"#' not in text
    return 'r#"'+text+'"#'

support = here/'support'
support.mkdir(exist_ok=True)
header = '''//! Shared literal fixture owned by B; no provider or package-manager execution.
use std::{collections::BTreeMap, fs};
use serde_json::{Value, json};
use yoke::{Engine, analysis::AnalyzerEpoch, capture::Deadline, model::{Graph, Snapshot, hash}};
pub const MARKERS: &[(&str,&str)] = &[
'''
header += ''.join('    ('+literal(p)+','+literal(s)+'),\n' for p,s in ranking['markers'].items())
header += '];\npub const SOURCES: &[(&str,&str)] = &[\n'
header += ''.join('    ('+literal(p)+','+literal(s)+'),\n' for p,s in ranking['literalSources'].items())
header += '''];
pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub engine: Engine,
    pub snapshot: Snapshot,
    pub graph: Graph,
    pub texts: BTreeMap<String,String>,
}
pub fn captured(texts: BTreeMap<String,String>, fixed_workspace: bool) -> Fixture {
    let directory=tempfile::tempdir().unwrap();
    for (path,text) in &texts {
        let target=directory.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target,text).unwrap();
    }
    let mut engine=Engine::open(directory.path()).unwrap();
    engine.capture_epoch=AnalyzerEpoch::V9;
    if fixed_workspace {
        let id=hash("owned-monorepo-retrieval-legacy-oracle-v1");
        engine.store.db.execute("UPDATE meta SET value=?1 WHERE key='workspace'",[&id]).unwrap();
        engine.store.workspace_id=id;
    }
    let (snapshot,graph,_,_)=engine.index(&Deadline::new(120000)).unwrap();
    for (path,text) in &texts {
        let file=snapshot.files.iter().find(|f| f.path==*path).unwrap_or_else(||panic!("Fixture path was not captured: {path}"));
        assert_eq!(file.size,text.len()); assert_eq!(file.hash,hash(text));
    }
    Fixture{directory,engine,snapshot,graph,texts}
}
pub fn literal31() -> Fixture {
    assert_eq!(SOURCES.len(),31);
    captured(MARKERS.iter().chain(SOURCES).map(|(p,s)|(p.to_string(),s.to_string())).collect(),true)
}
pub fn query() -> Value {
    json!({"query":"payment","limit":2,"perFileLimit":200,"graphWeight":0,"relativePath":"packages","pathGlob":"**/*.rs","includeBody":true})
}
pub fn state(engine:&Engine)->(i64,Vec<String>) {
    let changes=engine.store.db.query_row("SELECT total_changes()",[],|r|r.get(0)).unwrap();
    let mut blobs=fs::read_dir(engine.store.dir.join("blobs")).unwrap().map(|e|e.unwrap().file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
    blobs.sort(); (changes,blobs)
}
pub fn padded_json(json:&str,bytes:usize)->String {
    assert!(json.len()<=bytes); let mut out=json.to_string(); out.extend(std::iter::repeat_n(' ',bytes-json.len())); out
}
pub fn eight_package_markers()->BTreeMap<String,String> {
    let mut texts=BTreeMap::from([("package.json".into(),padded_json("{\\"workspaces\\":[\\"*\\"]}",800000))]);
    for s in ["a","b","c","d","e","f","g","h"] {
        texts.insert(format!("{s}/package.json"),padded_json(&format!("{{\\"name\\":\\"{s}\\"}}"),400000));
    }
    assert_eq!(texts.values().map(String::len).sum::<usize>(),4_000_000); texts
}
pub fn function_body(name:&str,bytes:usize)->String {
    let prefix=format!("fn {name}(){{ payment(); /*"); let suffix="*/ }";
    assert!(prefix.len()+suffix.len()<=bytes);
    format!("{prefix}{}{suffix}"," ".repeat(bytes-prefix.len()-suffix.len()))
}
pub fn nested_body(parent:&str,child:&str,bytes:usize)->String {
    let head=format!("fn {parent}(){{ payment(); /*");
    let mut out=format!("{head}{}*/", " ".repeat(100-head.len()-2));
    assert_eq!(out.len(),100);
    let nested=function_body(child,1_000_000); out.push_str(&nested);
    out.push_str("/*"); out.push_str(&" ".repeat(bytes-out.len()-4)); out.push_str("*/ }");
    assert_eq!(out.len(),bytes); out
}
'''
(support/'monorepo_retrieval_fixture.rs').write_text(header,encoding='utf-8')

source=(root/'native/src/retrieval.rs').read_text(encoding='utf-8')
assert source.count('pub fn find(\n')==1
entry='''// TEST/STUB draft: legacy global body below is preserved byte-for-byte.
#[path = "retrieval_balanced.rs"]
mod balanced;
use crate::retrieval_scopes::RetrievalHardError;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum NativeScopeMode { Global, ManifestBalancedV1 }
pub(crate) fn native_scope_mode(args:&Value)->std::result::Result<NativeScopeMode,RetrievalHardError> {
    match args.get("scopeMode") {
        None => Ok(NativeScopeMode::Global),
        Some(Value::String(s)) if s=="global" => Ok(NativeScopeMode::Global),
        Some(Value::String(s)) if s=="manifest_balanced_v1" => Ok(NativeScopeMode::ManifestBalancedV1),
        _ => Err(RetrievalHardError::Internal("implementation_unavailable")),
    }
}
pub fn find(store:&Store,snapshot:&Snapshot,graph:&Graph,args:&Value,deadline:&Deadline)->Result<Value> {
    match native_scope_mode(args)? {
        NativeScopeMode::Global => {
            if args.get("scopeMode").is_none() { return find_global_legacy(store,snapshot,graph,args,deadline); }
            let mut forwarded=args.as_object().context("Retrieval options must be an object")?.clone();
            forwarded.remove("scopeMode");
            find_global_legacy(store,snapshot,graph,&Value::Object(forwarded),deadline)
        }
        NativeScopeMode::ManifestBalancedV1 => anyhow::bail!("Manifest-balanced retrieval implementation_unavailable"),
    }
}
pub(crate) fn find_compatibility_auto(
    _store:&Store,_snapshot:&Snapshot,_graph:&Graph,_args_without_scope_mode:&Value,
    _deadline:&Deadline,_output_limit:usize,
)->Result<Value> {
    anyhow::bail!("Compatibility automatic retrieval implementation_unavailable")
}
// Opaque options draft. No constructor may label unchecked request data valid.
struct CheckedRetrievalOptions { _private: () }
fn checked_retrieval_options(
    _args:&Value,_deadline:&Deadline,
)->std::result::Result<CheckedRetrievalOptions,RetrievalHardError> {
    Err(RetrievalHardError::Internal("implementation_unavailable"))
}
fn rank_balanced(
    _run:&mut crate::retrieval_scopes::CheckedRetrievalRun<'_, '_, '_>,
    _partition:&crate::retrieval_scopes::CheckedScopePartition<'_>,
    _options:&CheckedRetrievalOptions,
)->std::result::Result<Value,RetrievalHardError> {
    Err(RetrievalHardError::Internal("implementation_unavailable"))
}
fn rank_global_after_probe(
    _run:&mut crate::retrieval_scopes::CheckedRetrievalRun<'_, '_, '_>,
    _options:&CheckedRetrievalOptions,
    _fallback:&crate::retrieval_scopes::PolicyFallback,
)->std::result::Result<Value,RetrievalHardError> {
    Err(RetrievalHardError::Internal("implementation_unavailable"))
}
#[cfg(test)]
#[path = "retrieval_driver_tests.rs"]
mod driver_tests;
'''
draft=source.replace('pub fn find(\n',entry+'\nfn find_global_legacy(\n',1)
(here/'retrieval.rs').write_text(draft,encoding='utf-8')
body=source[source.index('pub fn find(\n'):].replace('pub fn find(\n','fn find_global_legacy(\n',1)
assert draft[draft.index('fn find_global_legacy(\n'):]==body
plan={'phase':'ignored tests/stubs only; not executed','baselineRetrievalSha256':hashlib.sha256((root/'native/src/retrieval.rs').read_bytes()).hexdigest(),
      'legacyBodyDraftEquality':'exact text after sole function-name replacement; no global arithmetic/body edits',
      'futurePromotion':{'retrieval.rs':'native/src/retrieval.rs','retrieval_balanced.rs':'native/src/retrieval_balanced.rs',
       'retrieval_balanced_tests.rs':'native/src/retrieval_balanced_tests.rs','monorepo_retrieval.rs':'native/tests/monorepo_retrieval.rs',
       'retrieval_driver_tests.rs':'native/src/retrieval_driver_tests.rs',
       'legacy_retrieval_oracle.rs':'native/tests/legacy_retrieval_oracle.rs','support/monorepo_retrieval_fixture.rs':'native/tests/support/monorepo_retrieval_fixture.rs'},
      'preFeatureOracle':'Root must first run legacy_retrieval_oracle against old production, freeze literal emitted hash, then promote new mode stubs. No invented golden.'}
(here/'PROMOTION.json').write_text(json.dumps(plan,indent=2)+'\n',encoding='utf-8')
print('Ignored drafts assembled; normal retrieval bytes unchanged:',plan['baselineRetrievalSha256'])
