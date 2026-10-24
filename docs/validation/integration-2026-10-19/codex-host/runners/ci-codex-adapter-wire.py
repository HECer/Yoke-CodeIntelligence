from pathlib import Path
wt=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');p=wt/'native/src/host_install.rs';s=p.read_text(encoding='utf-8')
s=s.replace('mod recovery;','mod recovery;\n#[path="host_install_adapter.rs"]\nmod adapter;\npub use adapter::AdapterBinding;')
s=s.replace('    pub state_root: DirectoryBinding,\n}', '    pub state_root: DirectoryBinding,\n    #[serde(default,skip_serializing_if="Option::is_none")]\n    pub adapter: Option<AdapterBinding>,\n}')
s=s.replace('                roots,\n                executable,', '                roots,\n                adapter: None,\n                executable,')
needle='    pub fn installation_id(&self) -> String {\n';assert s.count(needle)==1
s=s.replace(needle,needle+'        if let Some(binding)=&self.bindings.adapter {\n            return identity(&(adapter::CODEX_PROFILE,&self.installation,&self.bindings.roots,&self.bindings.state_root,binding));\n        }\n')
s=s.replace('.join("AGENTS.md"))','.join(adapter::filename(selection)))')
s=s.replace('    let authored = body(selection)?;\n    let mut files', '    let mut files')
start=s.index('        let patch = host_config::agents_patch(',s.index('fn plan_for('));end=s.index('        let output = ',start)
s=s[:start]+'        let patch = adapter::patch(selection,bytes,operation==Operation::Install,owned,d)?;\n'+s[end:]
s=s.replace('path: "AGENTS.md".into(),','path: adapter::filename(selection).into(),')
s=s.replace('profile: PROFILE_ID.into(),','profile: adapter::profile(selection).into(),')
s=s.replace('receipt.profile == PROFILE_ID','receipt.profile == adapter::profile(selection)')
s=s.replace('file.path == "AGENTS.md"','file.path == adapter::filename(selection)')
s=s.replace('            && receipt.bindings.state_root == selection.bindings.state_root','            && receipt.bindings.state_root == selection.bindings.state_root\n            && receipt.bindings.adapter == selection.bindings.adapter')
# load_plan reconstructs the exact owner after this coarse closed-profile check.
s=s.replace('            && plan.profile == PROFILE_ID\n            && !plan.bindings.roots.is_empty()', '            && matches!(plan.profile.as_str(),PROFILE_ID|adapter::CODEX_PROFILE)\n            && !plan.bindings.roots.is_empty()')
s=s.replace('plan.profile == PROFILE_ID','plan.profile == adapter::profile(selection)')
s=s.replace('    Ok(Guards {\n        _directories: dirs,','    adapter::guard(selection,&mut dirs,&mut files,d)?;\n    Ok(Guards {\n        _directories: dirs,')
p.write_text(s,encoding='utf-8')
for name in ['host_install_recovery.rs','host_install_recovery_v2.rs']:
 p=wt/'native/src'/name;s=p.read_text(encoding='utf-8').replace('profile: PROFILE_ID.into(),','profile: adapter::profile(selection).into(),');p.write_text(s,encoding='utf-8')
p=wt/'native/src/host_install_cli.rs';s=p.read_text(encoding='utf-8');s=s.replace('    state_root: PathBuf,\n}', '    state_root: PathBuf,\n    #[serde(default)]\n    adapter: Option<AdapterSelection>,\n}\n#[derive(Deserialize)]\n#[serde(tag="kind",rename_all="snake_case",rename_all_fields="camelCase",deny_unknown_fields)]\nenum AdapterSelection { CodexMcpV1 { source_root: PathBuf } }')
old='''    let selection = Selection::new(
        &file.installation,
        &file.target_roots,
        &file.executable,
        &file.policy,
        &file.state_root,
        &deadline,
    )?;'''
assert old in s
new='''    let selection = if let Some(AdapterSelection::CodexMcpV1{source_root})=&file.adapter {
        Selection::codex_mcp(&file.installation,&file.target_roots,&file.executable,&file.policy,&file.state_root,source_root,&deadline)?
    }else {Selection::new(&file.installation,&file.target_roots,&file.executable,&file.policy,&file.state_root,&deadline)?};'''
s=s.replace(old,new).replace('.join("AGENTS.md")','.join(selection.target_filename())');p.write_text(s,encoding='utf-8')
print('Wired separate Codex adapter through existing guarded installer and recovery')
