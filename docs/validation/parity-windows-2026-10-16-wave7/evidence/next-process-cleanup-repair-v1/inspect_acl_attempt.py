"""Read-only inspection of the owned failed ACL fixture setup and native runner."""
import json
import ctypes
from ctypes import wintypes
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
command = r'''$ErrorActionPreference = 'Stop'
$runner = @(Get-CimInstance Win32_Process -Filter "Name='python.exe'" | Where-Object { $_.CommandLine -like '*acceptance_wave4.py native --tag affected-r5b-process-20261016-full*' } | Select-Object ProcessId, ParentProcessId, ExecutablePath, CommandLine)
[pscustomobject]@{Runner=$runner} | ConvertTo-Json -Depth 5 -Compress
'''
raw = subprocess.check_output(['powershell.exe', '-NoProfile', '-Command', command])
value = json.loads(raw.decode('utf-8-sig'))
security = ctypes.WinDLL('advapi32', use_last_error=True)
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
security.GetNamedSecurityInfoW.argtypes = [wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
security.GetNamedSecurityInfoW.restype = wintypes.DWORD
security.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes = [ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(wintypes.DWORD)]
security.ConvertSecurityDescriptorToStringSecurityDescriptorW.restype = wintypes.BOOL
kernel.LocalFree.argtypes = [ctypes.c_void_p]
kernel.LocalFree.restype = ctypes.c_void_p
acls = []
for path in ['C:/Yoke-Test-ACL', 'C:/Yoke-Test-ACL/affected-r5b-process-20261016-acl', 'C:/Users/HEC_e/AppData/Local/Temp']:
    descriptor = ctypes.c_void_p()
    code = security.GetNamedSecurityInfoW(path, 1, 7, None, None, None, None, ctypes.byref(descriptor))
    assert code == 0, (path, code)
    text = ctypes.c_void_p()
    try:
        assert security.ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, 1, 7, ctypes.byref(text), None)
        acls.append(dict(path=path, sddl=ctypes.wstring_at(text)))
    finally:
        if text.value:
            kernel.LocalFree(text)
        kernel.LocalFree(descriptor)
value['Acls'] = acls
(HERE / 'acl-attempt-inspection-v1.json').open('x', encoding='utf-8').write(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
print(json.dumps(value, ensure_ascii=False))
