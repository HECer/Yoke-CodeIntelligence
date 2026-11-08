"""Prepare future PDF acceptance operations from the reviewed feedback checks."""
import hashlib
import json
from pathlib import Path
import re

BASE = Path(__file__).resolve().parent
SOURCE = BASE.parent/'outcome-learning-20261017'
BASELINE = 'd6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec'
CANDIDATE = 'a3e0210e314936ffe007661111e2780d02081dbd41c9ca21467a85129646f247'
INTEGRATOR = 'cc042dd8ff6342c2e37ed79b1e953122d7fc9700dc6bd95a21d218b887768832'
sha = lambda raw: hashlib.sha256(raw).hexdigest()
assert sha((BASE/'integrate.py').read_bytes()) == INTEGRATOR
assert sha((BASE/'development/review-candidate-source-freeze.json').read_bytes()) == CANDIDATE
names = list(json.loads((BASE/'development/validated-candidate.json').read_bytes())['changedPaths'])
assert len(names) == 9

def once(value, old, new):
    assert value.count(old) == 1, old[:100]
    return value.replace(old, new)

def original(name, expected):
    raw = (SOURCE/name).read_bytes()
    assert sha(raw) == expected, name
    return raw.decode('utf-8').replace('\r\n', '\n')

def common(value):
    value = value.replace('parity-windows-2026-10-17-wave13', 'parity-windows-2026-10-17-wave14')
    value = value.replace('parity-windows-2026-10-17-wave12', 'parity-windows-2026-10-17-wave13')
    value = value.replace('wave13_reviewed_parser', 'wave14_reviewed_parser')
    value = value.replace('thirteenthWaveAcceptance', 'fourteenthWaveAcceptance')
    value = value.replace('twelfthWaveAcceptance', 'thirteenthWaveAcceptance')
    value = value.replace('Wave 13', 'Wave 14').replace('Wave 12', 'Wave 13')
    for quote in ['"', "'"]:
        value = value.replace(quote+'outcome-feedback-windows-20261017-r1'+quote,
            quote+'pdf-mixed-view-windows-20261017-r1'+quote)
        value = value.replace(quote+'mixed-view-windows-20261017-r1'+quote,
            quote+'outcome-feedback-windows-20261017-r1'+quote)
    value = value.replace('1ef41d7564050c370f7db36fd539e0db1fa081248fe282db4997c46572d09e26', '__BASELINE__')
    value = value.replace(BASELINE, CANDIDATE).replace('__BASELINE__', BASELINE)
    value = value.replace('7e65528091d28639b8b27d18b6b473c8cbdf92c2a095467dce34be879936e863', INTEGRATOR)
    value = value.replace('nonadditiveFeedbackCliTests', 'nonadditivePdfTests')
    value = value.replace('newFeedbackCliTests', 'newPdfTests')
    if 'def verified_parent_map(' in value:
        assert value.count('[1898,0,33,244]') == 2
        value = value.replace('[1898,0,33,244]', '[1919,0,33,245]')
        value = once(value,
            "and len(json.loads(files[prefix+'native/target-inventory.json'])) == 239,",
            "and len(json.loads(files[prefix+'native/target-inventory.json'])) == 240,")
    return value

preserver = common(original('preserve_checkpoint.py',
    'dcff213d54bfce4474535af1467e47482d4c207b8ffebf5a6b71008f6f0d683f'))
preserver = once(preserver, 'regression_path="native/tests/feedback_cli.rs"',
    'regression_path="native/tests/mixed_pdf_view.rs"')
preserver = preserver.replace('Feedback regression', 'PDF regression')
preserver = once(preserver, 'len(cli_names)==21 and set(cli_names)==seen[("test","feedback_cli")]',
    'len(cli_names)==9 and set(cli_names)==seen[("test","mixed_pdf_view")]')
preserver = once(preserver, 'inventory_ignored[("test","feedback_cli")]',
    'inventory_ignored[("test","mixed_pdf_view")]')
preserver = once(preserver, 'native["passed"]==1919', 'native["passed"]==1928')
preserver = once(preserver, 'native["groups"]==245 and len(targets)==240',
    'native["groups"]==246 and len(targets)==241')
