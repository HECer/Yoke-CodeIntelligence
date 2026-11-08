from pathlib import Path
import py_compile
p=Path(__file__).parent;dest=p/'ci-falkor-native-live.py';s=dest.read_text(encoding='utf-8')
needle="password=secrets.token_urlsafe(32)"
start=s.index(needle)
block="""work=root.parent/'Yoke-CI-r1-attach'
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\\0')))
 return {n:sha((work/n).read_bytes()) for n in names if n and (work/n).is_file()}
source_before=inventory();(out/'source-before.json').write_text(json.dumps(source_before,indent=2)+'\\n',encoding='utf-8');binary_before=sha((root/'target/debug/yoke.exe').read_bytes())
"""
s=s[:start]+block+s[start:]
s+='\nsource_after=inventory();(out/"source-after.json").write_text(json.dumps(source_after,indent=2)+"\\n",encoding="utf-8");assert source_before==source_after;assert binary_before==sha((root/"target/debug/yoke.exe").read_bytes());(out/"frozen-inputs.json").write_text(json.dumps({"inputs":len(source_before),"sourceUnchanged":True,"binaryUnchanged":True,"binarySha256":binary_before})+"\\n",encoding="utf-8")\n'
s=s.replace("     removed=0;advapi=", "     (out/'responses-partial.json').write_text(json.dumps(responses,indent=2,ensure_ascii=False)+'\\n',encoding='utf-8')\n     removed=0;advapi=")
dest.write_text(s,encoding='utf-8');py_compile.compile(str(dest),doraise=True);print('Prepared frozen inputs and partial evidence capture')
