from pathlib import Path
p=Path(__file__).parent/'ci-session-pr.py';s=p.read_text(encoding='utf-8')
for old,new in [
    ('at0.3','at 0.3'),('native64,000-byte','native 64,000-byte'),
    ('has9default integration cases and1explicit actual gate','has 9 default integration cases and 1 explicit actual gate'),
    ('|15selected native groups|','|15 selected native groups|'),(' /{totals',' / {totals'),
    (';738inputs unchanged','; 738 inputs unchanged'),('TypeScript5.8.3/language-server5.1.3','TypeScript 5.8.3 / language-server 5.1.3'),
    ('Typecheck;260TypeScript tests /3existing skips;17Python tests pass','Typecheck; 260 TypeScript tests / 3 existing skips; 17 Python tests pass'),
    ('744inputs unchanged; all738native inputs','744 inputs unchanged; all 738 native inputs'),
    ('all60414prior body bytes','all 60414 prior body bytes'),('SHA-256`','SHA-256 `'),
    ('All353inventory identities,73unreconciled rows,77backend runtime unknowns','All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns')]:s=s.replace(old,new)
p.write_text(s,encoding='utf-8')
print('PR prose spacing normalized')