preserver = once(preserver, 'parent["passed"]==1898', 'parent["passed"]==1919')
start = preserver.index('    roots={name:')
end = preserver.index('    require(sha(roots[', start)
root_names = ['baseline-source-freeze.json', 'contract.md', 'contract_red.py',
    'cli-red.log', 'cli-red-r2.log', 'cli-red-r3.log', 'cli-red-r3-receipt.json',
    'original-pdf-worker-oracle.json', 'run_contract_green.py', 'run_scoped.py',
    'dependency-cache.json', 'prepare_dependency_cache.py', 'setup.json',
    'collect_scoped_validation.py', 'prepare_integration_helper.py',
    'record_candidate.py', 'before-nine-path-record_candidate.py', 'record_red.py',
    'record-before-page-lf.py', 'contract-before-page-lf.py', 'oracle-red-r1.log',
    'run_remaining_scoped.py', 'prepare_checkpoint_helpers.py']
preserver = preserver[:start]+'    roots={name:(BASE/name).read_bytes() for name in '+repr(root_names)+'}\n'+preserver[end:]
start = preserver.index('    integration_paths={')
end = preserver.index('    require(admitted', start)
preserver = preserver[:start]+'    integration_paths=set('+repr(names)+')\n'+preserver[end:]
preserver = once(preserver,
    'name.startswith("docs/validation/parity-windows-2026-10-17-wave11/")',
    'name.startswith(("docs/validation/parity-windows-2026-10-17-wave11/","docs/validation/parity-windows-2026-10-17-wave12/"))')
