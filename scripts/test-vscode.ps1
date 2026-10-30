param(
    [string]$CodeExecutable = "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe",
    [string]$NativeExecutable,
    [string]$ExtensionDirectory,
    [switch]$UseBundledRuntime,
    [int]$TimeoutSeconds = 180
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
if (!$ExtensionDirectory) { $ExtensionDirectory = Join-Path $taskRepo 'extensions\vscode' }
$ExtensionDirectory = (Resolve-Path -LiteralPath $ExtensionDirectory).Path
if ($UseBundledRuntime) { $NativeExecutable = Join-Path $ExtensionDirectory 'runtime\native\yoke.exe' }
if (!$NativeExecutable) { $NativeExecutable = Join-Path $taskRepo 'target\debug\yoke.exe' }
$NativeExecutable = (Resolve-Path -LiteralPath $NativeExecutable).Path
$CodeExecutable = (Resolve-Path -LiteralPath $CodeExecutable).Path
$taskFixture = Join-Path ([IO.Path]::GetTempPath()) ('yoke-vscode-' + [Guid]::NewGuid().ToString('N'))
$taskWorkspace = Join-Path $taskFixture 'workspace'
$taskProfile = Join-Path $taskFixture 'profile'
$taskExtensions = Join-Path $taskFixture 'extensions'
New-Item -ItemType Directory -Path $taskWorkspace, (Join-Path $taskProfile 'User'), $taskExtensions | Out-Null
[IO.File]::WriteAllText((Join-Path $taskWorkspace 'main.rs'), ('fn main() { println!("fixture"); }' + "`n"))
New-Item -ItemType Directory -Path (Join-Path $taskWorkspace '.vscode') | Out-Null
$taskUtf8 = [Text.UTF8Encoding]::new($false)
$taskWorkspaceSettings = @{ 'codeIntelligence.nativeExecutable' = (Join-Path $taskWorkspace 'must-not-run.exe'); 'codeIntelligence.runtime' = 'typescript' } | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $taskWorkspace '.vscode\settings.json'), $taskWorkspaceSettings, $taskUtf8)
$taskSettings = @{
    'security.workspace.trust.enabled' = $false
    'telemetry.telemetryLevel' = 'off'
    'update.mode' = 'none'
    'extensions.autoCheckUpdates' = $false
    'extensions.autoUpdate' = $false
    'workbench.startupEditor' = 'none'
    'window.restoreWindows' = 'none'
}
[IO.File]::WriteAllText((Join-Path $taskProfile 'User\settings.json'), ($taskSettings | ConvertTo-Json), $taskUtf8)
$taskResult = Join-Path $taskFixture 'result.json'
$taskOut = Join-Path $taskFixture 'stdout.log'
$taskErr = Join-Path $taskFixture 'stderr.log'
$taskPreviousNative = $env:YOKE_VSCODE_NATIVE
$taskPreviousResult = $env:YOKE_VSCODE_RESULT
$taskPreviousBundled = $env:YOKE_VSCODE_BUNDLED
$taskPreviousElectron = $env:ELECTRON_RUN_AS_NODE
$taskPreviousIpc = $env:VSCODE_IPC_HOOK_CLI
$taskProcess = $null
try {
    $env:YOKE_VSCODE_NATIVE = $NativeExecutable
    $env:YOKE_VSCODE_RESULT = $taskResult
    $env:YOKE_VSCODE_BUNDLED = if ($UseBundledRuntime) { '1' } else { '0' }
    $env:ELECTRON_RUN_AS_NODE = $null
    $env:VSCODE_IPC_HOOK_CLI = $null
    $taskArguments = @(
        '--new-window', '--wait', '--disable-extensions', '--disable-updates', '--skip-welcome',
        '--skip-release-notes', '--user-data-dir', $taskProfile, '--extensions-dir', $taskExtensions,
        '--extensionDevelopmentPath', $ExtensionDirectory,
        '--extensionTestsPath', (Join-Path $taskRepo 'extensions\vscode\test-host.cjs'), $taskWorkspace
    )
    # Quoted Windows arguments contain only resolved paths and fixed test flags.
    $taskArgumentLine = ($taskArguments | ForEach-Object { '"' + $_.Replace('"', '\"') + '"' }) -join ' '
    $taskProcess = Start-Process -FilePath $CodeExecutable -ArgumentList $taskArgumentLine -WindowStyle Hidden -PassThru -RedirectStandardOutput $taskOut -RedirectStandardError $taskErr
    $taskWatch = [Diagnostics.Stopwatch]::StartNew()
    while (!(Test-Path -LiteralPath $taskResult) -and $taskWatch.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if ($taskProcess.HasExited) { break }
        Start-Sleep -Milliseconds 250
    }
    if (!(Test-Path -LiteralPath $taskResult)) {
        throw "VS Code extension-host test produced no result. Inspect $taskOut and $taskErr"
    }
    $taskReport = Get-Content -Raw -LiteralPath $taskResult | ConvertFrom-Json
    if (!$taskReport.passed) { throw 'VS Code extension-host test failed' }
    Get-Content -Raw -LiteralPath $taskResult | Write-Output
} finally {
    $env:YOKE_VSCODE_NATIVE = $taskPreviousNative
    $env:YOKE_VSCODE_RESULT = $taskPreviousResult
    $env:YOKE_VSCODE_BUNDLED = $taskPreviousBundled
    $env:ELECTRON_RUN_AS_NODE = $taskPreviousElectron
    $env:VSCODE_IPC_HOOK_CLI = $taskPreviousIpc
    if ($taskProcess) {
        try {
            if (!$taskProcess.WaitForExit(5000)) {
                $taskCleanupOut = Join-Path $taskFixture 'cleanup.stdout.log'
                $taskCleanupErr = Join-Path $taskFixture 'cleanup.stderr.log'
                $taskCleanup = Start-Process -FilePath 'taskkill.exe' -ArgumentList @('/PID', $taskProcess.Id, '/T', '/F') -WindowStyle Hidden -PassThru -RedirectStandardOutput $taskCleanupOut -RedirectStandardError $taskCleanupErr
                try {
                    if (!$taskCleanup.WaitForExit(5000)) {
                        try { $taskCleanup.Kill() } catch { if (!$taskCleanup.HasExited) { throw } }
                        if (!$taskCleanup.WaitForExit(5000)) { throw 'Owned cleanup helper could not be stopped' }
                        throw 'Owned VS Code cleanup helper timed out'
                    }
                    if (!$taskProcess.WaitForExit(5000)) { throw 'Owned VS Code process cleanup failed' }
                } finally {
                    $taskCleanup.Dispose()
                }
            }
        } finally {
            $taskProcess.Dispose()
        }
    }
}
