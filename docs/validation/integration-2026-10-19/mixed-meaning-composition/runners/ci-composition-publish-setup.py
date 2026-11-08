from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-consumers-publish.py').read_text(encoding='utf-8').replace('mixed-meaning-consumers','mixed-meaning-composition')
s=s.replace('### Latest explicit mixed-source meaning builds','### Latest checked mixed interpretation graph queries')
start=s.index("latest=f'''");end=s.index("text=text[:start]+latest",start)
latest="""latest=f'''### Latest checked mixed graph composition

Opaque composition consumes two to four independently source/response-replayed
parents and an exact ordered host-authored link review. Original parent-qualified
concept/observation namespaces, parallel/reverse/self provider rows, ordered
hyperedge slots and full nested typed source/receipt/accounting remain intact.
Labels/aliases create no merge or semantic identity. Host links retain separate
indices, unverified provenance and degree counts; traces follow them only when
explicitly selected. Hyperedge roles create no flow.

Library parents may come from different checked workspaces/history. The CLI
selects explicit jobs from one root Store and applies PDF worker permission per
parent, including combined PDF/Office graphs. Frozen queries survive both Store
losses. No provider, persistence, source write, JSON capability construction or
default eight-tool MCP schema change is added. Broader analytics, semantic
qualification, cross-workspace CLI admission and full R4 remain open.

Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
Eight new default cases include owned Node/PDF setup, qualified identities,
original and authored multiedges/degrees, Unicode ambiguity, direction/depth0,
capacity/deadline/private input refusal, independent parent/endpoints, both Store
losses and actual PDF+Office CLI query without provider files. Initial test-only
Copy/Clone compilation failure executes no test groups; the next focused three
groups pass27/0/0. Scoped protocol/query evidence establishes no model-quality,
independent billing, semantic truth or current live workspace applicability.

'''
"""
s=s[:start]+latest+s[end:]
s=s.replace("line.startswith('|19 latest selected native groups|')","line.startswith('|19 latest selected native groups|')")
# The prior updater matches18 and emits19; this scope matches19 and emits20.
s=s.replace("line.startswith('|18 latest selected native groups|')","line.startswith('|19 latest selected native groups|')")
s=s.replace('f"|19 latest selected native groups|','f"|20 latest selected native groups|')
s=s.replace("assert '|18 latest selected native groups|' not in text", "assert '|19 latest selected native groups|' not in text")
s=s.replace('Checked mixed graph query contract','Checked mixed composition contract').replace('native/docs/mixed-meaning-queries.md','native/docs/mixed-meaning-composition.md')
s=s.replace('Prior explicit provider build qualification','Prior checked single-graph query qualification').replace("link('docs/validation/integration-2026-10-19/mixed-meaning-provider/index.json')", "link('docs/validation/integration-2026-10-19/mixed-meaning-consumers/index.json')")
(t/'ci-composition-publish.py').write_text(s,encoding='utf-8')
print('Prepared fresh composition PR publisher; not executed')
