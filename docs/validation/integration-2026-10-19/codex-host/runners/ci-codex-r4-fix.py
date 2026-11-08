from pathlib import Path
import subprocess,sys
p=Path(__file__).parent;work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
for name in ['host_install_recovery.rs','host_install_recovery_v2.rs']:
 f=work/'native/src'/name;s=f.read_text(encoding='utf-8');assert s.count('    let authored = body(selection)?;')==1;s=s.replace('    let authored = body(selection)?;\n','');s=s.replace('let patch = host_config::agents_patch(\n            bytes,\n            (plan.operation == Operation::Install).then_some(authored.as_slice()),','let patch = adapter::patch(\n            selection,\n            bytes,\n            plan.operation == Operation::Install,');f.write_text(s,encoding='utf-8')
f=work/'native/src/host_install.rs';s=f.read_text(encoding='utf-8');assert s.count('    let authored = body(selection)?;')==1;s=s.replace('    let authored = body(selection)?;\n','');s=s.replace('let patch = host_config::agents_patch(bytes, Some(&authored), None, d)?;','let patch = adapter::patch(selection,bytes,true,None,d)?;');f.write_text(s,encoding='utf-8')
f=work/'native/src/host_install_adapter_tests.rs';s=f.read_text(encoding='utf-8');s+='''
#[test]
fn codex_initialization_repair_replays_the_canonical_toml_edit_without_touching_targets(){
    let (_dir,selection,root,source)=fixture();let d=Deadline::new(30000);let path=root.join("config.toml");let original=b"# existing human config\\r\\n";fs::write(&path,original).unwrap();let plan=plan_install(&selection,1000,&d).unwrap();let scaffold=state(&selection);fs::create_dir(&scaffold).unwrap();fs::write(scaffold.join("writer.lock"),[]).unwrap();for name in ["plans","receipts","journals"]{fs::create_dir(scaffold.join(name)).unwrap();}
    let authority=WriteAuthority::new(&selection);let token=initialization_token(&plan);let first=recover_initialization(&selection,&authority,&plan,&token,1001,&d).unwrap();let second=recover_initialization(&selection,&authority,&plan,&token,1002,&d).unwrap();assert_eq!(first,second);assert_eq!(fs::read(&path).unwrap(),original);assert!(!source.join(".yoke").exists());apply(&selection,&authority,&plan,1003,&d).unwrap();assert!(fs::read(path).unwrap().starts_with(original));
}
''';f.write_text(s,encoding='utf-8');subprocess.run([str(Path.home()/'.cargo/bin/cargo.exe'),'fmt','--all'],cwd=work,check=True)
s=(p/'ci-codex-adapter-check-r3.py').read_text(encoding='utf-8').replace('codex-host/r3','codex-host/r4');runner=p/'ci-codex-adapter-check-r4.py';runner.write_text(s,encoding='utf-8');subprocess.run([sys.executable,str(runner)],check=True)
