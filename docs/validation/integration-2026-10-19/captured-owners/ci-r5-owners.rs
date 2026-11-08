//! Authored captured ownership candidates. Legacy publications stay unchanged.
use super::areas;
use crate::{capture::Deadline,model::{Snapshot,valid_id},store::Store};
use anyhow::{Result,bail};
use regex::{Regex,RegexBuilder};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};

const LOCATIONS:[&str;3]=[".github/CODEOWNERS","CODEOWNERS","docs/CODEOWNERS"];
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Limits {
    pub max_rule_bytes:usize,pub max_rules:usize,pub max_owner_tokens:usize,
    pub max_matches:usize,pub max_match_work:usize,pub max_output_bytes:usize,
}
impl Default for Limits {fn default()->Self{Self{max_rule_bytes:2_000_000,max_rules:4096,max_owner_tokens:16384,max_matches:65536,max_match_work:32_000_000,max_output_bytes:4_000_000}}}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Options{pub areas:areas::Options,pub limits:Limits}
fn input(ok:bool)->Result<()>{if !ok{bail!("git_owners_input_invalid")}Ok(())}
fn cap(ok:bool)->Result<()>{if !ok{bail!("git_owners_capacity_exceeded")}Ok(())}
fn check(d:&Deadline)->Result<()>{d.check().map_err(|_|anyhow::anyhow!("git_owners_deadline_exceeded"))}
fn rows<'a>(v:&'a Value,k:&str)->Result<&'a [Value]>{v[k].as_array().map(Vec::as_slice).ok_or_else(||anyhow::anyhow!("git_owners_source_refused"))}
fn text<'a>(v:&'a Value,k:&str)->Result<&'a str>{v[k].as_str().ok_or_else(||anyhow::anyhow!("git_owners_source_refused"))}
fn charge(n:&mut usize,amount:usize,max:usize)->Result<()>{*n=n.checked_add(amount).ok_or_else(||anyhow::anyhow!("git_owners_capacity_exceeded"))?;cap(*n<=max)}
impl Options {
    pub fn parse(bytes:&[u8])->Result<Self>{crate::json::parse(bytes,8192).and_then(crate::json::decode).map_err(|_|anyhow::anyhow!("git_owners_input_invalid"))}
    pub fn admit(&self,id:&str,d:&Deadline)->Result<()>{
        check(d)?;input(valid_id(id))?;self.areas.admit(id,&[],d)?;
        let h=Limits::default();let l=&self.limits;
        input(l.max_rule_bytes<=h.max_rule_bytes&&l.max_rules<=h.max_rules&&l.max_owner_tokens<=h.max_owner_tokens&&l.max_matches<=h.max_matches&&l.max_match_work<=h.max_match_work&&l.max_output_bytes<=h.max_output_bytes)
    }
}
#[derive(Default,Serialize)]
#[serde(rename_all="camelCase")]
struct Used{rule_bytes:usize,rules:usize,owner_tokens:usize,matches:usize,match_work:usize,reserved_output_bytes:usize}
fn reserve(v:&impl Serialize,used:&mut Used,l:&Limits,d:&Deadline)->Result<()>{
    check(d)?;let remaining=l.max_output_bytes.saturating_sub(used.reserved_output_bytes);
    let encoded=crate::json::encode_bounded(v,remaining,d);check(d)?;
    let bytes=encoded.map_err(|_|anyhow::anyhow!("git_owners_capacity_exceeded"))?;
    charge(&mut used.reserved_output_bytes,bytes.len(),l.max_output_bytes)
}
struct Rule{record:Value,matcher:Option<Regex>,pattern_bytes:usize}
struct Source{record:Value,rules:Vec<Rule>,unsupported:bool}

