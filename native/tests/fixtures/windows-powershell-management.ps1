# Dependency of owned test payloads only; never a production shell prelude.
try {
    Import-Module -Name ($PSHOME+'\Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1') -ErrorAction Stop
} catch {
    [Console]::Error.WriteLine('Owned PowerShell Management module unavailable')
    exit 125
}
