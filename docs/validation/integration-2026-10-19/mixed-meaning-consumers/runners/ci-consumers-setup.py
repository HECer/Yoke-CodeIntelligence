from pathlib import Path
t=Path(__file__).parent
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
s=(t/'ci-provider-check.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-provider','mixed-meaning-consumers').replace('START MIXED MEANING PROVIDER','START MIXED MEANING CONSUMERS')
s=s.replace("'mixed_view.rs',", "'mixed_view.rs','mixed_meaning_query.rs','mixed-meaning-queries.md',")
s=s.replace("['mixed_meaning_claims','meaning_build']", "['mixed_meaning_queries','mixed_meaning_provider']")
(t/'ci-consumers-check.py').write_text(s,encoding='utf-8')
old=(w/'native/tests/mixed_meaning_provider.rs').read_text(encoding='utf-8')
prefix=old[:old.index('fn job(')]
prefix=prefix.replace('    ledger::{Ledger, Usage},\n','').replace('    retention,\n','')
prefix=prefix.replace(" {key:'b',label:"," {key:'c',label:'Hyper only',description:'Hyperedge participation',aliases:[],support:[supports[0]]},\n {key:'d',label:'Isolated',description:'No relation or hyperedge',aliases:[],support:[supports[0]]},\n {key:'b',label:")
prefix=prefix.replace("description:'Reverse observation',support:supports}],", "description:'Reverse observation',support:supports},{...edge,target:'a'}],")
prefix=prefix.replace("{role:'aux',concept:'a'}]", "{role:'aux',concept:'a'},{role:'other',concept:'c'}]")
prefix=prefix.replace('//! Owned protocol processes exercise accounting and replay, not model quality.', '//! Checked query capabilities retain exact replay and original observation positions.')
(w/'native/tests/mixed_meaning_queries.rs').write_text(prefix,encoding='utf-8')
print('Prepared fresh consumer runner and independent query fixture')