// Finite, independently owned path-pattern policy, not GitHub implementation code.
// A slashful wildcard terminal matches full paths only (docs/* is nonrecursive).
// Literal terminals and explicit directories can include descendants.
fn matcher(pattern:&str)->Result<Option<Regex>>{
    if pattern.len()>1024{return Ok(None)}
    if pattern.starts_with('!')||pattern.contains(['[',']','\\','{','}',':'])||pattern.chars().any(char::is_control)||pattern.contains('\u{feff}') {return Ok(None)}
    let anchored=pattern.starts_with('/');let p=pattern.strip_prefix('/').unwrap_or(pattern);
    let directory=p.ends_with('/');let p=p.strip_suffix('/').unwrap_or(p);
    if p.is_empty(){return Ok(None)}
    let segments:Vec<_>=p.split('/').collect();
    if segments.iter().any(|s|s.is_empty()||*s=="."||*s==".."||(s.contains("**")&&*s!="**")){return Ok(None)}
    let mut re=String::from("^");
    if !anchored&&!p.contains('/'){re.push_str("(?:.*/)?");}
    for (i,segment) in segments.iter().enumerate(){
        if *segment=="**" {if i+1==segments.len(){re.push_str(".*")}else{re.push_str("(?:[^/]+/)*")}continue;}
        for c in segment.chars(){match c{'*'=>re.push_str("[^/]*"),'?'=>re.push_str("[^/]"),_=>re.push_str(&regex::escape(&c.to_string()))}}
        if i+1<segments.len(){re.push('/');}
    }
    let literal_terminal=segments.last().is_some_and(|s|!s.contains(['*','?']));
    if directory{re.push_str("/.*")}else if literal_terminal||(!anchored&&!p.contains('/')){re.push_str("(?:/.*)?")}
    re.push('$');
    Ok(Some(RegexBuilder::new(&re).size_limit(1_000_000).dfa_size_limit(1_000_000).build().map_err(|_|anyhow::anyhow!("git_owners_capacity_exceeded"))?))
}
fn owner_label(token:&str)->bool{
    if token.is_empty()||token.len()>256||!token.is_ascii(){return false;}
    if let Some(label)=token.strip_prefix('@'){
        let parts:Vec<_>=label.split('/').collect();return (1..=2).contains(&parts.len())&&parts.iter().all(|p|!p.is_empty()&&p.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_')));
    }
    let parts:Vec<_>=token.split('@').collect();parts.len()==2&&parts.iter().all(|p|!p.is_empty())&&token.bytes().all(|b|b.is_ascii_alphanumeric()||b"._+-@".contains(&b))
}
fn tokens(line:&str)->Vec<(usize,&str)>{
    let mut result=Vec::new();let mut start=None;
    for (i,c) in line.char_indices(){
        if c.is_ascii_whitespace(){if let Some(begin)=start.take(){result.push((begin,&line[begin..i]));}}
        else if start.is_none(){start=Some(i);}
    }
    if let Some(begin)=start{result.push((begin,&line[begin..]));}result
}
fn parse_rules(source:&str,used:&mut Used,l:&Limits,d:&Deadline)->Result<Vec<Rule>>{
    let mut rules=Vec::new();let mut offset=0;
    for (line_index,line) in source.split_inclusive('\n').enumerate(){
        check(d)?;let start=offset;offset+=line.len();cap(line.len()<=4096)?;
        let parts=tokens(line);let Some((_,pattern))=parts.first() else{continue};
        if pattern.starts_with('#'){continue;}
        charge(&mut used.rules,1,l.max_rules)?;
        let mut labels=Vec::new();let mut owner_tokens=Vec::new();let mut supported=true;
        for (position,token) in parts.iter().skip(1){
            check(d)?;if token.starts_with('#'){break;}
            charge(&mut used.owner_tokens,1,l.max_owner_tokens)?;cap(labels.len()<50)?;
            supported &= owner_label(token);
            labels.push(*token);owner_tokens.push(json!({"label":token,"startByte":start+position,"endByte":start+position+token.len()}));
        }
        let compiled=if supported{matcher(pattern)?}else{None};supported &= compiled.is_some();
        rules.push(Rule{pattern_bytes:pattern.len(),matcher:compiled,record:json!({"ordinal":rules.len(),"line":line_index+1,"startByte":start,"endByte":offset,"quote":line,"pattern":pattern,"ownerLabels":labels,"ownerTokens":owner_tokens,"supported":supported,"unsupportedReason":if supported{Value::Null}else{json!("outside_captured_codeowners_path_rules_v1")}})});
    }
    check(d)?;Ok(rules)
}
fn source(store:&Store,snapshot:&Snapshot,side:&str,used:&mut Used,l:&Limits,d:&Deadline)->Result<Source>{
    check(d)?;
    let selected=LOCATIONS.iter().position(|path|snapshot.files.iter().any(|file|file.path==*path));
    let missing:Vec<_>=LOCATIONS.iter().take(selected.unwrap_or(LOCATIONS.len())).copied().collect();
    let Some(index)=selected else {
        return Ok(Source{rules:Vec::new(),unsupported:false,record:json!({"side":side,"snapshotId":snapshot.id,"sourcePath":null,"contentHash":null,
            "selection":"no_captured_rules","uncapturedHigherPriorityPaths":missing,"snapshotOmissionCount":snapshot.omissions.len(),"rules":[]})});
    };
    let path=LOCATIONS[index];let file=snapshot.files.iter().find(|f|f.path==path).ok_or_else(||anyhow::anyhow!("git_owners_source_refused"))?;
    cap(file.size<=1_000_000)?;charge(&mut used.rule_bytes,file.size,l.max_rule_bytes)?;
    let scope=crate::capture::source_scope_policy(store,snapshot)?;check(d)?;
    let bytes=scope.read_source(path)?;check(d)?;input(bytes.len()==file.size)?;
    let source=std::str::from_utf8(&bytes).map_err(|_|anyhow::anyhow!("git_owners_source_refused"))?;
    let rules=parse_rules(source,used,l,d)?;
    let unsupported=rules.iter().any(|r|r.matcher.is_none());
    let record=json!({"side":side,"snapshotId":snapshot.id,"snapshotKind":snapshot.kind,"sourcePath":path,"contentHash":file.hash,
        "sourceBytes":file.size,"selection":"first_captured_path_in_declared_order","uncapturedHigherPriorityPaths":missing,
        "snapshotOmissionCount":snapshot.omissions.len(),"unsupportedRulesPresent":unsupported,
        "rules":rules.iter().map(|r|&r.record).collect::<Vec<_>>()});
    Ok(Source{record,rules,unsupported})
}
pub fn derive(store:&Store,report:&str,d:&Deadline)->Result<Value>{derive_with_options(store,report,&Options::default(),d)}
pub fn derive_with_options(store:&Store,report:&str,o:&Options,d:&Deadline)->Result<Value>{
    let run=||->Result<Value>{
        o.admit(report,d)?;cap(o.limits.max_output_bytes>0)?;
        areas::derive_with(store,report,&[],&o.areas,d,|parent,snapshots|{
            let mut used=Used::default();charge(&mut used.reserved_output_bytes,parent.canonical_bytes().len(),o.limits.max_output_bytes)?;
            // Admit both physical selections before the first new source read.
            let mut physical=0usize;
            for snapshot in snapshots {
                check(d)?;
                if let Some(file)=LOCATIONS.iter().find_map(|path|snapshot.files.iter().find(|file|file.path==*path)){
                    cap(file.size<=1_000_000)?;charge(&mut physical,file.size,o.limits.max_rule_bytes)?;
                }
            }
            let sources=[source(store,&snapshots[0],"base",&mut used,&o.limits,d)?,source(store,&snapshots[1],"head",&mut used,&o.limits,d)?];
            let source_records:Vec<_>=sources.iter().map(|s|&s.record).collect();reserve(&source_records,&mut used,&o.limits,d)?;
            let mut ownership=Vec::new();
            for path in rows(parent.value(),"pathClassifications")? {
                check(d)?;let side=text(path,"side")?;
                let source_index=match side{"base"=>0,"head"=>1,_=>{input(false)?;unreachable!()}};
                let source=&sources[source_index];let path_text=text(path,"path")?;
                input(path["snapshotId"]==snapshots[source_index].id)?;
                let mut matching=Vec::new();
                for (i,rule) in source.rules.iter().enumerate(){
                    check(d)?;let Some(matcher)=&rule.matcher else{continue};
                    let cost=rule.pattern_bytes.max(1).checked_mul(path_text.len().max(1)).ok_or_else(||anyhow::anyhow!("git_owners_capacity_exceeded"))?;
                    charge(&mut used.match_work,cost,o.limits.max_match_work)?;
                    if matcher.is_match(path_text){charge(&mut used.matches,1,o.limits.max_matches)?;matching.push(i);}
                }
                let winner=if source.unsupported{None}else{matching.last().copied()};
                let labels=winner.map(|i|&source.rules[i].record["ownerLabels"]);
                let status=if source.record["sourcePath"].is_null(){"no_captured_rules"}else if source.unsupported{"unsupported_rules_present"}
                    else if labels.is_some_and(|v|v.as_array().is_some_and(Vec::is_empty)){"explicit_empty_owner_list"}else if winner.is_some(){"authored_owner_candidates"}else{"no_matching_supported_rule"};
                let winning=winner.map(|i|{
                    let mut r=source.rules[i].record.clone();r["sourceOrdinal"]=json!(source_index);r["ruleOrdinal"]=json!(i);r["sourcePath"]=source.record["sourcePath"].clone();
                    r["contentHash"]=source.record["contentHash"].clone();r["snapshotId"]=path["snapshotId"].clone();r
                });
                let row=json!({"classificationOrdinal":path["ordinal"],"side":side,"snapshotId":path["snapshotId"],"path":path_text,"contentHash":path["contentHash"],
                    "sourceOrdinal":source_index,"status":status,"ownerLabels":labels.unwrap_or(&Value::Null),"winningRule":winning,
                    "matchingSupportedRuleOrdinals":matching,"uncapturedPrecedenceMayDiffer":!rows(&source.record,"uncapturedHigherPriorityPaths")?.is_empty(),
                    "ownerIdentityVerified":false});
                reserve(&row,&mut used,&o.limits,d)?;ownership.push(row);
            }
            #[derive(Serialize)]
            #[serde(rename_all="camelCase")]
            struct Report<'a>{schema_version:u8,kind:&'static str,policy:&'static str,workspace_id:&'a str,report_artifact_id:&'a str,parent_result_id:&'a Value,
                impact_areas:&'a Value,rule_sources:Vec<&'a Value>,ownership:Vec<Value>,consumed:Used,options:&'a Options,
                state_written:bool,source_written:bool,git_subprocesses:u8,network_requests:u8,model_calls:u8,owner_identity_verified:bool,github_permission_verified:bool,
                source_validation:&'static str,complete:bool,limitations:[&'static str;5]}
            let report_value=Report{schema_version:1,kind:"native_git_captured_owners_v1",policy:"captured_codeowners_path_rules_v1",workspace_id:&store.workspace_id,report_artifact_id:report,
                parent_result_id:&parent.value()["resultId"],impact_areas:parent.value(),rule_sources:source_records,ownership,consumed:used,options:o,
                state_written:false,source_written:false,git_subprocesses:0,network_requests:0,model_calls:0,owner_identity_verified:false,github_permission_verified:false,
                source_validation:"checked_native_publication_and_captured_rule_bytes",complete:false,limitations:[
                    "Original impact/area evidence and legacy owner suggestions remain unchanged. New rows interpret authored captured path rules, not verified GitHub users/teams, permissions, actual review requirements or historical execution.",
                    "Rule selection is the first captured exact path in .github/CODEOWNERS, CODEOWNERS, docs/CODEOWNERS order. Uncaptured higher-precedence files or omitted sources may change actual repository behavior; no current file or remote fallback is used.",
                    "This independently owned finite pattern policy is not full GitHub parser/schema equivalence. Unsupported active rules remain explicit and make every path's winner indeterminate; supported matches remain available as candidates.",
                    "Simple unanchored patterns match path components; slashful wildcard terminals match complete paths; literal terminals and explicit directory rules include descendants. Double-star is supported only as a whole component. Matching is case sensitive over original Unicode text; question-mark matches one Unicode scalar.",
                    "Base/head, rule locations, original owner token positions, all supported matches and explicit empty-owner winners stay distinct. New quotas cover new rule interpretation/output, alongside the parent's separate replay/SQL/source limits and absolute host deadline. No ownership truth, merge safety, remote freshness, full allocation or upstream acceptance is implied."]};
            let encoded=crate::json::encode_bounded(&report_value,o.limits.max_output_bytes,d)?;check(d)?;
            let mut value=crate::json::parse(&encoded,o.limits.max_output_bytes)?;value["resultId"]=json!(crate::model::identity(&value));
            let encoded=crate::json::encode_bounded(&value,o.limits.max_output_bytes,d)?;check(d)?;
            crate::json::parse(&encoded,o.limits.max_output_bytes)
        })
    };
    run().map_err(|error|{
        if check(d).is_err(){return anyhow::anyhow!("git_owners_deadline_exceeded");}
        match error.to_string().as_str(){
            "git_owners_input_invalid"=>anyhow::anyhow!("git_owners_input_invalid"),
            "git_owners_capacity_exceeded"|"git_areas_capacity_exceeded"|"JSON output exceeds byte capacity"=>anyhow::anyhow!("git_owners_capacity_exceeded"),
            _=>anyhow::anyhow!("git_owners_source_refused")
        }
    })
}
