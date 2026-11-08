$ErrorActionPreference = 'Stop'
$taskRoot = 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence'
$taskResult = Get-Content -LiteralPath (Join-Path $taskRoot 'validation\integration-20261018\arxiv-source\final\result.json') -Raw | ConvertFrom-Json
if (-not $taskResult.sourceUnchanged) { throw 'Final frozen test run is not complete and unchanged.' }
if (Get-Process cargo,rustc,clippy-driver -ErrorAction SilentlyContinue) { throw 'Cargo/compiler still active.' }
$taskResolvedRoot = (Resolve-Path -LiteralPath $taskRoot).ProviderPath
$taskCache = (Resolve-Path -LiteralPath (Join-Path $taskRoot 'target\debug\deps')).ProviderPath
if ($taskCache -ne (Join-Path $taskResolvedRoot 'target\debug\deps')) { throw 'Cache resolved outside exact intended workspace target.' }
foreach ($taskDir in @($taskResolvedRoot, (Join-Path $taskResolvedRoot 'target'), (Join-Path $taskResolvedRoot 'target\debug'), $taskCache)) {
    if (((Get-Item -LiteralPath $taskDir).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Linked cache ancestor.' }
}
$taskRows = @()
$taskFiles = @(Get-ChildItem -LiteralPath $taskCache -File -Filter '*.exe' | Where-Object { $_.LastWriteTimeUtc -lt [datetime]'2026-10-19T20:00:00Z' -and $_.Name -ne 'github_app_digest_service-1b96734347e70c9f.exe' } | Sort-Object LastWriteTimeUtc | Select-Object -First 20)
foreach ($taskFile in $taskFiles) {
    $taskFull = [IO.Path]::GetFullPath($taskFile.FullName)
    if ([IO.Path]::GetDirectoryName($taskFull) -ne $taskCache -or ($taskFile.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Unexpected cache member.' }
    $taskHash = (Get-FileHash -LiteralPath $taskFull -Algorithm SHA256).Hash.ToLowerInvariant()
    $taskDeleted = $false
    try { Remove-Item -LiteralPath $taskFull -ErrorAction Stop; $taskDeleted = $true } catch { }
    $taskRows += [ordered]@{name=$taskFile.Name;bytes=$taskFile.Length;sha256=$taskHash;lastWriteTimeUtc=$taskFile.LastWriteTimeUtc.ToString('o');deleted=$taskDeleted}
}
$taskProof = [ordered]@{scope='Disposable old test executables only; no sources, fixtures, logs or evidence removed';cache=$taskCache;afterFrozenFinal=$true;excludedKnownActiveHistoricalProgram=$true;files=$taskRows;deletedBytes=($taskRows | Where-Object deleted | Measure-Object bytes -Sum).Sum}
$taskOutput = Join-Path $taskRoot 'validation\integration-20261018\arxiv-source\inactive-test-cache-cleanup.json'
[IO.File]::WriteAllText($taskOutput, ($taskProof | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
[pscustomobject]@{deletedFiles=@($taskRows | Where-Object deleted).Count;deletedBytes=$taskProof.deletedBytes;freeBytes=(Get-PSDrive -Name C).Free} | ConvertTo-Json -Compress
