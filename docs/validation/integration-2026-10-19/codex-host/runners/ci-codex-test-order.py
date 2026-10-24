from pathlib import Path
import re
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach/native/src/host_install_adapter_tests.rs');s=p.read_text(encoding='utf-8');s=re.sub(r'apply\(\s*&selection,\s*&(\w+),\s*&authority,',r'apply(&selection,&authority,&\1,',s);s=re.sub(r'apply\(\s*&selection,\s*&(\w+),\s*&WriteAuthority::new\(&(\w+)\),',r'apply(&selection,&WriteAuthority::new(&\2),&\1,',s);p.write_text(s,encoding='utf-8')
