$ErrorActionPreference = 'Stop'
$taskRoot = 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence'
$taskResult = Get-Content -LiteralPath (Join-Path $taskRoot 'validation\integration-20261018\mixed-meaning-consumers\final\result.json') -Raw | ConvertFrom-Json
if (-not $taskResult.sourceUnchanged) { throw 'Frozen final run not complete.' }
if (Get-Process cargo,rustc,clippy-driver -ErrorAction SilentlyContinue) { throw 'Compiler still active.' }
$taskResolvedRoot = (Resolve-Path -LiteralPath $taskRoot).ProviderPath
$taskCache = (Resolve-Path -LiteralPath (Join-Path $taskRoot 'target\debug\deps')).ProviderPath
if ($taskCache -ne (Join-Path $taskResolvedRoot 'target\debug\deps')) { throw 'Unexpected resolved cache.' }
foreach ($taskDir in @($taskResolvedRoot,(Join-Path $taskRoot 'target'),(Join-Path $taskRoot 'target\debug'),$taskCache)) {
    if (((Get-Item -LiteralPath $taskDir).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Linked cache ancestor.' }
}
$taskActive = @(Get-Process | ForEach-Object { try { $_.Path } catch {} } | Where-Object { $_ })
$taskFiles = @(Get-ChildItem -LiteralPath $taskCache -File  | Where-Object { $_.Extension -in @('.exe','.pdb') -and $_.LastWriteTimeUtc -lt [datetime]'2026-10-20T07:00:00Z' -and $_.BaseName -ne 'github_app_digest_service-1b96734347e70c9f' -and $_.BaseName -notin @($taskActive | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_) }) -and $_.FullName -notin $taskActive } | Sort-Object LastWriteTimeUtc | Select-Object -First 120)
$taskReceipt = Join-Path $taskRoot 'validation\integration-20261018\mixed-meaning-consumers\inactive-cache-cleanup.jsonl'
$taskStream = [IO.File]::Open($taskReceipt,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
$taskWriter = [IO.StreamWriter]::new($taskStream,[Text.UTF8Encoding]::new($false))
function Write-TaskRecord($taskRecord) {
    $taskWriter.WriteLine(($taskRecord | ConvertTo-Json -Compress -Depth 6))
    $taskWriter.Flush()
    $taskStream.Flush($true)
}
$taskDeleted = 0
[long]$taskBytes = 0
try {
    Write-TaskRecord ([pscustomobject]@{kind='header';cache=$taskCache;scope='old inactive disposable test executables/PDBs only';nativeInputsFrozen=$taskResult.inputs;maxCandidates=120;plannedBytesFlushedBeforeDeletion=$true})
    foreach ($taskFile in $taskFiles) {
        $taskFull = [IO.Path]::GetFullPath($taskFile.FullName)
        if ([IO.Path]::GetDirectoryName($taskFull) -ne $taskCache -or ($taskFile.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Unexpected cache member.' }
        $taskHash = (Get-FileHash -LiteralPath $taskFull -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-TaskRecord ([pscustomobject]@{kind='planned';name=$taskFile.Name;bytes=$taskFile.Length;sha256=$taskHash;lastWriteTimeUtc=$taskFile.LastWriteTimeUtc.ToString('o')})
        $taskRemoved = $false
        try { Remove-Item -LiteralPath $taskFull -ErrorAction Stop; $taskRemoved = $true } catch {}
        if ($taskRemoved) { $taskDeleted++; $taskBytes += $taskFile.Length }
        Write-TaskRecord ([pscustomobject]@{kind='result';name=$taskFile.Name;deleted=$taskRemoved})
        if ($taskBytes -ge 5000MB) { break }
    }
    $taskSummary = [pscustomobject]@{kind='summary';deletedFiles=$taskDeleted;deletedBytes=$taskBytes;freeBytes=(Get-PSDrive -Name C).Free;sourceOrEvidenceRemoved=$false}
    Write-TaskRecord $taskSummary
    $taskSummary | ConvertTo-Json -Compress
} finally { $taskWriter.Dispose(); $taskStream.Dispose() }
