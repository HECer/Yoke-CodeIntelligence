from pathlib import Path
wt=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach');p=wt/'native/src/host_output_recovery_tests.rs';s=p.read_text(encoding='utf-8');start=s.index('    Selection::new(',s.index('fn selection_for_count('));end=s.index('\n}',start)
old=s[start:end]
new='''    let roots=(0..count).map(|index|fixture_target(&root,index)).collect::<Vec<_>>();
    match fs::read(root.join("codex-profile")) {
        Ok(marker)=>{
            ensure!(marker==b"codex-mcp-v1","Unknown owned recovery fixture profile");
            Selection::codex_mcp("actual-output-interruption",&roots,&root.join("native.exe"),&root.join("policy.json"),&root.join("state"),&root.join("source"),d)
        },
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>Selection::new("actual-output-interruption",&roots,&root.join("native.exe"),&root.join("policy.json"),&root.join("state"),d),
        Err(error)=>Err(error).context("Cannot read owned fixture profile marker"),
    }'''
assert '"actual-output-interruption"' in old;s=s[:start]+new+s[end:]
s+='''

fn new_codex_fixture()->Result<Interrupted>{
    let source="# retained Codex human bytes \\u{1f4a1}\\r\\n[mcp_servers.human]\\r\\ncommand=\\"unrelated\\"\\r\\n".as_bytes();
    let mut fixture=new_fixture(source,1)?;
    let root=fixture.fixture.as_ref().context("Owned fixture missing")?.path().canonicalize()?;
    fs::rename(fixture_target(&root,0).join("AGENTS.md"),fixture_target(&root,0).join("config.toml"))?;
    fs::create_dir(root.join("source"))?;fs::write(root.join("codex-profile"),b"codex-mcp-v1")?;
    let d=Deadline::new(30000);fixture.selection=selection_for_count(&root,1,&d)?;fixture.plan=plan_install(&fixture.selection,1000,&d)?;Ok(fixture)
}
#[test]
fn codex_actual_partial_process_exit_resumes_the_same_owned_toml_output()->Result<()>{
    let mut fixture=new_codex_fixture()?;interrupt_worker(&mut fixture,"partial",Termination::Exit,false)?;
    let receipt=fixture.recover(RecoveryMode::Resume)?;ensure!(receipt.profile==adapter::CODEX_PROFILE&&receipt.status=="installed","Codex recovery receipt mismatch");
    let bytes=fs::read(fixture.target()?)?;ensure!(bytes.starts_with(&fixture.source)&&std::str::from_utf8(&bytes)?.contains("[mcp_servers.yoke_actual-output-interruption]"),"Codex recovery changed human bytes or lost managed table");
    absent(&fixture.target()?.with_file_name("AGENTS.md"))?;Ok(())
}
#[test]
fn codex_actual_prepared_kill_rolls_back_original_toml_identity_and_bytes()->Result<()>{
    let mut fixture=new_codex_fixture()?;let before=io::entry_info(&fixture.target()?,&Deadline::new(10000))?.context("Original missing")?.identity;
    interrupt_worker(&mut fixture,"prepared",Termination::Kill,false)?;
    let receipt=fixture.recover(RecoveryMode::Rollback)?;ensure!(receipt.profile==adapter::CODEX_PROFILE&&receipt.status=="rolled_back","Codex rollback receipt mismatch");
    ensure!(fs::read(fixture.target()?)?==fixture.source&&io::entry_info(&fixture.target()?,&Deadline::new(10000))?.context("Original missing")?.identity==before,"Codex rollback replaced original object");absent(&fixture.after()?)?;Ok(())
}
#[test]
fn codex_pending_recovery_refuses_a_replaced_source_directory_without_state_changes()->Result<()>{
    let mut fixture=new_codex_fixture()?;interrupt_worker(&mut fixture,"prepared",Termination::Exit,false)?;
    let root=fixture.fixture.as_ref().context("Owned fixture missing")?.path();let original=root.join("source");fs::rename(&original,root.join("moved-source"))?;fs::create_dir(&original)?;
    let before=state_manifest(&fixture.selection)?;let target_before=fs::read(fixture.target()?)?;
    ensure!(fixture.recover(RecoveryMode::Resume).is_err()&&fixture.recover(RecoveryMode::Rollback).is_err(),"Stale source root was accepted during Codex recovery");
    ensure!(state_manifest(&fixture.selection)?==before&&fs::read(fixture.target()?)?==target_before,"Failed stale Codex recovery mutated state");Ok(())
}
'''
p.write_text(s,encoding='utf-8');print('Added three Codex-specific actual process interruption/recovery cases; legacy fixture selection defaults unchanged')
