$ErrorActionPreference = 'Stop'
$ownedNative = @(Get-CimInstance Win32_Process -Filter "Name='python.exe'" |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains('acceptance.py native --tag native-wave2-1440') })
if ($ownedNative.Count -eq 0) {
    Write-Output 'The owned native acceptance process has already ended.'
    exit 0
}
if ($ownedNative.Count -ne 1) {
    throw 'The exact owned acceptance process is ambiguous; no process was stopped.'
}
$ownedNativePid = [int]$ownedNative[0].ProcessId
Write-Output "Stopping only owned native-wave2-1440 acceptance tree at PID $ownedNativePid after verified StorageFull fixture failures."
& taskkill.exe /PID $ownedNativePid /T /F
if ($LASTEXITCODE -ne 0) { throw 'Owned acceptance process termination failed.' }
