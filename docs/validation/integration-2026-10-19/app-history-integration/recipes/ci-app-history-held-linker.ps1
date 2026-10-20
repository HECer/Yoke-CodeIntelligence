$ErrorActionPreference='Stop'
if(Get-Process cargo,rustc -ErrorAction SilentlyContinue){throw 'Native compiler is active'}
$ownedRoot=[IO.Path]::GetFullPath('C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\target\debug\deps')
$ownedSource=Join-Path $ownedRoot 'digest_history_jobs-2b5969e3eb9af212.exe'
$ownedTarget=$ownedSource+'.held-linker-22940'
foreach($candidate in @($ownedSource,$ownedTarget)) {
    $resolved=[IO.Path]::GetFullPath($candidate)
    if(-not $resolved.StartsWith($ownedRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Cache target escaped owned deps'}
}
$parent=Get-Item -LiteralPath $ownedRoot
while($null -ne $parent){if($parent.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked ancestor refused'};$parent=$parent.Parent}
$item=Get-Item -LiteralPath $ownedSource
if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked cache image refused'}
$record=@{source=$ownedSource;destination=$ownedTarget;bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $ownedSource -Algorithm SHA256).Hash.ToLowerInvariant();reason='MSVC linker aborted with LNK1000; Windows retains owned linker PID22940 and denies termination, causing LNK1104 on the next build. Preserve the incomplete cache output under a separate name.'}
$receipt='C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence\validation\integration-20261018\app-history-integration\held-linker-relocation.json'
$bytes=[Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Depth 4))
$stream=[IO.FileStream]::new($receipt,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try{$stream.Write($bytes,0,$bytes.Length);$stream.Flush($true)}finally{$stream.Dispose()}
Move-Item -LiteralPath $ownedSource -Destination $ownedTarget
if((Get-FileHash -LiteralPath $ownedTarget -Algorithm SHA256).Hash.ToLowerInvariant() -ne $record.sha256){throw 'Relocated cache hash mismatch'}
Write-Output 'Owned crashed test image preserved under a separate cache filename.'
