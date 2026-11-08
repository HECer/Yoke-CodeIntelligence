"""Owned Windows inert stdin bridge probe; run from repository root.

No Yoke image is opened or executed. Each worker belongs to an assigned-before-
resume kill-on-close Job; host/settings/privileges remain unchanged.
"""
import base64
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def word(value):
    result, slashes = '"', 0
    for char in value:
        if char == '\\':
            slashes += 1
            continue
        result += '\\' * (slashes * 2 + 1 if char == '"' else slashes) + char
        slashes = 0
    return result + '\\' * (slashes * 2) + '"'

def literal(value):
    return "'" + value.replace("'", "''") + "'"

def parent():
    import _winapi
    class Basic(c.Structure):
        _fields_=[('process',c.c_int64),('job',c.c_int64),('flags',c.c_uint32),('min',c.c_size_t),
            ('max',c.c_size_t),('active',c.c_uint32),('affinity',c.c_size_t),('priority',c.c_uint32),('class_',c.c_uint32)]
    class Extended(c.Structure):
        _fields_=[('basic',Basic),('io',c.c_uint64*6),('process',c.c_size_t),('job',c.c_size_t),('peakp',c.c_size_t),('peakj',c.c_size_t)]
    assert c.sizeof(Extended)==144
    api=c.WinDLL('kernel32',use_last_error=True)
    for name,args,restype in [
        ('CreateJobObjectW',[c.c_void_p,c.c_wchar_p],c.c_void_p),
        ('SetInformationJobObject',[c.c_void_p,c.c_int,c.c_void_p,c.c_uint32],c.c_int),
        ('AssignProcessToJobObject',[c.c_void_p,c.c_void_p],c.c_int),
        ('QueryInformationJobObject',[c.c_void_p,c.c_int,c.c_void_p,c.c_uint32,c.c_void_p],c.c_int),
        ('ResumeThread',[c.c_void_p],c.c_uint32),('TerminateJobObject',[c.c_void_p,c.c_uint32],c.c_int),
        ('CloseHandle',[c.c_void_p],c.c_int)]:
        function=getattr(api,name);function.argtypes=args;function.restype=restype
    root=Path(tempfile.mkdtemp(prefix='yoke-owned-stdio-probe-')).resolve()
    assert root.is_relative_to(Path(tempfile.gettempdir()).resolve())
    job_name='Local\\YokeHistoryCI-'+os.urandom(32).hex()
    job=api.CreateJobObjectW(None,job_name);assert job
    limits=Extended();limits.basic.flags=0x2000
    process=thread=None
    assigned=False
    try:
        assert api.SetInformationJobObject(job,9,c.byref(limits),c.sizeof(limits))
        startup=subprocess.STARTUPINFO()
        environment=os.environ.copy();environment['OWNED_JOB']=job_name
        argv=[sys.executable,'-I','-B',str(Path(__file__).resolve()),'--worker',str(root)]
        process,thread,pid,_=_winapi.CreateProcess(None,subprocess.list2cmdline(argv),None,None,False,
            subprocess.CREATE_NO_WINDOW|4,environment,str(Path.cwd()),startup)
        assert api.AssignProcessToJobObject(job,process);assigned=True
        assert api.ResumeThread(thread)!=0xffffffff
        deadline=time.monotonic()+60
        while _winapi.WaitForSingleObject(process,100)==258:
            assert time.monotonic()<deadline,'Owned transport probe deadline'
        if _winapi.GetExitCodeProcess(process)!=0:
            print((root/'failure.json').read_text(encoding='utf-8'), flush=True)
            raise RuntimeError('Owned probe worker failed')
    finally:
        if process is not None and not assigned:
            _winapi.TerminateProcess(process,124)
        assert api.TerminateJobObject(job,125)
        deadline=time.monotonic()+10
        while True:
            info=c.create_string_buffer(48)
            assert api.QueryInformationJobObject(job,1,info,48,None)
            active=int.from_bytes(info.raw[40:44],'little')
            if not active or time.monotonic()>=deadline:break
            time.sleep(.05)
        assert active==0
        for handle in (thread,process,job):
            if handle is not None:assert api.CloseHandle(handle)
        (root/'cleanup.json').write_text(json.dumps({'activeAfterCleanup': active}), encoding='utf-8')
    report=json.loads((root/'probe.json').read_text(encoding='utf-8'))
    report.update(activeAfterCleanup=active,fixture=str(root))
    output=Path.cwd()/'validation/integration-20261018/status-stdin-20261024-2f5f1bc7/inert-probe.json'
    output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report,ensure_ascii=False,indent=2))

