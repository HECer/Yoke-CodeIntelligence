#Requires -RunAsAdministrator
#Requires -Version 7.0
param(
    [Parameter(Mandatory=$true)][string]$Workspace,
    [Parameter(Mandatory=$true)][string]$Descriptor,
    [Parameter(Mandatory=$true)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
# Deliberately does not elevate itself or alter developer mode/user rights.
# The caller reviews this script and its exact frozen binary/case descriptor.
$gateRoot=(Resolve-Path -LiteralPath $Workspace).Path.TrimEnd('\')
if((Get-Item -LiteralPath $gateRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Workspace must be an unlinked directory'}
$gatePlan=Get-Content -LiteralPath $Descriptor -Raw -Encoding UTF8 | ConvertFrom-Json
if($gatePlan.schemaVersion -ne 1 -or $gatePlan.product -ne 'Code Intelligence' -or @($gatePlan.cases).Count -gt 32){throw 'Invalid gate descriptor'}
function Resolve-GatePath([string]$Relative){
    if([IO.Path]::IsPathRooted($Relative)){throw 'Gate paths must be relative'}
    $gatePath=[IO.Path]::GetFullPath((Join-Path $gateRoot $Relative))
    if(-not $gatePath.StartsWith($gateRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Gate path escapes workspace'}
    $gateCursor=$gatePath
    while($gateCursor -ne $gateRoot){
        if((Get-Item -LiteralPath $gateCursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Gate path contains a reparse point'}
        $gateCursor=[IO.Path]::GetDirectoryName($gateCursor)
    }
    return $gatePath
}
$gateManifestPath=Resolve-GatePath $gatePlan.sourceManifest
if((Get-FileHash -LiteralPath $gateManifestPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $gatePlan.sourceManifestSha256){throw 'Frozen source manifest differs'}
$gateApplication=Resolve-GatePath $gatePlan.applicationBinary
if((Get-FileHash -LiteralPath $gateApplication -Algorithm SHA256).Hash.ToLowerInvariant() -ne $gatePlan.applicationBinarySha256){throw 'Native CLI differs from frozen descriptor'}
$gateFrozen=Get-Content -LiteralPath $gateManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
if(@($gateFrozen.PSObject.Properties).Count -gt 2000){throw 'Source manifest exceeds gate bound'}
foreach($gateEntry in $gateFrozen.PSObject.Properties){
    if($gateEntry.Value -notmatch '^[0-9a-f]{64}$'){throw 'Invalid source hash'}
    if((Get-FileHash -LiteralPath (Resolve-GatePath $gateEntry.Name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $gateEntry.Value){throw ('Frozen source differs: '+$gateEntry.Name)}
}
$gateOutput=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $gateOutput){throw 'Choose a new output directory'}
[void][IO.Directory]::CreateDirectory($gateOutput)
Write-Output ('Code Intelligence '+$gatePlan.version+' Windows reparse prerequisites; frozen cases='+@($gatePlan.cases).Count)
$gateReceipts=@()
foreach($gateCase in $gatePlan.cases){
    if($gateCase.testName -notmatch '^[A-Za-z0-9_:]{1,200}$' -or $gateCase.binarySha256 -notmatch '^[0-9a-f]{64}$'){throw 'Invalid binary/case binding'}
    $gateBinary=Resolve-GatePath $gateCase.binary
    if((Get-FileHash -LiteralPath $gateBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $gateCase.binarySha256){throw 'Test binary differs from frozen descriptor'}
    $gateLog=Join-Path $gateOutput ($gateCase.testName.Replace(':','_')+'.log')
    $gateWatch=[Diagnostics.Stopwatch]::StartNew()
    $gateStart=[Diagnostics.ProcessStartInfo]::new($gateBinary)
    $gateStart.UseShellExecute=$false
    $gateStart.CreateNoWindow=$true
    $gateStart.RedirectStandardOutput=$true
    $gateStart.RedirectStandardError=$true
    foreach($gateArgument in @('--exact',$gateCase.testName,'--test-threads=1')){[void]$gateStart.ArgumentList.Add($gateArgument)}
    $gateProcess=[Diagnostics.Process]::Start($gateStart)
    $gateStdout=$gateProcess.StandardOutput.ReadToEndAsync()
    $gateStderr=$gateProcess.StandardError.ReadToEndAsync()
    $gateTimedOut=-not $gateProcess.WaitForExit(60000)
    if($gateTimedOut){
        $gateProcess.Kill($true)
        if(-not $gateProcess.WaitForExit(10000)){throw 'Owned gate process cleanup is unproven'}
    }
    $gateText=$gateStdout.GetAwaiter().GetResult()+$gateStderr.GetAwaiter().GetResult()
    $gateCode=$gateProcess.ExitCode
    $gateProcess.Dispose()
    if($gateTimedOut -or $gateText -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;'){$gateCode=1}
    [IO.File]::WriteAllText($gateLog,$gateText,[Text.UTF8Encoding]::new($false))
    $gateReceipts+=[pscustomobject]@{testName=$gateCase.testName;binary=$gateCase.binary;binarySha256=$gateCase.binarySha256;exitCode=$gateCode;timedOut=$gateTimedOut;elapsedSeconds=$gateWatch.Elapsed.TotalSeconds;log=[IO.Path]::GetFileName($gateLog)}
    [IO.File]::WriteAllText((Join-Path $gateOutput 'receipts.json'),($gateReceipts | ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
}
$gateFailed=@($gateReceipts | Where-Object exitCode -ne 0)
Write-Output ('Code Intelligence '+$gatePlan.version+' checked='+$gateReceipts.Count+' failed='+$gateFailed.Count)
if($gateFailed.Count -gt 0){exit 1}
