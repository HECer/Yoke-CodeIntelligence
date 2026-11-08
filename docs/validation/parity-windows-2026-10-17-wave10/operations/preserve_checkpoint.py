"""Archive matching five-phase bytes and reconcile four status files only."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
import types

ROOT = Path("G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence").resolve()
BASE = Path(__file__).resolve().parent
TAG = "declaration-moves-windows-20261017-r2"
BUNDLE = ROOT / "target/parity-validation-20261015" / TAG
DEST = ROOT / "docs/validation/parity-windows-2026-10-17-wave10"
RUNNER_SHA = "412b796dff54e94a5ba1076204c28646f85dc1a769a4e7afa0428416b07f6a6b"
METADATA = ["docs/STATUS.md", "docs/parity/native-reconciliation-2026-10-15.json",
    "docs/parity/windows-development-gates-2026-10-15.json", "native/docs/windows-parity-2026-10-15.md"]
PHASES = ["format", "clippy", "native", "python", "typescript"]

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2)+"\n").encode("utf-8")

def snapshot(folder):
    require(folder.is_dir() and not folder.is_symlink(), "Invalid proof directory")
    files = {}
    for path in sorted(folder.rglob("*")):
        require(not path.is_symlink() and path.resolve().is_relative_to(folder.resolve()), "Proof path escaped")
        if path.is_file():
            files[path.relative_to(folder).as_posix()] = path.read_bytes()
    return files

def atomic(path, data, original):
    require(path.resolve().is_relative_to(ROOT) and not path.is_symlink(), "Metadata path escaped")
    require(path.read_bytes()==original, "Metadata changed before staging")
    backup = BASE / "metadata-backups" / sha(original) / path.relative_to(ROOT)
    backup.parent.mkdir(parents=True, exist_ok=True)
    if not backup.exists():
        backup.write_bytes(original)
    staged = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".yoke-wave10-status-", delete=False) as stream:
            staged = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        require(staged.read_bytes()==data and path.read_bytes()==original, "Metadata changed while staging")
        os.replace(staged, path)
    finally:
        if staged is not None and staged.exists():
            staged.unlink()

def main():
    arguments = argparse.ArgumentParser()
    arguments.add_argument("--validate-only", action="store_true")
    args = arguments.parse_args()
    require(not DEST.exists(), "Refusing an existing checkpoint destination")
    files = snapshot(BUNDLE)
    decode = lambda name: json.loads(files[name])
    summary = decode("summary.json")
    frozen = decode("source-freeze.json")
    require(summary["accepted"] is True and set(summary["phaseReceipts"])==set(PHASES), "Incomplete final acceptance")
    require(summary["runnerSha256"]==sha(files["runner.py"])==RUNNER_SHA, "Independently reviewed runner differs")
    require(summary["sourceManifestSha256"]==sha(files["source-freeze.json"])
        and summary["sourceFiles"]==len(frozen), "Frozen source identity differs")
    require(all(sha(files[name])==digest for name,digest in summary["freezeArtifactSha256"].items()), "Freeze artifact pin differs")
    parser = types.ModuleType("wave10_reviewed_parser")
    parser.__file__ = str(ROOT / "target/parity-validation-20261015/acceptance_wave8.py")
    exec(compile(files["runner.py"], parser.__file__, "exec"), parser.__dict__)
    require(parser.ROOT.resolve()==ROOT and parser.source_manifest()==frozen, "Current source differs from accepted freeze")
    phases = {}
    expected_files = {"summary.json", "source-freeze.json", "runner.py", "freeze.json"}
    for phase in PHASES:
        proof = summary["phaseReceipts"][phase]
        raw = files[phase+"/receipt.json"]
        receipt = json.loads(raw)
        require(sha(raw)==proof["receiptSha256"] and receipt["phase"]==phase, "Phase receipt differs")
        require(receipt["accepted"] is True and not receipt["sourceChanges"] and not receipt["validationErrors"], "Unaccepted phase")
        require(receipt["sourceManifestSha256"]==summary["sourceManifestSha256"]
            and receipt["runnerSha256"]==RUNNER_SHA, "Cross-phase source or runner differs")
        require(decode(phase+"/source-before.json")==decode(phase+"/source-after.json")==frozen, "Phase source maps differ")
        require(all(receipt[key]==proof[key]==sum(row[key] for row in receipt["results"])
            for key in ["passed", "failed", "ignored", "groups"]), "Phase counters differ")
        expected_files.add(phase+"/receipt.json")
        for name,digest in receipt["artifactSha256"].items():
            require(Path(name).name==name and sha(files[phase+"/"+name])==digest, "Phase artifact differs")
            expected_files.add(phase+"/"+name)
        require(len({row["label"] for row in receipt["results"]})==len(receipt["results"]), "Duplicate result labels")
        for row in receipt["results"]:
            require(row["exitCode"]==row["failed"]==0 and row["origin"]=="fresh_execution"
                and row["sourceManifestSha256"]==summary["sourceManifestSha256"]
                and row["runnerSha256"]==RUNNER_SHA, "Result admission differs")
            require(sha(files[phase+"/"+row["label"]+".log"])==row["logSha256"], "Result log differs")
        phases[phase] = receipt
    require(set(files)==expected_files, "Final evidence file set differs from receipt chain")
    exact_commands = {
        "format": [("format", ["cargo","fmt","--all","--","--check"])],
        "clippy": [("clippy", ["cargo","clippy","--workspace","--all-targets","--locked","-j","1","--","-D","warnings"])],
        "python": [("python", ["python","-X","utf8","-m","unittest","discover","-s","scripts/tests","-v"])],
        "typescript": [("typecheck", ["npm.cmd","run","typecheck"]),("typescript", ["npm.cmd","test"])],
    }
    for phase, expected in exact_commands.items():
        require([(row["label"],row["command"]) for row in phases[phase]["results"]]==expected
            and all(row["inventoryOnly"] is False for row in phases[phase]["results"]), "Static gate command or inventory role differs")
    native = phases["native"]
    coverage = native["matrixCoverage"]
    require(coverage["libraryDisjointCoverageAccepted"] and coverage["integrationCoverageAccepted"], "Incomplete native coverage")
    targets = decode("native/target-inventory.json")
    metadata_row = native["results"][0]
    require(metadata_row["label"]=="cargo-metadata" and metadata_row["inventoryOnly"] is True
        and metadata_row["command"]==["cargo","metadata","--no-deps","--locked","--format-version","1"], "Native metadata command differs")
    metadata = decode("native/cargo-metadata.log")
    packages = [row for row in metadata["packages"] if row["id"] in metadata["workspace_members"]]
    require(len(packages)==1 and packages[0]["targets"]==targets, "Cargo target inventory differs")
    seen = {}
    def family(label):
        if label.startswith("library-"):
            return "lib"
        for kind in ["bin","example","bench"]:
            if label.startswith(kind+"-"):
                return kind
        return "doc" if label=="doctests" else "test"
    raw_totals = dict(groups=0, passed=0, failed=0, ignored=0)
    for row in native["results"]:
        if row["inventoryOnly"]:
            continue
        raw = files["native/"+row["label"]+".log"]
        if row["label"]=="doctests":
            raw = re.sub(rb"^\s*Doc-tests (\S+)\r?$",rb" Running doctests (\1.exe)",raw,flags=re.MULTILINE)
        groups = parser.harness_groups(raw,False)
        recorded = [{key:value for key,value in group.items() if key!="target"} for group in row["testGroups"]]
        require(groups==recorded, "Raw native harness differs from receipt")
        require(row["groups"]==len(groups) and all(row[key]==sum(group[key] for group in groups)
            for key in ["passed","failed","ignored"]), "Native raw counters differ")
        for key in raw_totals:
            raw_totals[key] += row[key]
        for group,registered in zip(groups,row["testGroups"],strict=True):
            target = (family(row["label"]),registered.get("target", "doctests"))
            names = set(group["rows"])
            require(not seen.setdefault(target,set()).intersection(names), "Repeated executed native names")
            seen[target].update(names)
    require(all(native[key]==value for key,value in raw_totals.items()), "Native raw aggregate differs")
    inventory_names = {}
    inventory_ignored = {}
    inventory_executables = {}
    for label in ["library-inventory","bin-inventory","example-inventory","bench-inventory","integration-inventory"]:
        if "native/"+label+".json" not in files:
            continue
        inventory = decode("native/"+label+".json")
        all_groups = parser.group_map(parser.harness_groups(files["native/"+label+"-all.log"], True), list(inventory))
        ignored_groups = parser.group_map(parser.harness_groups(files["native/"+label+"-ignored.log"], True), list(inventory))
        for target, record in inventory.items():
            require(record["registered"]==sorted(all_groups[target]["rows"])
                and record["ignored"]==sorted(ignored_groups[target]["rows"])
                and record["executable"]==all_groups[target]["executable"]==ignored_groups[target]["executable"], "Native inventory raw names differ")
            key = (family(label),target)
            inventory_names[key] = set(record["registered"])
            inventory_ignored[key] = set(record["ignored"])
            inventory_executables[key] = record["executable"]
    require(set(seen)-{("doc","doctests")}=={(row["kind"][0],row["name"]) for row in targets}, "Executed target set differs")
    require(set(inventory_names)==set(seen)-{("doc","doctests")}, "Missing target inventory")
    require(all(seen[target]==names for target,names in inventory_names.items()), "Missing native registered tests")
    for row in native["results"]:
        if row["inventoryOnly"] or row["label"]=="doctests":
            continue
        for group in row["testGroups"]:
            target = (family(row["label"]),group["target"])
            require(group["executable"]==inventory_executables[target]
                and group["filtered"]==len(inventory_names[target])-len(group["rows"]), "Executed artifact or filtered count differs")
            require({name for name,state in group["rows"].items() if state=="ignored"}
                ==set(group["rows"]).intersection(inventory_ignored[target]), "Ignored native tests relabeled")
    cli_names = sorted(seen[("test","symbol_move_declarations")])
    require(len(cli_names)==18 and "negotiated_attachments_move_as_one_bundle_with_distinct_entity_and_text_ranges" in cli_names and "transferred_bundles_accept_crlf_insertion_padding_without_changing_their_bytes" in cli_names, "CLI regression set differs")
    require(phases["python"]["passed"]==11 and phases["python"]["failed"]==0
        and phases["typescript"]["passed"]>0 and phases["typescript"]["failed"]==0, "Required language gate differs")

    originals = {name:(ROOT/name).read_bytes() for name in METADATA}
    require(all(sha(raw)==frozen[name] for name,raw in originals.items()), "Metadata differs from frozen source")
    reconciliation = json.loads(originals[METADATA[1]])
    historical_ledger = copy.deepcopy({key:reconciliation[key] for key in ["inventoryLedger","ledgerSummary"]})
    gates = json.loads(originals[METADATA[2]])
    historical = copy.deepcopy({key:gates[key] for key in ["eighthWaveAcceptance","ninthWaveAcceptance","meaningGroupProjectionScopedChecks"]})
    mapping_path = reconciliation["currentBehaviorMapping"]["path"]
    require(mapping_path=="docs/parity/serena-facade-mapping-2026-10-14.json", "Current mapping path differs")
    require(sha((ROOT/mapping_path).read_bytes())==frozen[mapping_path], "Current mapping differs from frozen source")
    reconciliation["currentBehaviorMapping"]["sha256"] = frozen[mapping_path]
    launcher = snapshot(BASE/"acceptance"/TAG)
    failed_tag = "declaration-moves-windows-20261017-r1"
    failed_files = snapshot(ROOT/"target/parity-validation-20261015"/failed_tag)
    require(json.loads(failed_files["source-freeze.json"])==frozen
        and sha(failed_files["runner.py"])==RUNNER_SHA,"Failed attempt source/runner differs")
    for phase in ["format","clippy","native"]:
        prior = json.loads(failed_files[phase+"/receipt.json"])
        require(prior["sourceManifestSha256"]==summary["sourceManifestSha256"]
            and prior["runnerSha256"]==RUNNER_SHA and not prior["sourceChanges"],"Failed attempt identity differs")
        require(all(Path(name).name==name and sha(failed_files[phase+"/"+name])==digest
            for name,digest in prior["artifactSha256"].items()),"Failed attempt artifact differs")
        require(prior["accepted"] is (phase!="native"),"Failed attempt phase status differs")
    failed_native = json.loads(failed_files["native/receipt.json"])
    require(all(failed_native[key]==0 for key in ["passed","failed","ignored","groups"])
        and len(failed_native["results"])==1 and failed_native["results"][0]["label"]=="cargo-metadata"
        and failed_native["results"][0]["exitCode"]==0 and failed_native["results"][0]["inventoryOnly"]
        and len(failed_native["validationErrors"])==1 and failed_native["validationErrors"][0].startswith("JSONDecodeError:"),
        "Earlier attempt is not the diagnosed pre-test JSON admission failure")
    failed_launcher = snapshot(BASE/"acceptance"/failed_tag)
    probes = {}
    for name in ["red.log","red-r2.log","green.log","green-r2.log","attachment-red.log",
                 "attachment-green.log","crlf-red.log","crlf-red-r2.log","final-scoped.log",
                 "typecheck.log","typescript.log"]:
        path = BASE.parent/("declaration-moves-20261017-"+name)
        probes[name] = path.read_bytes()
    def current_without_evidence():
        prefix = DEST.relative_to(ROOT).as_posix()+"/"
        return {name:digest for name,digest in parser.source_manifest().items() if not name.startswith(prefix)}
    require(current_without_evidence()==frozen,"Source changed during validation")
    if args.validate_only:
        print(json.dumps(dict(ready=True,sourceManifestSha256=summary["sourceManifestSha256"],
            sourceFiles=len(frozen),cargoTargets=len(targets),native=raw_totals,newScopedTests=len(cli_names))))
        return
    DEST.mkdir(parents=True,exist_ok=False)
    pins = {}
    def archive(values, prefix):
        for name,raw in sorted(values.items()):
            relative = prefix+"/"+name
            destination = DEST/relative
            require(destination.resolve().is_relative_to(DEST.resolve()),"Archive path escaped")
            destination.parent.mkdir(parents=True,exist_ok=True)
            with destination.open("xb") as stream:
                stream.write(raw)
            require(destination.read_bytes()==raw,"Archived bytes differ")
            pins[relative] = sha(raw)
    archive(files,"evidence/"+TAG)
    archive(launcher,"launcher")
    archive(failed_files,"failed-attempts/"+failed_tag)
    archive(failed_launcher,"failed-launchers/"+failed_tag)
    archive(probes,"development-probes")
    archive({"preserve_checkpoint.py":Path(__file__).read_bytes()},"operations")
    preserved = DEST/"evidence"/TAG
    require(all((preserved/name).read_bytes()==raw for name,raw in files.items()),"Preserved proof differs")
    claims = dict(universalParity=False,releaseCertified=False,newParityRowsCertified=False,
        fullUpstreamEquivalence=False,exhaustiveCompilerReferenceBinding=False,liveProprietaryIdeParity=False)
    index = dict(schemaVersion=1,kind="windows_rust_declaration_move_acceptance",accepted=True,
        acceptedTag=TAG,sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),
        rustFiles=sum(name.endswith(".rs") for name in frozen),phaseReceipts=summary["phaseReceipts"],
        artifactPins=pins,nonadditiveDeclarationTests=cli_names,postFreezeMetadataFiles=METADATA,claims=claims,
        failedAttempts=[dict(tag=failed_tag,accepted=False,contributesToAcceptedTotals=False,
            passed=0,failed=0,ignored=0,groups=0,
            reason="The unchanged runner combined RTK's no-hook stderr notice with cargo metadata stdout and refused JSON before any native test. A process-scoped RTK_SUPPRESS_HOOK_WARNING=1 suppresses only that notice; independently parsed metadata remained equal. The final five phases are fresh on the same source/runner; no global hook/configuration changed.")],
        developmentProbeInterpretation="Separate nonadditive probes: corrected initial RED had1passed/9failed; first implementation had10new passes plus one existing diagnostic-text failure; corrected scoped64passed/0failed; attachment RED0passed/5failed then scoped69passed/0failed; one misselected CRLF command ran zero tests and is not evidence; actual CRLF RED0passed/1failed; final scoped94passed/0failed. Only final five frozen phases provide accepted totals.",
        review="Two independent read-only source/doc/test reviews found no unresolved findings. Final review included CRLF padding fix, old capability compatibility and nested syntax. Reviewers ran no tests; actual final phase receipts provide runtime evidence.",
        interpretation="All five phases freshly executed on identical source and runner. Scoped checks and earlier accepted waves are never added; ignored tests remain ignored.")
    index_raw = encoded(index)
    (DEST/"index.json").write_bytes(index_raw)
    (DEST/"index.sha256").write_text(sha(index_raw)+"\n",encoding="ascii")
    relative_index = (DEST/"index.json").relative_to(ROOT).as_posix()
    relative_bundle = preserved.relative_to(ROOT).as_posix()
    phase_proof = {name:dict(receipt=f"{relative_bundle}/{name}/receipt.json",**proof)
        for name,proof in summary["phaseReceipts"].items()}
    acceptance = dict(status="accepted_windows_development_checkpoint",platform="Windows",acceptedTag=TAG,
        index=relative_index,indexSha256=sha(index_raw),receipt=phase_proof["native"]["receipt"],
        sourceManifest=f"{relative_bundle}/source-freeze.json",sourceManifestSha256=summary["sourceManifestSha256"],
        runnerSha256=RUNNER_SHA,sourceFiles=len(frozen),rustFiles=index["rustFiles"],phaseProof=phase_proof,
        cargoTargets=len(targets),**{key:native[key] for key in ["groups","passed","failed","ignored"]},
        nativeMatrix={key:coverage[key] for key in ["integrationTargets","libraryRegistered","libraryIgnored"]},
        claims=claims,sourceChangesDuringEachGate=[],postFreezeMetadataFiles=METADATA,
        boundedCapabilities=["Checked own_v1 moves of free Rust functions, structs, enums, traits, type aliases and constants at file/inline-module scope.",
            "Explicit sourceAttachments exact_text_v1 capability admits an independently derived unchanged outer-documentation/attribute text bundle, retaining distinct original graph/name anchors and checking every transferred attachment owner.",
            "Original-coordinate cut, exact AST/direct-parent placement, extra-copy provenance, other preamble/attachment preservation, old capability compatibility and CRLF padding; original guarded change/apply authority remains."],
        limitations=["Module/directory moves, associated/local declarations, other languages, compilation/attribute expansion and exhaustive reference repair remain outside this slice.",
            "Existing publication atomicity and proprietary IDE limitations remain; universal parity and release certification stay open."],
        inventoryBoundary=dict(originalIdentities=353,unreconciledReviewRequired=73,backendRuntimeUnknowns=77,
            identitiesStatusesAndCertificationsChanged=False),nonadditiveDeclarationTests=cli_names)
    reconciliation["tenthWaveAcceptance"] = acceptance
    replacements = [
        ("free Rust function directly under a file or inline module", "Rust function, struct, enum, trait, type alias or constant directly under a file or inline module"),
        ("free-Rust-function file/inline-module destination slice", "named-declaration file/inline-module destination and negotiated attachment-bundle slice"),
        ("Rust free-function slice", "Rust named-declaration/negotiated-attachment slice"),
        ("Rust free-function/file-or-inline-module slice", "Rust named-declaration/negotiated-attachment file-or-inline-module slice"),
        ("own_v1 free-Rust-function", "own_v1 named-Rust-declaration"),
        ("Source declarations with attached outer attributes/docstrings, other declaration kinds/languages,", "Attached outer attributes/docstrings require negotiated sourceAttachments exact_text_v1 and a separately checked sourceTextRange bundle with original entity/name anchors retained. Other declaration kinds/languages,"),
    ]
    def refresh(value):
        if isinstance(value,dict): return {key:refresh(item) for key,item in value.items()}
        if isinstance(value,list): return [refresh(item) for item in value]
        if isinstance(value,str):
            for old,new in replacements: value = value.replace(old,new)
        return value
    for key in ["currentBehaviorMapping","reconciledCapabilities","prioritizedRemainingScopes","residualRegister"]:
        reconciliation[key] = refresh(reconciliation[key])
    for scope in reconciliation["prioritizedRemainingScopes"]:
        if scope["id"]=="R1":
            scope["evidence"] = list(dict.fromkeys(scope["evidence"]+[
                "native/src/refactor_contract.rs","native/src/semantic_move_intents.rs","native/src/lsp_refactor_intents.rs","native/tests/symbol_move_declarations.rs"]))
    for capability in reconciliation["reconciledCapabilities"]:
        if capability["id"]=="owned_rust_function_move":
            capability["tests"] = list(dict.fromkeys(capability["tests"]+["native/tests/symbol_move_declarations.rs"]))
    require(all(reconciliation[key]==value for key,value in historical_ledger.items()),"Original ledger changed")
    gates["tenthWaveAcceptance"] = acceptance
    gates["latestWholeNativeRun"] = acceptance
    gates["wholeNativeRun"] = acceptance
    gates["packagingPython"] = phase_proof["python"]
    gates["typescript"] = dict(phase_proof["typescript"],skipped=phases["typescript"]["ignored"],typecheck="passed",
        interpretation="Complete fresh frozen phase only; scoped and historical checks are nonadditive.")
    require(all(gates[key]==value for key,value in historical.items()),"Historical acceptance changed")
    paragraph = (f"**Accepted Wave 10 Windows development checkpoint.** Tag `{TAG}` passed {native['passed']} native tests/"
        f"{native['failed']} failures/{native['ignored']} ignored across {len(targets)} Cargo targets/{native['groups']} groups; "
        f"Python {phases['python']['passed']} and TypeScript {phases['typescript']['passed']}/{phases['typescript']['ignored']} skipped. "
        f"Its {len(frozen)}-file/{index['rustFiles']}-Rust source map is `{summary['sourceManifestSha256']}`. "
        "Checked own_v1 Rust symbol moves now admit structs, enums, traits, type aliases and constants alongside free functions. "
        "Explicit negotiated exact_text_v1 attachment transfer preserves outer documentation, attributes and intervening original bytes, "
        "while graph declaration/name anchors stay distinct from the checked text range. Final AST kind, exact placement, attachment ownership, "
        "copy provenance and other preambles remain independently checked. CRLF padding, old capability envelopes and nested moved syntax are covered. "
        "Eighteen new declaration regressions and scoped94 are included, never added to whole-project totals. "
        "Compilation, attribute expansion, exhaustive reference repair, directory/module moves and proprietary IDE certification remain open. "
        "All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. "
        "This is an unreleased Windows checkpoint; universal parity and release remain open. ")
    outputs = {METADATA[1]:encoded(reconciliation),METADATA[2]:encoded(gates)}
    for name,link in [(METADATA[0],"validation/parity-windows-2026-10-17-wave10/index.json"),
                      (METADATA[3],"../../docs/validation/parity-windows-2026-10-17-wave10/index.json")]:
        content = originals[name].decode("utf-8")
        marker = "**Accepted Wave 9 Windows development checkpoint.**"
        require(content.count(marker)==1,"Status insertion marker differs")
        outputs[name] = content.replace(marker,paragraph+f"See the [preserved index]({link}).\n\n"+marker,1).encode("utf-8")
    require(current_without_evidence()==frozen,"Source changed before metadata reconciliation")
    for name,raw in outputs.items(): atomic(ROOT/name,raw,originals[name])
    current = current_without_evidence()
    require(set(current)==set(frozen),"Post-freeze source inventory changed")
    changed = sorted(name for name in frozen if current[name]!=frozen[name])
    require(changed==sorted(METADATA),"Unexpected post-freeze change")
    require(all(sha((DEST/name).read_bytes())==digest for name,digest in pins.items()),"Archived pin differs")
    (DEST/"post-freeze-metadata.json").write_bytes(encoded(dict(indexSha256=sha(index_raw),changedFrozenPaths=changed,
        hashes={name:current[name] for name in METADATA},originalLedgerUnchanged=True,historicalAcceptanceUnchanged=True)))
    print(json.dumps(dict(accepted=True,index=relative_index,indexSha256=sha(index_raw),artifactPins=len(pins),
        nativePassed=native["passed"],nativeIgnored=native["ignored"],changedFrozenPaths=changed)))

if __name__=="__main__":
    main()
