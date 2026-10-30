from pathlib import Path
temp=Path(__file__).parent
s=(temp/'ci-neo4j-baseline-r2.py').read_text(encoding='utf-8')
assert 'WHERE name= RETURN value' in s
s=s.replace('neo4j-publish/baseline-r2','neo4j-publish/baseline-r3').replace('WHERE name= RETURN value','WHERE name=$name RETURN value')
(temp/'ci-neo4j-baseline-r3.py').write_text(s,encoding='utf-8')
print('Corrected the test-script parameter stripped by PowerShell interpolation; previous attempt retained.')
