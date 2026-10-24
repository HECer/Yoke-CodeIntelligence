from pathlib import Path
temp=Path.home()/'AppData/Local/Temp'
source=(temp/'ci-neo4j-baseline-r3.py').read_text().replace('neo4j-publish/baseline-r3','neo4j-publish/native-live-r1')
start=source.index("   marker='owned-'");end=source.index('  finally:\n   if child.poll()',start)
block=(temp/'ci-neo4j-native-live-block.txt').read_text()
source=source[:start]+block+'\n'+source[end:]
(temp/'ci-neo4j-native-live.py').write_text(source)
