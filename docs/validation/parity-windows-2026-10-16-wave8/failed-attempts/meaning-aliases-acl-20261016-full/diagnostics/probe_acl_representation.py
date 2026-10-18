"""Characterize independent Win32 ACL query representations on owned files."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
from pathlib import Path
import tempfile
import sys

use_kernel = "--kernel" in sys.argv

a = c.WinDLL("advapi32", use_last_error=True)
k = c.WinDLL("kernel32", use_last_error=True)
P = c.c_void_p
a.GetFileSecurityW.argtypes = [w.LPCWSTR,w.DWORD,P,w.DWORD,c.POINTER(w.DWORD)]
a.GetFileSecurityW.restype = w.BOOL
a.GetSecurityInfo.argtypes = [w.HANDLE,w.DWORD,w.DWORD,c.POINTER(P),c.POINTER(P),c.POINTER(P),c.POINTER(P),c.POINTER(P)]
a.GetSecurityInfo.restype = w.DWORD
a.GetKernelObjectSecurity.argtypes = [w.HANDLE,w.DWORD,P,w.DWORD,c.POINTER(w.DWORD)]
a.GetKernelObjectSecurity.restype = w.BOOL
a.GetSecurityDescriptorDacl.argtypes = [P,c.POINTER(w.BOOL),c.POINTER(P),c.POINTER(w.BOOL)]
a.GetSecurityDescriptorDacl.restype = w.BOOL
a.GetSecurityDescriptorControl.argtypes = [P,c.POINTER(w.WORD),c.POINTER(w.DWORD)]
a.GetSecurityDescriptorControl.restype = w.BOOL
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes=[w.LPCWSTR,w.DWORD,c.POINTER(P),c.POINTER(w.DWORD)]
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype=w.BOOL
a.SetFileSecurityW.argtypes=[w.LPCWSTR,w.DWORD,P]
a.SetFileSecurityW.restype=w.BOOL
a.SetSecurityInfo.argtypes=[w.HANDLE,w.DWORD,w.DWORD,P,P,P,P]
a.SetSecurityInfo.restype=w.DWORD
k.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,P,w.DWORD,w.DWORD,w.HANDLE]
k.CreateFileW.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE]
k.CloseHandle.restype=w.BOOL
k.LocalFree.argtypes=[P]
k.LocalFree.restype=P

def require(ok, message):
    if not ok:
        raise RuntimeError(message + ": " + str(c.get_last_error()))

def named(path):
    length=w.DWORD()
    a.GetFileSecurityW(str(path),4,None,0,c.byref(length))
    require(0 < length.value <= 1_048_576,"Descriptor length")
    buffer=c.create_string_buffer(length.value)
    require(a.GetFileSecurityW(str(path),4,buffer,len(buffer),c.byref(length)),"Named descriptor")
    return buffer

def handle_descriptor(handle,info=7):
    descriptor=P()
    status=a.GetSecurityInfo(handle,1,info,None,None,None,None,c.byref(descriptor))
    require(status==0,"Handle descriptor " + str(status))
    return descriptor

def kernel_descriptor(handle,info=7):
    length=w.DWORD()
    a.GetKernelObjectSecurity(handle,info,None,0,c.byref(length))
    require(0 < length.value <= 1_048_576,"Kernel descriptor length")
    buffer=c.create_string_buffer(length.value)
    require(a.GetKernelObjectSecurity(handle,info,buffer,len(buffer),c.byref(length)),"Kernel descriptor")
    return buffer

def state(descriptor):
    present=w.BOOL();defaulted=w.BOOL();dacl=P();control=w.WORD();revision=w.DWORD()
    require(a.GetSecurityDescriptorDacl(descriptor,c.byref(present),c.byref(dacl),c.byref(defaulted)),"DACL")
    require(a.GetSecurityDescriptorControl(descriptor,c.byref(control),c.byref(revision)),"Control")
    raw=b""
    if present.value and dacl.value:
        size=c.c_ushort.from_address(dacl.value+2).value
        require(8<=size<=65535,"ACL size")
        raw=c.string_at(dacl.value,size)
    return control.value & 0x150c,raw

def summary(item):
    control,raw=item
    count=int.from_bytes(raw[4:6],"little") if raw else 0
    end=8
    flags=[]
    for _ in range(count):
        require(end+4<=len(raw),"ACE header")
        size=int.from_bytes(raw[end+2:end+4],"little")
        require(size>=4 and end+size<=len(raw),"ACE size")
        flags.append(raw[end+1])
        end+=size
    return dict(control=control,aclBytes=len(raw),aceCount=count,usedAclBytes=end,aceFlags=flags,
                rawHash=hashlib.sha256(raw).hexdigest(),aceHash=hashlib.sha256(raw[8:end]).hexdigest())

class Attributes(c.Structure):
    _fields_=[("length",w.DWORD),("descriptor",P),("inherit",w.BOOL)]

results=[]
for drive in ("E","D","C"):
    scratch=Path(f"{drive}:/Yoke-Test-Scratch") if drive!="C" else Path(tempfile.gettempdir())
    scratch.mkdir(parents=True,exist_ok=True)
    for protected in (False,True):
        with tempfile.TemporaryDirectory(prefix="acl-representation-",dir=scratch) as temp:
            root=Path(temp).resolve()
            require(root.is_relative_to(scratch.resolve()),"Owned fixture root")
            original=root/"original.txt";output=root/"output.txt"
            original.write_bytes(b"owned probe")
            if protected:
                sd=P()
                require(a.ConvertStringSecurityDescriptorToSecurityDescriptorW("D:P(A;;FA;;;WD)",1,c.byref(sd),None),"Protected fixture descriptor")
                try:
                    require(a.SetFileSecurityW(str(original),4|0x80000000,sd),"Protected fixture ACL")
                finally:
                    k.LocalFree(sd)
            before=state(named(original))
            handle=k.CreateFileW(str(original),0x80000000|0x20000,7,None,3,128,None)
            require(handle not in (None,c.c_void_p(-1).value),"Open original")
            descriptor=handle_descriptor(handle)
            try:
                from_handle=state(descriptor)
                from_kernel=state(kernel_descriptor(handle))
                mask4=handle_descriptor(handle,4)
                try:
                    from_mask4=state(mask4)
                finally:
                    k.LocalFree(mask4)
                source_buffer=kernel_descriptor(handle) if use_kernel else None
                copied_descriptor=c.cast(source_buffer,P) if source_buffer is not None else descriptor
                expected=state(copied_descriptor)
                attributes=Attributes(c.sizeof(Attributes),copied_descriptor,False)
                created=k.CreateFileW(str(output),0x10000000,7,c.byref(attributes),1,128,None)
                require(created not in (None,c.c_void_p(-1).value),"Create output")
                try:
                    initial=handle_descriptor(created)
                    try:
                        initial_state=state(initial)
                    finally:
                        k.LocalFree(initial)
                    if (state(kernel_descriptor(created)) if use_kernel else initial_state)!=expected:
                        present=w.BOOL();defaulted=w.BOOL();dacl=P()
                        require(a.GetSecurityDescriptorDacl(copied_descriptor,c.byref(present),c.byref(dacl),c.byref(defaulted)),"Restore descriptor ACL")
                        status=a.SetSecurityInfo(created,1,4|(0x80000000 if expected[0]&0x1000 else 0x20000000),None,None,dacl,None)
                        require(status==0,"Restore output DACL")
                    after_descriptor=handle_descriptor(created)
                    try:
                        after_handle=state(after_descriptor)
                    finally:
                        k.LocalFree(after_descriptor)
                    after_named=state(named(output))
                    original_after=state(named(original))
                    results.append(dict(drive=drive,protected=protected,before=summary(before),originalHandle=summary(from_handle),originalKernel=summary(from_kernel),originalMask4=summary(from_mask4),
                                        outputHandle=summary(after_handle),outputNamed=summary(after_named),originalAfter=summary(original_after),
                                        handleEqual=after_handle==from_handle,namedEqual=after_named==before,
                                        rawDifferenceOffsets=[i for i,(x,y) in enumerate(zip(before[1],after_named[1])) if x!=y]))
                finally:
                    k.CloseHandle(created)
            finally:
                k.LocalFree(descriptor)
                k.CloseHandle(handle)
if "--compact" in sys.argv:
    print(json.dumps([dict(drive=r["drive"],protected=r["protected"],namedEqual=r["namedEqual"],before=r["before"],after=r["outputNamed"]) for r in results]))
else:
    print(json.dumps(results,indent=2))
