from pathlib import Path
p=Path(__file__).parent;s=(p/'ci-falkor-host-check.py').read_text(encoding='utf-8').replace("root=Path(r'C:\\Users\\Lomi\\Documents\\NN-Developed\\Yoke-CI-r1-attach');out=root.parent/'Yoke-CodeIntelligence/validation/integration-20261018/falkor-publish/host-r1'", "root=Path(r'C:\\Users\\Lomi\\Documents\\NN-Developed\\Yoke-CodeIntelligence');out=root/'validation/integration-20261018/falkor-publish/integrated'")
assert "out=root/'validation/integration-20261018/falkor-publish/integrated'" in s
s=s.replace("[('python-contracts'", "[('format',[str(Path.home()/'.cargo/bin/cargo.exe'),'fmt','--all','--check']),('python-contracts'")
(p/'ci-falkor-integrated.py').write_text(s,encoding='utf-8')
