$ErrorActionPreference='Stop'
if(Get-Process cargo,rustc,link -ErrorAction SilentlyContinue){throw 'Native compiler is active'}
$ownedRoot=[IO.Path]::GetFullPath('C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\target\debug\deps')
$ownedSource=Join-Path $ownedRoot 'github_app_history_bridge-8ed3bfb891b4baaa.exe'
$ownedTarget=$ownedSource+'.held-crash-26288'
foreach($candidate in @($ownedSource,$ownedTarget)) {
    $resolved=[IO.Path]::GetFullPath($candidate)
    if(-not $resolved.StartsWith($ownedRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Cache target escaped owned deps'}
}
$parent=Get-Item -LiteralPath $ownedRoot
while($null -ne $parent){if($parent.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked ancestor refused'};$parent=$parent.Parent}
$item=Get-Item -LiteralPath $ownedSource
if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked cache image refused'}
$record=@{source=$ownedSource;destination=$ownedTarget;bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $ownedSource -Algorithm SHA256).Hash.ToLowerInvariant();reason='Owned test target aborted after double panic; Windows retains PID26288 and denies termination. Relocate cache image without deleting it so the same target can be rebuilt.'}
$receipt='C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\app-history-integration\held-cache-relocation.json'
$bytes=[Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Depth 4))
$stream=[IO.FileStream]::new($receipt,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
Move-Item -LiteralPath $ownedSource -Destination $ownedTarget
if((Get-FileHash -LiteralPath $ownedTarget -Algorithm SHA256).Hash.ToLowerInvariant() -ne $record.sha256){throw 'Relocated cache hash mismatch'}
Write-Output 'Owned crashed test image preserved under a separate cache filename.'
