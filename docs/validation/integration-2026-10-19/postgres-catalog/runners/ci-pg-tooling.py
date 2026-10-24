from pathlib import Path,PurePosixPath
import urllib.request,hashlib,zipfile,json,shutil,time,subprocess,stat
base=(Path.home()/'AppData/Local/YokeValidation').resolve();cache=base/'postgres-investigation';cache.mkdir(exist_ok=True)
url='https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip';archive=cache/'postgresql-18.6-5-windows-x64-binaries.zip';destination=base/'postgres-18.6-5';assert destination.resolve().is_relative_to(base)
assert not destination.exists(), 'Inspect existing owned tool directory before reuse'
assert shutil.disk_usage(base).free>1_900_000_000
start=time.time();h=hashlib.sha256();total=0;report=0
with urllib.request.urlopen(url,timeout=30) as response,archive.open('xb') as output:
    assert response.geturl().startswith('https://get.enterprisedb.com/')
    while True:
        block=response.read(1_048_576)
        if not block:break
        total+=len(block);assert total<=450_000_000;h.update(block);output.write(block)
        if total-report>=64_000_000:report=total;print(json.dumps({'downloadedBytes':total}),flush=True)
assert total==384620317
entries=[];expanded=0
with zipfile.ZipFile(archive) as z:
    selected=[]
    for member in z.infolist():
        name=PurePosixPath(member.filename);assert not name.is_absolute() and '..' not in name.parts and '\\' not in member.filename
        if member.is_dir() or member.filename.endswith('.pdb'):continue
        if not member.filename.startswith(('pgsql/bin/','pgsql/lib/','pgsql/share/')):continue
        assert (member.external_attr>>16)&0o170000!=stat.S_IFLNK
        expanded+=member.file_size;assert expanded<=900_000_000;selected.append(member)
    assert len(selected)>20
    destination.mkdir()
    for member in selected:
        target=destination.joinpath(*PurePosixPath(member.filename).parts);assert target.resolve().is_relative_to(destination.resolve());target.parent.mkdir(parents=True,exist_ok=True)
        digest=hashlib.sha256();size=0
        with z.open(member) as source,target.open('xb') as output:
            while True:
                block=source.read(65536)
                if not block:break
                size+=len(block);assert size<=member.file_size;digest.update(block);output.write(block)
        assert size==member.file_size;entries.append({'path':member.filename,'bytes':size,'sha256':digest.hexdigest()})
print(json.dumps({'extractedFiles':len(entries),'expandedBytes':expanded}),flush=True)
versions={}
for name in ['postgres','psql','initdb','pg_ctl']:
    command=[str(destination/'pgsql/bin'/(name+'.exe')),'--version'];p=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15);assert p.returncode==0;versions[name]=p.stdout.decode().strip()
receipt={'sourcePage':'https://www.postgresql.org/download/windows/','vendorPage':'https://www.enterprisedb.com/download-postgresql-binaries','observedFileLink':'https://sbp.enterprisedb.com/getfile.jsp?fileid=1260609','url':url,'archiveBytes':total,'archiveSha256':h.hexdigest(),'vendorChecksumIndependentlyVerified':False,'selectedPrefixes':['pgsql/bin/','pgsql/lib/','pgsql/share/'],'excluded':['pgAdmin and other package directories','debug PDB files'],'versions':versions,'files':entries,'elapsedSeconds':round(time.time()-start,2)}
(cache/'tooling-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({'versions':versions,'archiveSha256':h.hexdigest(),'files':len(entries)}),flush=True)
