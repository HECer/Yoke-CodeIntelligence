from pathlib import Path
import json
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');work=root.parent/'Yoke-CI-r1-attach';base=root/'validation/integration-20261018/windows-process-exit';assert json.loads((base/'r3/result.json').read_bytes())['sourceUnchanged'];assert json.loads((base/'probe/result.json').read_bytes())['sourceUnchanged']
path=work/'native/src/process_windows_job.rs';s=path.read_text(encoding='utf-8');start=s.index('    #[cfg(test)]\n    fn trace_members(');end=s.index('    pub(super) fn new()',start);s=s[:start]+s[end:]
old='    #[cfg(test)]\n    fn GetProcessId(handle: Handle) -> u32;';assert old in s;s=s.replace(old,'    fn OpenProcess(access: u32, inherit: i32, id: u32) -> Handle;')
start=s.index('    pub(super) fn terminate_until(');end=s.index('\n}\n\n#[derive(Debug)]\nstruct Attach',start);s=s[:start]+Path(__file__).with_name('ci-process-exit-members.rs').read_text(encoding='utf-8')+s[end:];assert 'owned_job_probe' not in s;path.write_text(s,encoding='utf-8',newline='\n');print('Replaced temporary tracing with bounded held-member exit proof; termination is requested even when member capture refuses.')