start = preserver.index('    def current_without_evidence():')
preserver = preserver[:start]+'''    scoped=json.loads(probes["validated-candidate.json"])
    require(scoped["sourceManifestSha256"]==sha(probes["review-candidate-source-freeze.json"])
        and scoped["scopedUniqueRustPassed"]==112 and scoped["newPdfTests"]==9
        and scoped["existingRegressionTests"]==103 and scoped["unchangedCliContractGreen"]==2
        and scoped["typescriptPassed"]==254 and scoped["typescriptSkipped"]==3
        and scoped["primaryIntegrated"] is False and scoped["wholeAcceptance"] is False,
        "Scoped PDF proof identity differs")
    scope_objects=dict(roots)
    scope_objects.update({"development/"+name:raw for name,raw in probes.items()})
    require(all(name in scope_objects and sha(scope_objects[name])==digest
        for name,digest in scoped["artifactPins"].items()),"Scoped artifact pin differs")
'''+preserver[start:]
start = preserver.index('    claims=dict(')
end = preserver.index('    index_raw=encoded(index)', start)
preserver = preserver[:start]+'''    claims=dict(universalParity=False,releaseCertified=False,newParityRowsCertified=False,fullUpstreamEquivalence=False,
        completeMixedDocumentParity=False,mixedSemanticExtraction=False,crossSourceGraphLinks=False,
        renderedPdfLayout=False,ocrEstablished=False,binaryAttestationEstablished=False)
    index=dict(schemaVersion=1,kind="windows_frozen_pdf_mixed_view_acceptance",accepted=True,acceptedTag=TAG,
        sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),rustFiles=sum(name.endswith(".rs") for name in frozen),
        phaseReceipts=summary["phaseReceipts"],artifactPins=pins,nonadditivePdfTests=cli_names,
        postFreezeMetadataFiles=METADATA,claims=claims,
        validationProfile="Original assertions, deadlines, security checks and durable flushes remain. The original sixteen-root durable fixture, three ACL cases and real short-name tests use C; query/github/debug targets run serially on E, ordinary library/integration tests retain four threads. This is a C-backed functional profile, not an HDD latency certification.",
        developmentProbeInterpretation="Actual unchanged CLI contract RED0/2 thenGREEN2/0. Nine PDF tests and103 existing tests across14targets passed on f236; only a schema2 documentation link followed. The same nine passed again on finalA3, along with fmt/all-targetClippy/typecheck/TS254/3skips. Repeated/scoped cases are nonadditive; only the five fresh frozen phases supply accepted totals. Early CLI fixture/oracle corrections remain recorded separately.",
        review="Independent read-only source/doc review Ready at A3 after the Office caveat, non-BMP page fixture and post-spawn original-process-handle cancellation test. All nine leaves checked; reviewers executed no tests. Future integration helper separately reviewed with its complete-manifest and pinned parent-verifier guards.",
        integrationInterpretation="Exactly nine reviewed auxiliary byte buffers and checked parent preimages; complete before/after source maps must match the fresh accepted source. Only four recorded status files and accepted Wave11/Wave12 archives extend the sealed product baseline; Wave13 is fully replayed as the parent. Git source admission and full closed archive pins remain separate. Per-file optimistic replacement under a quiescent checkout; no hostile-writer CAS or multi-file atomicity claim.",
        interpretation="Strict schema2 adds explicit registered frozen PDF assets to native/Office literal queries. Original containers are reparsed by the separately permitted bounded native worker, not trusted cached inspection text. PDF page UTF-8 hashes/ranges remain distinct from native source, XML and opaque binary bytes. No Store/source publication, capture or provider; complete remains false.")
'''+preserver[end:]
start = preserver.index('        boundedCapabilities=[')
end = preserver.index('    require(all(reconciliation[key]', start)
preserver = preserver[:start]+'''        boundedCapabilities=["Schema2 selects one pinned native View, optional DOCX/XLSX graphs and registered PDF assets, at most four documents together; a separate host capability or CLI flag permits the own native worker.",
            "Every load checks original frozen PDF size/hash/registrations and reruns the existing strict parser with managed process, bounded pipes, shared deadline/cancellation and resource caps. Combined row/container/text/worker reservations precede bulk native reads.",
            "Globally paged literal search and typed reads preserve exact decoded page UTF-8 hashes/ranges, original containers and native View/overlay/Meaning receipt/coverage roles; caller SQL state and source/Store bytes remain unchanged."],
        limitations=["Direct page content-stream text is not rendered reading order, OCR, layout or exhaustive form/image interpretation. Office XML origins enclose elements and cached spreadsheet values may be stale.",
            "The host explicitly trusts its native executable; binary attestation is unverified. Conservative input/metadata reservations and existing independent replay limits remain. There is no new persisted mixed graph.",
            "Mixed-space semantic extraction, general cross-source graph links and cross-workspace joins remain open. This bounded literal-query slice does not establish complete R4, universal or release parity."],
        inventoryBoundary=dict(originalIdentities=353,unreconciledReviewRequired=73,backendRuntimeUnknowns=77,identitiesStatusesAndCertificationsChanged=False),
        nonadditivePdfTests=cli_names)
    reconciliation["fourteenthWaveAcceptance"]=acceptance
    for scope in reconciliation["prioritizedRemainingScopes"]:
        if scope["id"]=="R4":
            scope["mixedQueryAddition"]="Checked schema1 native source/DOCX/XLSX queries and explicitly permitted schema2 frozen PDF pages share globally paged literal search and exact typed reads. Original source, container, XML and decoded page/text spaces remain distinct, with complete native View/Meaning provenance. Mixed semantic extraction and cross-source graph links remain open."
            scope["processedPdfAddition"]="Strict schema2 mixed literal queries add explicitly selected registered frozen PDF pages through the separately permitted bounded native worker. Original container and decoded page hashes/UTF-8 ranges remain distinct; cached inspection text is not source proof. Native View/Meaning provenance and schema1 Office behavior remain."
            scope["missingBehavior"]="Mixed-space semantic extraction and general cross-source graph links remain open. Checked source/Office queries and explicitly permitted frozen PDF page text now have separate bounded Windows evidence; no rendered layout, OCR or exhaustive binary-document interpretation is implied."
            scope["suggestedScope"]="Preserve distinct typed source spaces and explicit worker authority. Define mixed-space semantic inputs and graph relationships with complete original receipt/source replay; never promote decoded coordinates or provider claims into canonical source authority."
            scope["evidence"]=list(dict.fromkeys(scope["evidence"]+sorted(integration_paths)))
    for capability in reconciliation["reconciledCapabilities"]:
        if capability["id"]=="standalone_office_graphs":
            capability["current"]+=" Schema2 mixed literal queries additionally replay explicitly selected registered PDF containers through the bounded native worker and expose typed decoded page reads."
            capability["boundary"]="Office container/XML/decoded-text spaces and PDF container/page-text spaces stay distinct. Formula execution, complete layout/OCR, mixed-space semantic extraction and general cross-source graph links remain open. No persisted PDF graph is introduced."
            capability["docs"]=list(dict.fromkeys(capability["docs"]+["native/docs/mixed-pdf-views.md"]))
            capability["implementation"]=list(dict.fromkeys(capability["implementation"]+["native/src/mixed_pdf_input.rs"]))
            capability["tests"]=list(dict.fromkeys(capability["tests"]+[regression_path]))
    for residual in reconciliation["residualRegister"]:
        if residual["id"]=="document_external_acquisition_database":
            residual["remainingBoundary"]="Mixed-space semantic extraction and general cross-source graph links remain beyond the checked native/Office/frozen-PDF literal-query slice. External Postgres/Google/URL acquisition, actual ASR and graph database writes remain separate from local readers and generated Cypher."
'''+preserver[end:]
start = preserver.index('    paragraph=(f"**Accepted Wave')
end = preserver.index('    outputs=', start)
preserver = preserver[:start]+'''    paragraph=(f"**Accepted Wave 14 Windows development checkpoint.** Tag `{TAG}` passed {native['passed']} native tests/"
        f"{native['failed']} failures/{native['ignored']} ignored across {len(targets)} Cargo targets/{native['groups']} groups; "
        f"Python {phases['python']['passed']} and TypeScript {phases['typescript']['passed']}/{phases['typescript']['ignored']} skipped. "
        f"Its {len(frozen)}-file/{index['rustFiles']}-Rust source map is `{summary['sourceManifestSha256']}`. "
        "Schema2 mixed queries now add explicitly selected registered frozen PDF pages through the separately permitted bounded native worker. "
        "Original containers are rehashed and reparsed; decoded page UTF-8 coordinates, XML and native source bytes retain separate identities. "
        "Native View/overlay/Meaning provenance, global literal paging and pre-bulk combined budgets remain. Nine new PDF regressions are included, never added to whole-project totals. "
        "Queries publish no Store/source state or provider output. Mixed semantics, cross-source graph links, OCR and rendered layout remain open; host executable trust is not binary attestation. "
        "All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. "
        "This is an unreleased Windows development checkpoint; universal parity and release remain open. ")
'''+preserver[end:]

