# Dependency of owned test payloads only; never a production shell prelude.
try {
    Import-Module -Name ($PSHOME+'\Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
} catch {
    [Console]::Error.WriteLine('Owned PowerShell Utility module unavailable')
    exit 125
}
