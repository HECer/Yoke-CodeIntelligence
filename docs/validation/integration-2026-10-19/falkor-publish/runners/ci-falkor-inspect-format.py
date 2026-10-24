from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach/native/src/falkor_publish_plan.rs');s=p.read_text(encoding='utf-8');s=s.replace('"guardedPublicationPlatforms":["windows"]}),\n        ;','"guardedPublicationPlatforms":["windows"]});');p.write_text(s,encoding='utf-8')