verifier = common(original('verify_checkpoint.py',
    '7cabba3e438926a5667aa5e41f6e22bf443d9fb0e9459b3fe2dd1d97c12fbe2e'))
verifier = once(verifier, 'Independently verify Wave13 archive', 'Independently verify Wave14 archive')
verifier = once(verifier, "len(index['nonadditivePdfTests']) == 21", "len(index['nonadditivePdfTests']) == 9")
verifier = once(verifier, '[1919, 0, 33, 245]', '[1928, 0, 33, 246]')
verifier = once(verifier, "len(read(bundle/'native/target-inventory.json')) == 240", "len(read(bundle/'native/target-inventory.json')) == 241")
verifier = once(verifier, "g.get('target') == 'feedback_cli'", "g.get('target') == 'mixed_pdf_view'")
verifier = once(verifier, "len(integration['paths']) == 14", "len(integration['paths']) == 9")
verifier = once(verifier, "name.startswith('docs/validation/parity-windows-2026-10-17-wave11/')",
    "name.startswith(('docs/validation/parity-windows-2026-10-17-wave11/','docs/validation/parity-windows-2026-10-17-wave12/'))")
verifier = once(verifier, 'newPdfTests=21', 'newPdfTests=9')
launcher = common(original('run_acceptance.py',
    '0faa0e6ed4c5c438bf74aa181f336b73fb0b580d4d6d9d4a690050cd2b05b81c'))
launcher = launcher.replace('integrated feedback source freeze', 'integrated PDF mixed-view source freeze')
for name, value in [('preserve_checkpoint.py',preserver),('verify_checkpoint.py',verifier),('run_acceptance.py',launcher)]:
    compile(value, str(BASE/name), 'exec')
    with (BASE/name).open('xb') as stream:
        stream.write(value.encode('utf-8'))
    print(dict(prepared=name, sha256=sha((BASE/name).read_bytes()), executed=False))
