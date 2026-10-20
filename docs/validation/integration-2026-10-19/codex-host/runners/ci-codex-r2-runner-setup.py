from pathlib import Path
p=Path(__file__).parent;s=(p/'ci-codex-adapter-check-r1.py').read_text(encoding='utf-8').replace("codex-host/r1","codex-host/r2")
s=s.replace("if 'host_install' in name or name=='native/src/host_config.rs':", "if 'host_install' in name or 'host_output' in name or 'codex_host' in name or name=='native/src/host_config.rs':")
s=s.replace("('cli',[cargo,'build'", "('contracts',[cargo,'test',*common,'--no-fail-fast','--test','codex_host_cli','--test','host_install','--test','host_install_cli','--test','host_config','--test','host_output_disposition','--','--test-threads=1']),('cli',[cargo,'build'")
(p/'ci-codex-adapter-check-r2.py').write_text(s,encoding='utf-8')