def worker(root):
    import importlib.util
    spec = importlib.util.spec_from_file_location('owned_job', Path.cwd() / 'scripts/host_history_live.py')
    owned = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owned)
    admission = owned.require_worker_job(os.environ['OWNED_JOB'])
    source = root / "source ' \u0130 \u03c0"
    source.mkdir()
    environment = {key: os.environ[key] for key in ['PATH', 'SystemRoot', 'SystemDrive', 'WINDIR'] if key in os.environ}
    for key, name in [('HOME','home'),('USERPROFILE','home'),('APPDATA','roaming'),
                      ('LOCALAPPDATA','local'),('TEMP','tmp'),('TMP','tmp')]:
        path = root / name
        path.mkdir(exist_ok=True)
        environment[key] = str(path)
    environment.update(CI='1', DO_NOT_TRACK='1', NO_COLOR='1')
    launch = json.dumps({'source': str(source), 'quote': 'e"x', 'trailing': 'C:\\owned\\',
        'volume': '\\\\?\\Volume{00000000-0000-0000-0000-000000000000}\\owned'}, ensure_ascii=False)
    code = "import sys,json,hashlib; data=sys.stdin.buffer.read(65537); sys.stderr.buffer.write(b'owned-diagnostic'); print(json.dumps({'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'argumentSha256':hashlib.sha256(sys.argv[1].encode()).hexdigest()}))"
    arguments = ' '.join(word(value) for value in ['-I', '-B', '-u', '-c', code, launch])
    shell = Path(environment['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe'
    modern = Path('C:/Program Files/PowerShell/7/pwsh.exe')
    cases = [(shell, 'inherited-stdin', size) for size in (0, 164, 65536)]
    cases += [(shell, 'unicode-bom-text-wrapper', 164), (shell, 'unicode-console-inherited-stdin', 164)]
    if modern.is_file():
        cases += [(modern, 'forced-bom-text-wrapper', 164), (modern, 'raw-handles', 164)]
    reports = []
    for host, mode, size in cases:
        payload = (b'\x00\x1a' + '\u03c0 e\u0301 \U0001f980'.encode() + bytes(range(256))) * 256
        payload = payload[:size]
        common = "$ErrorActionPreference='Stop'; "
        if mode.startswith('unicode'):
            # UTF-16 is a managed Console encoding; .NET Framework explicitly
            # avoids changing the process console code page for this encoding.
            common += "[Console]::InputEncoding=[Text.UnicodeEncoding]::new($false,$true); "
        common += "$p=$null; $started=$false; $code=1; try { $s=[System.Diagnostics.ProcessStartInfo]::new(); $s.FileName=" + literal(sys.executable) + "; $s.Arguments=" + literal(arguments) + "; $s.UseShellExecute=$false; $s.CreateNoWindow=$true; "
        if mode in ('inherited-stdin', 'unicode-console-inherited-stdin', 'raw-handles'):
            common += "$s.RedirectStandardInput=$false; $s.RedirectStandardOutput=$true; $s.RedirectStandardError=$true; $p=[System.Diagnostics.Process]::new(); $p.StartInfo=$s; if(-not $p.Start()){throw 'start'}; $started=$true; $out=[Console]::OpenStandardOutput(); $err=[Console]::OpenStandardError(); $o=$p.StandardOutput.BaseStream.CopyToAsync($out); $e=$p.StandardError.BaseStream.CopyToAsync($err); if(-not $p.WaitForExit(25000)){throw 'wait'}; if(-not $o.Wait(2000) -or -not $e.Wait(2000)){throw 'output'}; $out.Flush(); $err.Flush(); $code=$p.ExitCode "
        else:
            common += "$s.RedirectStandardInput=$true; $s.RedirectStandardOutput=$true; $s.RedirectStandardError=$true; "
            if host == modern:
                common += "$s.StandardInputEncoding=[Text.UTF8Encoding]::new($true); "
            common += "$p=[System.Diagnostics.Process]::new(); $p.StartInfo=$s; if(-not $p.Start()){throw 'start'}; $started=$true; $out=[Console]::OpenStandardOutput(); $err=[Console]::OpenStandardError(); $o=$p.StandardOutput.BaseStream.CopyToAsync($out); $e=$p.StandardError.BaseStream.CopyToAsync($err); $i=[Console]::OpenStandardInput().CopyToAsync($p.StandardInput.BaseStream); if(-not $i.Wait(20000)){throw 'input'}; $p.StandardInput.BaseStream.Close(); if(-not $p.WaitForExit(25000)){throw 'wait'}; if(-not $o.Wait(2000) -or -not $e.Wait(2000)){throw 'output'}; $out.Flush(); $err.Flush(); $code=$p.ExitCode "
        script = common + "} catch { [Console]::Error.WriteLine('owned_status_shell_refused') } finally { if($p){if($started -and -not $p.HasExited){$p.Kill(); $null=$p.WaitForExit(5000)}; $p.Dispose()} }; exit $code"
        started = time.monotonic()
        result = subprocess.run([str(host), '-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand',
            base64.b64encode(script.encode('utf-16-le')).decode()], input=payload, cwd=source,
            env=environment, capture_output=True, timeout=35, creationflags=subprocess.CREATE_NO_WINDOW)
        (root / 'latest-case.json').write_text(json.dumps({'mode': mode, 'bytes': len(payload),
            'exitCode': result.returncode, 'stdout': result.stdout.decode(errors='replace'),
            'stderr': result.stderr.decode(errors='replace')}), encoding='utf-8')
        assert result.returncode == 0 and len(result.stdout) <= 8192 and len(result.stderr) <= 8192
        value = json.loads(result.stdout)
        expected_payload = (b'\xef\xbb\xbf' if mode == 'forced-bom-text-wrapper' else b'\xff\xfe') + payload if mode in ('forced-bom-text-wrapper','unicode-bom-text-wrapper') else payload
        assert value['bytes'] == len(expected_payload)
        assert value['sha256'] == hashlib.sha256(expected_payload).hexdigest()
        assert value['argumentSha256'] == hashlib.sha256(launch.encode()).hexdigest()
        assert result.stderr == b'owned-diagnostic'
        reports.append({'host': host.name, 'mode': mode, 'inputBytes': len(payload),
                        'observedBytes': value['bytes'], 'exactOriginalInputBytes': expected_payload == payload,
                        'expectedByteBehaviorVerified': True, 'exactUnicodeQuotedArgument': True,
                        'exactIndependentStderr': True, 'exitCode': result.returncode,
                        'seconds': time.monotonic() - started})
    report = {'kind': 'owned-raw-handle-bridge-probe', 'cases': reports,
              'nativeYokeImageOpenedOrExecuted': False, 'jobAdmission': admission, 'modernShellAvailable': modern.is_file(), 'probeSourceSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'containmentHelperSha256': 'bc9629eda761d22fe5d273b5ac5238c96d483784b9e87002ea5016f08053c6ba'}
    (root / 'probe.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps(report, ensure_ascii=False), flush=True)

if sys.argv[1:2] == ['--worker']:
    try:
        worker(Path(sys.argv[2]))
    except Exception as error:
        directory = Path(sys.argv[2])
        failure = {'errorType': type(error).__name__, 'message': str(error)}
        if (directory / 'latest-case.json').is_file():
            failure['lastCase'] = json.loads((directory / 'latest-case.json').read_text())
        (directory / 'failure.json').write_text(json.dumps(failure), encoding='utf-8')
        raise
else:
    parent()
