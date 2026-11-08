param(
    [string]$CodeExecutable = "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe",
    [string]$NativeExecutable,
    [string]$ExtensionDirectory,
    [switch]$UseBundledRuntime,
    [ValidateRange(1, 180)]
    [int]$TimeoutSeconds = 180,
    [string]$PythonExecutable = 'python'
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
$taskPythonMatches = @(Get-Command $PythonExecutable -CommandType Application -ErrorAction Stop)
$taskPython = $taskPythonMatches[0].Source
try {
    $env:YOKE_VSCODE_NATIVE = $NativeExecutable
    $env:YOKE_VSCODE_RESULT = $taskResult
    $env:YOKE_VSCODE_BUNDLED = if ($UseBundledRuntime) { '1' } else { '0' }
    $env:ELECTRON_RUN_AS_NODE = $null
    $env:VSCODE_IPC_HOOK_CLI = $null
    $taskSpec = Join-Path $taskFixture 'job-spec.json'
    $taskSelection = @{
        schemaVersion = 1
        fixtureRoot = $taskFixture
        program = $CodeExecutable
        repositoryRoot = $taskRepo
        extensionDirectory = $ExtensionDirectory
        timeoutSeconds = $TimeoutSeconds
    }
    [IO.File]::WriteAllText($taskSpec, ($taskSelection | ConvertTo-Json), $taskUtf8)
    & $taskPython -I (Join-Path $PSScriptRoot 'vscode_owned_job.py') --spec $taskSpec | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "VS Code owned process-tree proof failed. Inspect $(Join-Path $taskFixture 'job.json')" }
    if (!(Test-Path -LiteralPath $taskResult)) {
        throw "VS Code extension-host test produced no result. Inspect $taskOut and $taskErr"
    }
    $taskResultMetadata = Get-Item -LiteralPath $taskResult
    if (($taskResultMetadata.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $taskResultMetadata.Length -gt 65536) {
        throw 'VS Code extension-host result admission failed'
    }
    $taskReport = Get-Content -Raw -LiteralPath $taskResult | ConvertFrom-Json
    foreach ($taskFlag in @('passed', 'unsavedSourceBound', 'workspaceExecutableIgnored', 'protectedOpenBufferExcluded', 'savedSourceUnchanged')) {
        if ($taskReport.$taskFlag -isnot [bool] -or !$taskReport.$taskFlag) { throw 'VS Code extension-host test failed' }
    }
    if ($taskReport.kind -ne 'actual-vscode-extension-host' -or $taskReport.runtime -ne 'rust' -or
        $taskReport.bundledRuntime -isnot [bool] -or $taskReport.bundledRuntime -ne [bool]$UseBundledRuntime) {
        throw 'VS Code extension-host result scope differs'
    }
    Get-Content -Raw -LiteralPath $taskResult | Write-Output
} finally {
    $env:YOKE_VSCODE_NATIVE = $taskPreviousNative
    $env:YOKE_VSCODE_RESULT = $taskPreviousResult
    $env:YOKE_VSCODE_BUNDLED = $taskPreviousBundled
    $env:ELECTRON_RUN_AS_NODE = $taskPreviousElectron
    $env:VSCODE_IPC_HOOK_CLI = $taskPreviousIpc
}
