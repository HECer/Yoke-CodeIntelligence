from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-process-member-cache.ps1').read_text(encoding='utf-8')
s=s.replace('windows-process-exit\\final\\result.json','mixed-meaning-consumers\\final\\result.json')
s=s.replace("-Filter '*.exe'",'')
s=s.replace("$_.LastWriteTimeUtc -lt [datetime]'2026-10-19T20:00:00Z'", "$_.Extension -in @('.exe','.pdb') -and $_.LastWriteTimeUtc -lt [datetime]'2026-10-20T07:00:00Z'")
s=s.replace("$_.Name -ne 'github_app_digest_service-1b96734347e70c9f.exe'", "$_.BaseName -ne 'github_app_digest_service-1b96734347e70c9f' -and $_.BaseName -notin @($taskActive | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_) })")
s=s.replace('-First 40','-First 120').replace('maxCandidates=40','maxCandidates=120')
s=s.replace('windows-process-exit\\inactive-cache-cleanup-v2.jsonl','mixed-meaning-consumers\\inactive-cache-cleanup.jsonl')
s=s.replace('old inactive disposable test executables only','old inactive disposable test executables/PDBs only')
s=s.replace('1500MB','5000MB')
(t/'ci-consumers-cache.ps1').write_text(s,encoding='utf-8')
print('Prepared checked inactive target cache cleanup; not executed')
