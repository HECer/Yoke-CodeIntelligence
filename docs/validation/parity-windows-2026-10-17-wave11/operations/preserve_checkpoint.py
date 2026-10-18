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
TAG = "meaning-join-windows-20261017-c-durable"
BUNDLE = ROOT / "target/parity-validation-20261015" / TAG
DEST = ROOT / "docs/validation/parity-windows-2026-10-17-wave11"
RUNNER_SHA = "5b1dbf5179f274d57fc0217108867d90d9a62ae233ca551496e89d386df81a00"
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
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".yoke-wave11-status-", delete=False) as stream:
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
    parser = types.ModuleType("wave11_reviewed_parser")
    parser.__file__ = str(ROOT / "target/parity-validation-20261015/acceptance_wave11_c_durable.py")
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
    durable_name="host_install::output_recovery_tests::sixteen_root_v2_history_reserves_only_remaining_output_work"
    require(coverage["libraryDurableTests"]==[durable_name],"Durable fixture profile differs")
    durable_rows=[row for row in native["results"] if row["label"]=="library-c-durable-0"]
    require(len(durable_rows)==1 and durable_rows[0]["testGroups"][0]["rows"]=={durable_name:"ok"}
        and durable_rows[0]["tempRoot"]==native["aclTempRoot"]
        and durable_rows[0]["command"][-2:]==["--test-threads=1","--exact"],"Durable fixture execution differs")
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

    def required_tests(path):
        require(sha((ROOT/path).read_bytes())==frozen[path],"Regression source differs")
        return sorted(re.findall(r"#\[test\]\s*fn\s+(\w+)\s*\(",(ROOT/path).read_text(encoding="utf-8")))
    cli_names=required_tests("native/tests/meaning_join_cli.rs")
    unit_names=["meaning_join::query::tests::"+name for name in required_tests("native/src/meaning_join_query_tests.rs")]
    require(len(cli_names)==13 and len(unit_names)==3,"New regression inventory differs")
    require(set(cli_names)==seen[("test","meaning_join_cli")]
        and not set(cli_names).intersection(inventory_ignored[("test","meaning_join_cli")]),"New CLI names missing or ignored")
    require(set(unit_names).issubset(seen[("lib","yoke")])
        and not set(unit_names).intersection(inventory_ignored[("lib","yoke")]),"New unit names missing or ignored")
    require(phases["python"]["passed"]==11 and phases["typescript"]["passed"]>0,"Language gate differs")
    originals={name:(ROOT/name).read_bytes() for name in METADATA}
    require(all(sha(raw)==frozen[name] for name,raw in originals.items()),"Metadata differs from freeze")
    reconciliation=json.loads(originals[METADATA[1]])
    ledger=copy.deepcopy({key:reconciliation[key] for key in ["inventoryLedger","ledgerSummary"]})
    gates=json.loads(originals[METADATA[2]])
    historical=copy.deepcopy({key:value for key,value in gates.items()
        if key.endswith("WaveAcceptance") or key=="meaningGroupProjectionScopedChecks"})
    historical_reconciliation=copy.deepcopy({key:value for key,value in reconciliation.items() if key.endswith("WaveAcceptance")})
    parent=gates["tenthWaveAcceptance"]
    parent_index=ROOT/parent["index"]
    require(sha(parent_index.read_bytes())==parent["indexSha256"]=="4de65fbe19220356f19fcdddb079542f9475490dcb790265d38a565a2b985ee3","Parent index differs")
    require(native["passed"]==parent["passed"]+len(cli_names)+len(unit_names)
        and native["ignored"]==parent["ignored"] and native["failed"]==0,"New whole native counts differ from disjoint regression delta")
    mapping=reconciliation["currentBehaviorMapping"]
    require(mapping["path"]=="docs/parity/serena-facade-mapping-2026-10-14.json"
        and mapping["sha256"]==frozen[mapping["path"]]==sha((ROOT/mapping["path"]).read_bytes()),"Current facade mapping pin differs")
    launcher=snapshot(BASE/"acceptance"/TAG)

    legacy_runner_sha="412b796dff54e94a5ba1076204c28646f85dc1a769a4e7afa0428416b07f6a6b"
    failed_proofs={}
    failed_attempts=[]
    expected_failures={
        "meaning-join-windows-20261017-r1":("github","explicit_loopback_http_reads_observed_ci_review_and_scope_without_commands"),
        "meaning-join-windows-20261017-r2":("debug_adapter","reverse_launch_authority_and_foreign_stale_or_unstopped_frames_are_refused"),
        "meaning-join-windows-20261017-serial-hosts":("yoke",durable_name),
    }
    for failed_tag,expected_failure in expected_failures.items():
        failed_files=snapshot(ROOT/"target/parity-validation-20261015"/failed_tag)
        failed_launcher=snapshot(BASE/"acceptance"/failed_tag)
        failed_native=json.loads(failed_files["native/receipt.json"])
        incomplete=failed_tag=="meaning-join-windows-20261017-r2"
        failed_runner_sha=("de3a16cd7bfa2d15d0f7f4546d5151450349b674290c12a02e6688cf3da0ce4c"
            if failed_tag=="meaning-join-windows-20261017-serial-hosts" else legacy_runner_sha)
        require(failed_native["accepted"] is False and failed_native["failed"]==1
            and not failed_native["sourceChanges"]
            and failed_native["validationErrors"]==(["RuntimeError: Missing harness summary before next executable"] if incomplete else [])
            and failed_native["sourceManifestSha256"]==summary["sourceManifestSha256"]
            and failed_native["runnerSha256"]==sha(failed_files["runner.py"])==failed_runner_sha
            and failed_files["source-freeze.json"]==files["source-freeze.json"],"Failed-attempt identity differs")
        failed_totals=dict(groups=0,passed=0,failed=0,ignored=0)
        failed_names=[]
        unfinished=[]
        for row in failed_native["results"]:
            require(row["sourceManifestSha256"]==summary["sourceManifestSha256"]
                and row["runnerSha256"]==failed_runner_sha
                and sha(failed_files["native/"+row["label"]+".log"])==row["logSha256"],"Failed-attempt result identity differs")
            if row["inventoryOnly"]:
                continue
            raw=failed_files["native/"+row["label"]+".log"]
            if incomplete and row["label"]=="integration-e":
                require(row["exitCode"]==101 and "testGroups" not in row,
                    "Incomplete harness was unexpectedly admitted")
                lines=raw.decode("utf-8").splitlines(keepends=True)
                starts=[(position,match[1]) for position,line in enumerate(lines)
                    if (match:=parser.RUNNING.fullmatch(line.rstrip("\r\n")))]
                blocks=[(position,starts[index+1][0] if index+1<len(starts) else len(lines),executable)
                    for index,(position,executable) in enumerate(starts)
                    if parser.executable_target(executable,json.loads(failed_files["native/integration-inventory.json"]))=="skill_takeaways"]
                require(len(blocks)==1,"Unexpected unfinished target set")
                start,end,executable=blocks[0]
                block="".join(lines[start:end])
                observed={match[1]:match[2] for line in block.splitlines()
                    if (match:=parser.TEST_ROW.fullmatch(line))}
                registered=json.loads(failed_files["native/integration-inventory.json"])["skill_takeaways"]
                unresolved="a_successful_learning_never_makes_its_default_skill_view_unreadable"
                require(executable==registered["executable"] and len(registered["registered"])==6
                    and set(observed)==set(registered["registered"])-{unresolved}
                    and len(observed)==5 and all(state=="ok" for state in observed.values())
                    and not any(parser.TEST_END.fullmatch(line) for line in block.splitlines())
                    and "running 6 tests" in block
                    and unresolved+" has been running for over 60 seconds" in block
                    and "memory allocation of 1680427 bytes failed" in block
                    and "exit code: 0xc0000409, STATUS_STACK_BUFFER_OVERRUN" in block,
                    "Unfinished allocator-abort evidence differs")
                unfinished.append(dict(target="skill_takeaways",executable=executable,
                    observedRows=observed,unresolvedRegisteredTests=[unresolved],harnessSummaryPresent=False,
                    allocatorFailureBytes=1680427,processExitCode="0xc0000409",
                    contributesToAcceptedTotals=False))
                raw="".join(lines[:start]+lines[end:]).encode("utf-8")
            if row["label"]=="doctests":
                raw=re.sub(rb"^\s*Doc-tests (\S+)\r?$",rb" Running doctests (\1.exe)",raw,flags=re.MULTILINE)
            groups=parser.harness_groups(raw,False)
            if incomplete and row["label"]=="integration-e":
                group_records=[dict(group,target=parser.executable_target(group["executable"],
                    json.loads(failed_files["native/integration-inventory.json"]))) for group in groups]
            else:
                require(groups==[{key:value for key,value in group.items() if key!="target"} for group in row["testGroups"]],
                    "Failed-attempt raw groups differ")
                group_records=row["testGroups"]
            require(row["groups"]==len(groups) and all(row[key]==sum(group[key] for group in groups)
                for key in ["passed","failed","ignored"]),"Failed-attempt raw counters differ")
            for key in failed_totals:
                failed_totals[key]+=row[key]
            failed_names.extend((record["target"],name) for record in group_records
                for name,state in record["rows"].items() if state=="FAILED")
        require(all(failed_native[key]==value for key,value in failed_totals.items())
            and failed_names==[expected_failure],"Failed-attempt aggregate or failure names differ")
        require(len(unfinished)==int(incomplete),"Unexpected incomplete harness count")
        for phase in ["format","clippy","native"]:
            receipt=json.loads(failed_files[phase+"/receipt.json"])
            require(receipt["sourceManifestSha256"]==summary["sourceManifestSha256"]
                and receipt["runnerSha256"]==failed_runner_sha and not receipt["sourceChanges"]
                and json.loads(failed_files[phase+"/source-before.json"])==json.loads(failed_files[phase+"/source-after.json"])==frozen,
                "Failed-attempt phase identity differs")
            require(all(sha(failed_files[phase+"/"+name])==digest for name,digest in receipt["artifactSha256"].items()),
                "Failed-attempt artifact differs")
        failed_proofs[failed_tag]=dict(files=failed_files,launcher=failed_launcher)
        failed_attempts.append(dict(tag=failed_tag,accepted=False,contributesToAcceptedTotals=False,
            completeNativeMatrix=not incomplete,validationErrors=failed_native["validationErrors"],unfinishedHarnesses=unfinished,
            **{key:failed_native[key] for key in ["passed","failed","ignored","groups","sourceManifestSha256","runnerSha256"]},
            failedTests=["/".join(expected_failure)],
            interpretation=("Earlier scheduling profile, unchanged product/test sources and original deadlines. "
                "One observed deadline failure; precise contention cause is unproven. "
                +("Native matrix incomplete: the skill_takeaways process aborted after an allocator failure without a harness summary. "
                    "Receipt counters cover completed harnesses only; five individual observed successes are retained separately and never counted as an accepted target. " if incomplete else "")
                +"Retained raw evidence is never added or substituted into accepted totals.")))
    reference_failed=failed_proofs["meaning-join-windows-20261017-r1"]["files"]
    failure_name=expected_failures["meaning-join-windows-20261017-r1"][1]
    diagnostics={name:(BASE/name).read_bytes() for name in ["github-deadline-diagnostic.log","github-suite-diagnostic.log",
        "github-exact-verified.log","github-suite-verified.log","github-verified-receipt.json",
        "github-verified-launch.log","run_github_diagnostics.py"]}
    diagnostic=json.loads(diagnostics["github-verified-receipt.json"])
    github_inventory=json.loads(reference_failed["native/integration-inventory.json"])["github"]
    require(diagnostic["contributesToAcceptedTotals"] is False and not diagnostic["sourceChanges"]
        and diagnostic["sourceManifestSha256"]==summary["sourceManifestSha256"]
        and diagnostic["runnerSha256"]==sha(diagnostics["run_github_diagnostics.py"])
        =="d51804e3599e9fe7e2fac985ec5cc05d98d6b7cfc0b989b09c34bd928b3cea09"
        and diagnostic["integrationInventorySha256"]==sha(reference_failed["native/integration-inventory.json"])
        and diagnostic["executable"]==github_inventory["executable"]
        ==decode("native/integration-inventory.json")["github"]["executable"]
        and diagnostic["executableSha256"]==sha(Path(diagnostic["executable"]).read_bytes()),"Diagnostic execution/source identity differs")
    require([row["label"] for row in diagnostic["results"]]==["exact","suite"],"Diagnostic result selection differs")
    for row,arguments,names,filtered in zip(diagnostic["results"],
        [["--exact",failure_name,"--test-threads=1"],["--test-threads=4"]],
        [{failure_name},set(github_inventory["registered"])],[11,0],strict=True):
        require(row["command"]==[diagnostic["executable"],*arguments] and row["exitCode"]==0
            and row["sourceManifestSha256"]==diagnostic["sourceManifestSha256"]
            and row["executableSha256"]==diagnostic["executableSha256"]
            and sha(diagnostics[row["log"]])==row["logSha256"],"Diagnostic result proof differs")
        lines=diagnostics[row["log"]].decode("utf-8").splitlines()
        rows=[match for line in lines if (match:=parser.TEST_ROW.fullmatch(line))]
        ends=[match for line in lines if (match:=parser.TEST_END.fullmatch(line))]
        require(len(rows)==len(names) and {match[1] for match in rows}==names
            and all(match[2]=="ok" for match in rows) and len(ends)==1
            and [int(ends[0][n]) for n in range(2,7)]==[len(names),0,0,0,filtered],"Diagnostic raw names/counters differ")
    probes=snapshot(BASE/"development")
    host_observations={name:(BASE/name).read_bytes() for name in ["host-memory-diagnostic.json","memory-before-serial.json","host-storage-observation.json"]}
    recovery_diagnostics={name:(BASE/name).read_bytes() for name in ["run_recovery_diagnostic.py","run_recovery_ssd_diagnostic.py",
        "recovery-exact-verified.log","recovery-ssd-exact-verified.log","recovery-verified-receipt.json",
        "recovery-ssd-verified-receipt.json","recovery-verified-launch.log","recovery-ssd-verified-launch.log"]}
    storage_reference=failed_proofs["meaning-join-windows-20261017-serial-hosts"]["files"]
    storage_inventory=json.loads(storage_reference["native/library-inventory.json"])["yoke"]
    for prefix,script_name,state,expected_exit,passed,failed,drive in [
        ("recovery","run_recovery_diagnostic.py","FAILED",101,0,1,"E:"),
        ("recovery-ssd","run_recovery_ssd_diagnostic.py","ok",0,1,0,"C:")]:
        proof=json.loads(recovery_diagnostics[prefix+"-verified-receipt.json"])
        require(proof["sourceManifestSha256"]==summary["sourceManifestSha256"] and not proof["sourceChanges"]
            and proof["contributesToAcceptedTotals"] is False and proof["exitCode"]==expected_exit
            and proof["runnerSha256"]==sha(recovery_diagnostics[script_name])
            and proof["libraryInventorySha256"]==sha(storage_reference["native/library-inventory.json"])
            and proof["executable"]==storage_inventory["executable"]
            and proof["executableSha256"]==sha(Path(proof["executable"]).read_bytes())
            =="9bb343e1047080890b317e370b2eaf75c79eb95d63d5a1259888d788826e6d0f"
            and proof["command"]==[proof["executable"],"--exact",durable_name,"--test-threads=1"]
            and Path(proof["tempRoot"]).drive.upper()==drive
            and sha(recovery_diagnostics[proof["log"]])==proof["logSha256"],"Recovery diagnostic identity differs")
        lines=recovery_diagnostics[proof["log"]].decode("utf-8").splitlines()
        rows=[match for line in lines if (match:=parser.TEST_ROW.fullmatch(line))]
        ends=[match for line in lines if (match:=parser.TEST_END.fullmatch(line))]
        require(len(rows)==len(ends)==1 and rows[0][1]==durable_name and rows[0][2]==state and ends[0][1]==state
            and [int(ends[0][n]) for n in range(2,7)]==[passed,failed,0,0,len(storage_inventory["registered"])-1],
            "Recovery diagnostic raw outcome differs")
    integration=snapshot(BASE/"integration")
    require(json.loads(integration["receipt.json"])["accepted"] is True,"Missing checked integration receipt")
    admitted_integration=json.loads(integration["receipt.json"])
    integration_paths={"native/src/lib.rs","native/src/meaning_cli.rs","native/docs/meaning-cli.md",
        "native/src/meaning_join.rs","native/src/meaning_join_query.rs","native/src/meaning_join_query_tests.rs",
        "native/tests/meaning_join_cli.rs","native/docs/meaning-join.md"}
    require(set(admitted_integration["paths"])==integration_paths
        and admitted_integration["parentIndexSha256"]==parent["indexSha256"],"Integration path set or parent index differs")
    require(admitted_integration==dict(accepted=True,**json.loads(integration["proposal.json"])),"Integration proposal and receipt differ")
    parent_source=(ROOT/parent["sourceManifest"]).read_bytes()
    require(sha(parent_source)==parent["sourceManifestSha256"],"Parent frozen source map differs")
    parent_frozen=json.loads(parent_source)
    for name,proof in admitted_integration["paths"].items():
        original=integration.get("original/"+name)
        require(proof["before"]==parent_frozen.get(name)==(sha(original) if original is not None else None),"Integration preimage differs")
        require(proof["after"]==frozen[name],"Integrated reviewed delta differs from accepted source")
    require({name for name in integration if name.startswith("original/")}==
        {"original/"+name for name,proof in admitted_integration["paths"].items() if proof["before"] is not None},"Integration original path set differs")
    require(sha(integration["operation-executed.py"])=="bc71a39e8d5255650113d577998dd123267843d492343e6fe4c4574da797e559",
        "Executed integration helper differs")
    def current_without_evidence():
        prefix=DEST.relative_to(ROOT).as_posix()+"/"
        return {name:digest for name,digest in parser.source_manifest().items() if not name.startswith(prefix)}
    require(current_without_evidence()==frozen,"Source changed during proof validation")
    if args.validate_only:
        print(json.dumps(dict(ready=True,sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),
            cargoTargets=len(targets),native=raw_totals,newCliTests=len(cli_names),newUnitTests=len(unit_names))))
        return
    DEST.mkdir(parents=True,exist_ok=False)
    pins={}
    def archive(values,prefix):
        for name,raw in sorted(values.items()):
            relative=prefix+"/"+name;destination=DEST/relative
            require(destination.resolve().is_relative_to(DEST.resolve()),"Archive path escaped")
            destination.parent.mkdir(parents=True,exist_ok=True)
            with destination.open("xb") as stream:stream.write(raw)
            require(destination.read_bytes()==raw,"Archive bytes differ")
            pins[relative]=sha(raw)
    archive(files,"evidence/"+TAG)
    archive(launcher,"launcher")
    for tag,proof in failed_proofs.items():
        archive(proof["files"],"failed-attempts/"+tag+"/evidence")
        archive(proof["launcher"],"failed-attempts/"+tag+"/launcher")
    archive(diagnostics,"failed-attempts/meaning-join-windows-20261017-r1/diagnostics")
    archive(host_observations,"host-observations")
    archive(recovery_diagnostics,"failed-attempts/meaning-join-windows-20261017-serial-hosts/diagnostics")
    archive(probes,"development-probes")
    archive(integration,"integration")
    archive({"preserve_checkpoint.py":Path(__file__).read_bytes(),"integrate.py":(BASE/"integrate.py").read_bytes()},"operations")
    require(all(sha((DEST/name).read_bytes())==digest for name,digest in pins.items()),"Archive pins differ before index")
    claims=dict(universalParity=False,releaseCertified=False,newParityRowsCertified=False,fullUpstreamEquivalence=False,
        semanticIdentityEstablished=False,authenticatedReview=False,crossHistoryWorkspaceJoin=False,mixedDocumentView=False)
    index=dict(schemaVersion=1,kind="windows_checked_meaning_join_acceptance",accepted=True,acceptedTag=TAG,
        sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),rustFiles=sum(name.endswith(".rs") for name in frozen),
        phaseReceipts=summary["phaseReceipts"],artifactPins=pins,nonadditiveJoinCliTests=cli_names,
        nonadditiveJoinUnitTests=unit_names,postFreezeMetadataFiles=METADATA,claims=claims,
        failedAttempts=failed_attempts,
        hostObservationInterpretation="Separate snapshots record low system commit headroom and the reported HDD/NVMe drive mapping. Same-source, same-binary, same-argv exact recovery diagnostics fail on E and pass on C. Inclusive spans overlap; these observations do not establish a sole root cause or HDD/concurrent-load latency certification and never contribute to accepted totals.",
        validationProfile="All tests retain their original assertions, deadlines, security checks and durable flushes. The exact durable-write recovery fixture runs once on C alongside three disjoint ACL fixtures and real short-name tests. Complete query_parity, github and debug_adapter targets execute serially on E; other ordinary integration and library tests retain four threads. This is C-backed functional storage and scheduling evidence, not HDD or concurrent-load latency certification.",
        developmentProbeInterpretation="Separate nonadditive development only. Initial CLI RED0/8 absent command; first compile/link attempt failed on a nonexistent task TEMP before tests; after creating a fresh E TEMP, GREEN8/0. Added admission tests RED10/1 proved Serde unit-variant unknown-field bypass; changed Degree to empty struct. A subsequent build overlapped a source API update and failed compile E0061; it is not runtime evidence. Final stable scoped tests are3unit+13joinCLI+13existingCLI+7existinggroupadmission=36/0. Auxiliary TypeScript254/0/3skip+typecheck and fmt/all-targetclippy are separate. Only five final source-frozen phases supply accepted totals.",
        review="Independent read-only source/doc/test review Ready after query-selection preflight, isolated representative/rationale identity checks, independent hash oracle, deadline/cancellation and asymmetric direction/hyperedge tests. Reviewer ran no tests.",
        integrationInterpretation="The executed helper is pinned separately under integration/operation-executed.py. Under a quiescent checkout, all eight copied before/after identities matched the earlier admitted proposal and the accepted whole-source freeze. Review subsequently added a buffer-to-candidate seal and full auxiliary-manifest recheck to the future operations/integrate.py. That stronger helper was not retroactively executed; no concurrent hostile-writer atomicity is claimed.",
        interpretation="All five phases freshly executed on identical source and unchanged reviewed runner. Historical/scoped totals are nonadditive; ignored tests remain ignored.")
    index_raw=encoded(index)
    (DEST/"index.json").write_bytes(index_raw)
    (DEST/"index.sha256").write_text(sha(index_raw)+"\n",encoding="ascii")
    relative_index=(DEST/"index.json").relative_to(ROOT).as_posix()
    relative_bundle=(DEST/"evidence"/TAG).relative_to(ROOT).as_posix()
    phase_proof={name:dict(receipt=f"{relative_bundle}/{name}/receipt.json",**proof) for name,proof in summary["phaseReceipts"].items()}
    acceptance=dict(status="accepted_windows_development_checkpoint",platform="Windows",acceptedTag=TAG,
        index=relative_index,indexSha256=sha(index_raw),receipt=phase_proof["native"]["receipt"],sourceManifest=f"{relative_bundle}/source-freeze.json",
        sourceManifestSha256=summary["sourceManifestSha256"],runnerSha256=RUNNER_SHA,sourceFiles=len(frozen),rustFiles=index["rustFiles"],
        phaseProof=phase_proof,cargoTargets=len(targets),**{key:native[key] for key in ["groups","passed","failed","ignored"]},
        nativeMatrix={key:coverage[key] for key in ["integrationTargets","libraryRegistered","libraryIgnored"]},claims=claims,
        sourceChangesDuringEachGate=[],postFreezeMetadataFiles=METADATA,
        boundedCapabilities=["Explicit host grouping across selected published Meaning graphs sharing one pinned native workspace/snapshot/base graph; complete joint source/receipt replay in a frozen read transaction.",
            "Opaque checked projection supports original-member lookup, exact parallel/selfloop/raw-role-slot degree counts and bounded directed trace with induced relations and fully contained original hyperedges.",
            "All original graph-qualified claims, evidence, provider provenance, coverage and source-space bindings remain; deterministic compact UTF-8 identity, exact capacity/deadline handling and no provider/capture/persistence."],
        limitations=["Host presentation assertions do not establish semantic identity, authenticated review or automatic deduplication.",
            "Cross-history/workspace joins, mixed-document source spaces, generic serialized projection imports, model-quality and full upstream certification remain open."],
        inventoryBoundary=dict(originalIdentities=353,unreconciledReviewRequired=73,backendRuntimeUnknowns=77,identitiesStatusesAndCertificationsChanged=False),
        nonadditiveJoinCliTests=cli_names,nonadditiveJoinUnitTests=unit_names)
    reconciliation["eleventhWaveAcceptance"]=acceptance
    for scope in reconciliation["prioritizedRemainingScopes"]:
        if scope["id"]=="R2":
            scope["hostGroupingAddition"]="Explicit disjoint host-selected groups preserve one-parent group schema and now extend across selected native Meaning graphs sharing one exact workspace/canonical snapshot/base-graph pin. JoinedGraph is an opaque checked query capability with lookup, degree and bounded directed traces. Complete joint stored source/receipt replay preserves graph-qualified concepts, directed parallel observations, selfloops, ordered role slots, evidence, coverage and provenance. No parent mutation, provider, capture or persistence; host assertions establish neither semantic identity nor authenticated review. Automatic semantic decisions, cross-history/workspace joins, mixed-document spaces and generic serialized projection consumers remain open."
            scope["missingBehavior"]="Semantic alias identity/de-dup, mixed Office/PDF source spaces, generic portable serialized projection consumers and outcome learning remain open. Checked same-snapshot native joins and lookup/degree/directed-trace consumers are evidenced separately; exact transport never supplies semantic identity or model-quality proof."
            scope["suggestedScope"]="Preserve complete typed source/receipt replay and the bounded checked same-snapshot joins/analytics. Define broader mixed-document and cross-history/workspace contracts and reviewed semantic alias identity without granting source authority to labels or serialized JSON."
            scope["evidence"]=list(dict.fromkeys(scope["evidence"]+["native/src/meaning_join.rs","native/src/meaning_join_query.rs",
                "native/src/meaning_join_query_tests.rs","native/tests/meaning_join_cli.rs","native/docs/meaning-join.md"]))
    require(all(reconciliation[key]==value for key,value in ledger.items()),"Original ledger changed")
    require(all(reconciliation[key]==value for key,value in historical_reconciliation.items()),"Historical reconciliation acceptance changed")
    gates["eleventhWaveAcceptance"]=acceptance
    gates["latestWholeNativeRun"]=acceptance;gates["wholeNativeRun"]=acceptance
    gates["packagingPython"]=phase_proof["python"]
    gates["typescript"]=dict(phase_proof["typescript"],skipped=phases["typescript"]["ignored"],typecheck="passed",interpretation="Complete fresh frozen phase only; scoped and historical checks are nonadditive.")
    require(all(gates[key]==value for key,value in historical.items()),"Historical gate acceptance changed")
    paragraph=(f"**Accepted Wave 11 Windows development checkpoint.** Tag `{TAG}` passed {native['passed']} native tests/"
        f"{native['failed']} failures/{native['ignored']} ignored across {len(targets)} Cargo targets/{native['groups']} groups; "
        f"Python {phases['python']['passed']} and TypeScript {phases['typescript']['passed']}/{phases['typescript']['ignored']} skipped. "
        f"Its {len(frozen)}-file/{index['rustFiles']}-Rust source map is `{summary['sourceManifestSha256']}`. "
        "Explicit checked Meaning joins now group selected native graphs on one pinned canonical snapshot and provide opaque lookup/degree/directed-trace consumers. "
        "Joint frozen source/receipt replay preserves all original graph-qualified claims, evidence, coverage, parallel relations, selfloops and ordered role slots. "
        "Thirteen CLI and three asymmetric query regressions are included, never added to whole-project totals. No provider, capture or persistence is introduced. "
        "Three earlier attempts retained GitHub, debugger and durable-recovery deadline failures; the second native matrix was also incomplete after a skill_takeaways allocator abort without a harness summary. Its five partial successes remain uncounted. The recovery case also failed alone on E and passed with the identical binary/argv on C. The final functional profile runs that exact case on C and serializes complete host-protocol/query targets on E, preserving every test, deadline and durable flush. HDD/concurrent-load latency is not certified. Diagnostic and failed totals are nonadditive. "
        "Host assertions do not establish semantic identity or authenticated review; automatic deduplication, cross-history/workspace and mixed-document joins remain open. "
        "All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. "
        "This is an unreleased Windows development checkpoint; universal parity and release remain open. ")
    outputs={METADATA[1]:encoded(reconciliation),METADATA[2]:encoded(gates)}
    for name,link in [(METADATA[0],"validation/parity-windows-2026-10-17-wave11/index.json"),(METADATA[3],"../../docs/validation/parity-windows-2026-10-17-wave11/index.json")]:
        content=originals[name].decode("utf-8");marker="**Accepted Wave 10 Windows development checkpoint.**"
        require(content.count(marker)==1,"Status insertion marker differs")
        outputs[name]=content.replace(marker,paragraph+f"See the [preserved index]({link}).\n\n"+marker,1).encode()
    require(current_without_evidence()==frozen,"Source changed before metadata reconciliation")
    for name,raw in outputs.items():atomic(ROOT/name,raw,originals[name])
    current=current_without_evidence()
    require(set(current)==set(frozen),"Source inventory changed")
    changed=sorted(name for name in frozen if current[name]!=frozen[name])
    require(changed==sorted(METADATA),"Unexpected post-freeze changes")
    require(all(sha((DEST/name).read_bytes())==digest for name,digest in pins.items()),"Archived pins differ")
    (DEST/"post-freeze-metadata.json").write_bytes(encoded(dict(indexSha256=sha(index_raw),changedFrozenPaths=changed,
        hashes={name:current[name] for name in METADATA},originalLedgerUnchanged=True,historicalAcceptanceUnchanged=True)))
    print(json.dumps(dict(accepted=True,index=relative_index,indexSha256=sha(index_raw),artifactPins=len(pins),nativePassed=native["passed"],nativeIgnored=native["ignored"],changedFrozenPaths=changed)))

if __name__=="__main__":main()
