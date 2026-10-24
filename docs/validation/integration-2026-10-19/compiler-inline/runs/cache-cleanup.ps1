$ErrorActionPreference = 'Stop'
$ciWorkspace = 'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence'
$ciExpected = Join-Path $ciWorkspace 'target\debug\deps'
$ciCacheRoot = (Resolve-Path -LiteralPath $ciExpected).ProviderPath.TrimEnd('\')
if ($ciCacheRoot -ne $ciExpected -or -not $ciCacheRoot.StartsWith($ciWorkspace + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Cache path escapes the explicit workspace target' }
if (Get-CimInstance Win32_Process -Filter "name = 'cargo.exe' OR name = 'rustc.exe'") { throw 'Cargo/Rust compilation is still active' }
$ciAncestor = Get-Item -LiteralPath $ciCacheRoot
while ($ciAncestor.FullName.Length -ge $ciWorkspace.Length) {
    if (($ciAncestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Cache ancestor is a reparse point' }
    $ciAncestor = $ciAncestor.Parent
}
$ciProcesses = @(Get-CimInstance Win32_Process)
$ciLoadedPaths = @($ciProcesses | Where-Object ExecutablePath | ForEach-Object { $_.ExecutablePath })
$ciLoadedNames = @($ciProcesses | ForEach-Object { $_.Name })
$ciKeep = '^(yoke|compiler_inline|projects|project_session|agent_repl|semantic_refactor|refactor_intents|session_refactor|symbol_move_own_v1|symbol_move_effect_admission|facade_file_move|facade_directory_move|digest_history_jobs)(-|\.|$)'
$ciOut = Join-Path $ciWorkspace 'validation\integration-20261018\compiler-inline'
$ciReceipt = Join-Path $ciOut 'cache-cleanup.json'
if (Test-Path -LiteralPath $ciReceipt) { throw 'Cache receipt already exists' }
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $ciOut 'cache-cleanup.ps1')
$ciBefore = (Get-PSDrive C).Free
$ciRemoved = @()
try {
    $ciCandidates = @(Get-ChildItem -LiteralPath $ciCacheRoot -File | Where-Object { $_.Extension -in '.exe','.pdb' -and $_.Name -notmatch $ciKeep } | Sort-Object Length -Descending)
    foreach ($ciFile in $ciCandidates) {
        if ((Get-PSDrive C).Free -ge 12000000000) { break }
        if ($ciLoadedPaths -contains $ciFile.FullName -or $ciLoadedNames -contains ($ciFile.BaseName + '.exe')) { continue }
        if ($ciFile.DirectoryName -ne $ciCacheRoot -or ($ciFile.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Cache entry path/type changed' }
        $ciHash = (Get-FileHash -LiteralPath $ciFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $ciItem = [ordered]@{ path=$ciFile.FullName; bytes=$ciFile.Length; sha256=$ciHash }
        Remove-Item -LiteralPath $ciFile.FullName
        $ciRemoved += $ciItem
    }
} finally {
    $ciResult = [ordered]@{ scope='Nonrecursive disposable test .exe/.pdb cache only; selected targets and loaded/held images excluded'; target=$ciCacheRoot; freeBefore=$ciBefore; freeAfter=(Get-PSDrive C).Free; grossRemovedBytes=($ciRemoved | Measure-Object -Property bytes -Sum).Sum; files=$ciRemoved }
    $ciResult | ConvertTo-Json -Depth 5 | Out-File -LiteralPath $ciReceipt -Encoding utf8
}
Write-Output ($ciResult | Select-Object scope,freeBefore,freeAfter,grossRemovedBytes | ConvertTo-Json -Compress)
