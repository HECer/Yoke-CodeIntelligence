"""Archive matching five-phase bytes and reconcile four status files only."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import types

ROOT = Path("G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence").resolve()
BASE = Path(__file__).resolve().parent
TAG = "outcome-feedback-windows-20261017-r1"
BUNDLE = ROOT / "target/parity-validation-20261015" / TAG
DEST = ROOT / "docs/validation/parity-windows-2026-10-17-wave13"
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
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".yoke-wave13-status-", delete=False) as stream:
            staged = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        require(staged.read_bytes()==data and path.read_bytes()==original, "Metadata changed while staging")
        os.replace(staged, path)
    finally:
        if staged is not None and staged.exists():
            staged.unlink()

def verified_parent_map(parent_index, parent, expected_index):
    def insist(ok, message):
        if not ok:
            raise RuntimeError(message)
    files = {}
    for path in sorted(parent_index.parent.rglob('*')):
        insist(not path.is_symlink() and path.resolve().is_relative_to(parent_index.parent.resolve()),
            'Parent archive path escaped')
        if path.is_file():
            files[path.relative_to(parent_index.parent).as_posix()] = path.read_bytes()
    value = json.loads(files['index.json'])
    insist(sha(files['index.json']) == expected_index == parent['indexSha256']
        == files['index.sha256'].decode().strip(), 'Parent index pin differs')
    insist(value['accepted'] is True and not any(value['claims'].values())
        and value['acceptedTag'] == parent['acceptedTag'] == 'mixed-view-windows-20261017-r1', 'Parent acceptance differs')
    special = {'index.json','index.sha256','post-freeze-metadata.json'}
    insist(not special.intersection(value['artifactPins']) and set(files) == set(value['artifactPins'])|special,
        'Parent archive file closure differs')
    insist(all(sha(files[name]) == digest for name,digest in value['artifactPins'].items()), 'Parent archive pin differs')
    prefix = 'evidence/'+value['acceptedTag']+'/'
    frozen_raw = files[prefix+'source-freeze.json']
    frozen = json.loads(frozen_raw)
    summary = json.loads(files[prefix+'summary.json'])
    insist(summary['accepted'] is True and sha(frozen_raw) == value['sourceManifestSha256']
        == parent['sourceManifestSha256'] == summary['sourceManifestSha256']
        and len(frozen) == value['sourceFiles'] == summary['sourceFiles'], 'Parent source freeze differs')
    runner = '5b1dbf5179f274d57fc0217108867d90d9a62ae233ca551496e89d386df81a00'
    insist(sha(files[prefix+'runner.py']) == summary['runnerSha256'] == runner, 'Parent runner differs')
    phases = ['format','clippy','native','python','typescript']
    insist(set(value['phaseReceipts']) == set(summary['phaseReceipts']) == set(phases), 'Parent phase closure differs')
    receipts = {}
    for phase in phases:
        raw = files[prefix+phase+'/receipt.json']
        receipt = json.loads(raw)
        receipts[phase] = receipt
        insist(sha(raw) == value['phaseReceipts'][phase]['receiptSha256']
            == summary['phaseReceipts'][phase]['receiptSha256'], 'Parent phase receipt pin differs')
        insist(receipt['accepted'] is True and receipt['phase'] == phase
            and not receipt['sourceChanges'] and not receipt['validationErrors']
            and receipt['sourceManifestSha256'] == sha(frozen_raw) and receipt['runnerSha256'] == runner,
            'Parent phase admission differs')
        insist(json.loads(files[prefix+phase+'/source-before.json'])
            == json.loads(files[prefix+phase+'/source-after.json']) == frozen, 'Parent phase source maps differ')
        insist(all(sha(files[prefix+phase+'/'+name]) == digest for name,digest in receipt['artifactSha256'].items()),
            'Parent phase artifact differs')
        for row in receipt['results']:
            insist(row['exitCode'] == row['failed'] == 0 and row['origin'] == 'fresh_execution'
                and row['sourceManifestSha256'] == sha(frozen_raw) and row['runnerSha256'] == runner
                and sha(files[prefix+phase+'/'+row['label']+'.log']) == row['logSha256'], 'Parent result differs')
        for key in ['passed','failed','ignored','groups']:
            insist(receipt[key] == sum(row[key] for row in receipt['results'])
                == value['phaseReceipts'][phase][key] == summary['phaseReceipts'][phase][key],
                'Parent phase aggregate differs')
    native = receipts['native']
    insist([native[k] for k in ['passed','failed','ignored','groups']] == [1898,0,33,244]
        and len(json.loads(files[prefix+'native/target-inventory.json'])) == 239,
        'Parent native inventory or total differs')
    raw_totals = [0,0,0,0]
    pattern = re.compile(rb'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', re.M)
    for row in native['results']:
        if row['inventoryOnly']:
            continue
        groups = pattern.findall(files[prefix+'native/'+row['label']+'.log'])
        actual = [sum(int(g[n]) for g in groups) for n in range(3)]+[len(groups)]
        insist(actual == [row[k] for k in ['passed','failed','ignored','groups']], 'Parent raw native totals differ')
        raw_totals = [a+b for a,b in zip(raw_totals,actual)]
    insist(raw_totals == [1898,0,33,244] and receipts['python']['passed'] == 11
        and receipts['typescript']['passed'] == 254 and receipts['typescript']['ignored'] == 3,
        'Parent language/raw totals differ')
    post = json.loads(files['post-freeze-metadata.json'])
    metadata = set(METADATA)
    insist(post['indexSha256'] == expected_index and set(post['hashes']) == metadata
        and set(post['changedFrozenPaths']) == metadata and set(value['postFreezeMetadataFiles']) == metadata,
        'Parent post-freeze metadata differs')
    insist(all(name in frozen and re.fullmatch('[0-9a-f]{64}', digest) for name,digest in post['hashes'].items()),
        'Invalid parent metadata identity')
    result = dict(frozen)
    result.update(post['hashes'])
    archive_prefix = parent_index.parent.relative_to(ROOT).as_posix()+'/'
    insist(not any(name.startswith(archive_prefix) for name in frozen), 'Parent archive already in frozen map')
    included = set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','--',
        parent_index.parent.relative_to(ROOT).as_posix()],cwd=ROOT).decode().split('\0'))-{''}
    insist(included <= {archive_prefix+name for name in files}, 'Unexpected Git-admitted parent proof path')
    result.update({archive_prefix+name:sha(raw) for name,raw in files.items() if archive_prefix+name in included})
    return result, frozen


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
    parser = types.ModuleType("wave13_reviewed_parser")
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


    regression_path="native/tests/feedback_cli.rs"
    require(sha((ROOT/regression_path).read_bytes())==frozen[regression_path],"Feedback regression source differs")
    cli_names=sorted(re.findall(r"#\[test\]\s*fn\s+(\w+)\s*\(",(ROOT/regression_path).read_text(encoding="utf-8")))
    require(len(cli_names)==21 and set(cli_names)==seen[("test","feedback_cli")]
        and not set(cli_names).intersection(inventory_ignored[("test","feedback_cli")]),"Feedback regression names missing or ignored")
    require(native["passed"]==1919 and native["failed"]==0 and native["ignored"]==33
        and native["groups"]==245 and len(targets)==240,"Whole native totals differ")
    require(phases["python"]["passed"]==11 and phases["typescript"]["passed"]==254
        and phases["typescript"]["ignored"]==3,"Language gate differs")
    originals={name:(ROOT/name).read_bytes() for name in METADATA}
    require(all(sha(raw)==frozen[name] for name,raw in originals.items()),"Metadata differs from freeze")
    reconciliation=json.loads(originals[METADATA[1]])
    ledger=copy.deepcopy({key:reconciliation[key] for key in ["inventoryLedger","ledgerSummary"]})
    gates=json.loads(originals[METADATA[2]])
    historical=copy.deepcopy({key:value for key,value in gates.items()
        if key.endswith("WaveAcceptance") or key=="meaningGroupProjectionScopedChecks"})
    historical_reconciliation=copy.deepcopy({key:value for key,value in reconciliation.items() if key.endswith("WaveAcceptance")})
    parent=gates["twelfthWaveAcceptance"]
    parent_index=ROOT/parent["index"]
    require(parent["index"]=="docs/validation/parity-windows-2026-10-17-wave12/index.json"
        and sha(parent_index.read_bytes())==parent["indexSha256"],"Parent index differs")
    parent_value=json.loads(parent_index.read_bytes())
    require(parent_value["accepted"] is True and not any(parent_value["claims"].values()),"Unaccepted parent checkpoint")
    parent_evidence="evidence/"+parent_value["acceptedTag"]
    parent_bundle=parent_index.parent.relative_to(ROOT).as_posix()+"/"+parent_evidence
    require(parent["acceptedTag"]==parent_value["acceptedTag"]=="mixed-view-windows-20261017-r1"
        and parent["receipt"]==parent_bundle+"/native/receipt.json"
        and parent["sourceManifest"]==parent_bundle+"/source-freeze.json","Parent evidence path differs")
    for name,digest in parent_value["artifactPins"].items():
        path=parent_index.parent/name
        require(path.resolve().is_relative_to(parent_index.parent.resolve()) and not path.is_symlink()
            and sha(path.read_bytes())==digest,"Parent archive pin differs")
    parent_native_raw=(ROOT/parent["receipt"]).read_bytes()
    parent_native=json.loads(parent_native_raw)
    require(sha(parent_native_raw)==parent_value["artifactPins"][parent_evidence+"/native/receipt.json"]
        ==parent_value["phaseReceipts"]["native"]["receiptSha256"],"Parent native receipt pin differs")
    require(parent_native["accepted"] is True and parent_native["sourceManifestSha256"]==parent["sourceManifestSha256"]
        ==parent_value["sourceManifestSha256"] and all(parent_native[key]==parent[key]
        ==parent_value["phaseReceipts"]["native"][key] for key in ["passed","failed","ignored","groups"]),"Parent native receipt differs")
    require(native["passed"]==parent["passed"]+len(cli_names) and parent["passed"]==1898
        and parent["ignored"]==33 and parent["failed"]==0,"Native regression delta differs")
    mapping=reconciliation["currentBehaviorMapping"]
    require(mapping["path"]=="docs/parity/serena-facade-mapping-2026-10-14.json"
        and mapping["sha256"]==frozen[mapping["path"]]==sha((ROOT/mapping["path"]).read_bytes()),"Current facade mapping pin differs")
    launcher=snapshot(BASE/"acceptance"/TAG)
    probes=snapshot(BASE/"development")
    roots={name:(BASE/name).read_bytes() for name in ["baseline-source-freeze.json","contract.md","contract_red.py",
        "cli-red.log","cli-red-r2.log","run_contract_green.py","run_scoped.py","dependency-cache.json",
        "prepare_dependency_cache.py","setup.json"]}
    require(sha(roots["baseline-source-freeze.json"])=="1ef41d7564050c370f7db36fd539e0db1fa081248fe282db4997c46572d09e26",
        "Scoped baseline pin differs")
    require(sha(probes["review-candidate-source-freeze.json"])=="d6510d06d4cc8d3f53a0f375f4801e2529e24b16c7a5a960e47bafcfb4c4f5ec",
        "Reviewed candidate pin differs")
    integration=snapshot(BASE/"integration")
    admitted=json.loads(integration["receipt.json"])
    integration_paths={"native/src/knowledge_feedback.rs","native/src/knowledge_feedback_store.rs",
        "native/src/knowledge_feedback_rank.rs","native/src/knowledge_feedback_provenance.rs",
        "native/src/knowledge_feedback_transcript.rs","native/src/feedback_cli.rs","native/tests/feedback_cli.rs",
        "native/docs/outcome-feedback.md","native/src/knowledge.rs","native/src/main.rs","native/src/retention.rs",
        "native/src/meaning_build.rs","native/src/graph_view_comparison_input.rs","native/README.md"}
    require(admitted==dict(accepted=True,**json.loads(integration["proposal.json"]))
        and set(admitted["paths"])==integration_paths and admitted["parentIndexSha256"]==parent["indexSha256"],
        "Integration proposal, receipt, path set or parent differs")
    parent_source=(ROOT/parent["sourceManifest"]).read_bytes()
    require(sha(parent_source)==admitted["parentSourceManifestSha256"]==parent["sourceManifestSha256"]
        ==parent_value["sourceManifestSha256"]==parent_value["artifactPins"][parent_evidence+"/source-freeze.json"],
        "Parent source map differs")
    require(admitted["reviewedBaselineSourceManifestSha256"]==sha(roots["baseline-source-freeze.json"])
        and admitted["candidateSourceManifestSha256"]==sha(probes["review-candidate-source-freeze.json"]),
        "Integration development source pins differ")
    parent_frozen=json.loads(parent_source)
    candidate=json.loads(probes["review-candidate-source-freeze.json"])
    baseline=json.loads(roots["baseline-source-freeze.json"])
    require({name for name in set(candidate)|set(baseline) if candidate.get(name)!=baseline.get(name)}==integration_paths,
        "Reviewed auxiliary delta differs")
    for name,proof in admitted["paths"].items():
        original=integration.get("original/"+name)
        require(proof["before"]==parent_frozen.get(name)==baseline.get(name)
            ==(sha(original) if original is not None else None),"Integration original differs")
        require(proof["after"]==frozen[name]==candidate[name],"Integrated reviewed source differs")
    require({name for name in integration if name.startswith("original/")}==
        {"original/"+name for name,proof in admitted["paths"].items() if proof["before"] is not None},"Integration original set differs")
    require(sha(integration["operation-executed.py"])=="7e65528091d28639b8b27d18b6b473c8cbdf92c2a095467dce34be879936e863","Executed integration helper differs")
    before_raw = integration["source-before.json"]
    after_raw = integration["source-after.json"]
    require(sha(before_raw)==admitted["beforeSourceManifestSha256"]
        and sha(after_raw)==admitted["afterSourceManifestSha256"],"Integration whole-map pins differ")
    before_map, after_map = json.loads(before_raw), json.loads(after_raw)
    expected_before, parent_map = verified_parent_map(parent_index,parent,admitted["parentIndexSha256"])
    require(before_map==expected_before and parent_map==parent_frozen,"Integration whole parent map differs")
    expected_after=dict(before_map)
    expected_after.update({name:proof["after"] for name,proof in admitted["paths"].items()})
    require(after_map==expected_after==frozen,"Accepted whole source differs from exact integration result")
    parent_changes={name for name in set(parent_frozen)|set(baseline) if parent_frozen.get(name)!=baseline.get(name)}
    require(all(name in METADATA or (name.startswith("docs/validation/parity-windows-2026-10-17-wave11/")
        and name not in baseline) for name in parent_changes),"Parent exceeds reviewed product baseline")
    def current_without_evidence():
        prefix=DEST.relative_to(ROOT).as_posix()+"/"
        return {name:digest for name,digest in parser.source_manifest().items() if not name.startswith(prefix)}
    require(current_without_evidence()==frozen,"Source changed during proof validation")
    if args.validate_only:
        print(json.dumps(dict(ready=True,sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),
            cargoTargets=len(targets),native=raw_totals,newFeedbackCliTests=len(cli_names))))
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
    archive(probes,"development-probes")
    archive(roots,"development-inputs")
    archive(integration,"integration")
    archive({name:(BASE/name).read_bytes() for name in ["preserve_checkpoint.py","integrate.py","verify_checkpoint.py"]},"operations")
    require(all(sha((DEST/name).read_bytes())==digest for name,digest in pins.items()),"Archive pins differ before index")
    claims=dict(universalParity=False,releaseCertified=False,newParityRowsCertified=False,fullUpstreamEquivalence=False,
        completeOutcomeLearningParity=False,generalSemanticTranscriptUnderstanding=False,authenticatedIndependentCorroboration=False)
    index=dict(schemaVersion=1,kind="windows_source_bound_feedback_acceptance",accepted=True,acceptedTag=TAG,
        sourceManifestSha256=summary["sourceManifestSha256"],sourceFiles=len(frozen),rustFiles=sum(name.endswith(".rs") for name in frozen),
        phaseReceipts=summary["phaseReceipts"],artifactPins=pins,nonadditiveFeedbackCliTests=cli_names,
        postFreezeMetadataFiles=METADATA,claims=claims,
        validationProfile="Original assertions, deadlines, security checks and durable flushes remain. The exact16-root durability fixture runs on C alongside three ACL cases and real short-name tests; query/github/debug targets are serial on E and other ordinary library/integration tests retain four threads. C-backed functional storage profile only; no HDD/concurrent-load latency certification.",
        developmentProbeInterpretation="Separate nonadditive development only: unchanged three CLI contract cases RED0/3 on prior executable, thenGREEN3/0 on the new binary. Twenty-one feedback,107 existing regression and15 comparison adapter tests passed in scoped development. A test-only type alias followed, then21feedback tests,all-target Clippy,fmt and a fresh254/3skipped TypeScript run passed on candidate d651. Earlier compile/oracle failures, Clippy allocator abort and incomplete interrupted TypeScript r1 (235observed tests,230passes,two failed file harnesses,three skips and Zone Allocation OOM output) remain explicit historical evidence and never contribute to accepted totals. Only final five frozen phases supply accepted totals.",
        review="Independent read-only source/doc/test review Ready after complete source receipt replay, original provenance, supersession proof closure, capacity and host path guards. Final fourteen source hashes checked. Reviewer ran no tests.",
        integrationInterpretation="Reviewed helper seals exactly fourteen auxiliary byte buffers and parent preimages, requires the complete reviewed candidate map, checks absence including ignored targets, rechecks both inventories and verifies the final whole manifest. Only the parent's recorded status and preserved Wave11 evidence may extend the reviewed product baseline. Complete before/after maps are pinned and replayed against the parent archive while retaining the original Git source admission policy; accepted source must equal the entire integration result. Per-file optimistic replacement under a quiescent checkout; no hostile-writer CAS or multi-file atomic transaction is claimed.",
        interpretation="Explicit immutable source-bound knowledge outcomes and host-selected plain UTF-8 transcripts; original View/overlay/Meaning receipt/coverage scopes are replayed without capture or providers. Transparent latest-label-sequence votes and integer hyperbolic decay do not establish execution truth or independent corroboration. Literal transcript candidates require host selection. Historical/scoped totals are nonadditive, ignored tests retained.")
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
        boundedCapabilities=["Explicit source-bound knowledge feedback validates complete original and evaluated View proofs, exact primary-source or explicitly imported UTF-8 transcript quotes, immutable IDs and unique ordered label sequences.",
            "Read-only ranking replays event sources, exposes every contribution, excludes future events and older label sequences, uses bounded integer hyperbolic decay and gives stale or proved authored-superseded knowledge zero effective score.",
            "Explicit transcript import preserves original UTF-8/CRLF bytes; bounded literal candidate mining creates no events. Writes require host permission, checked quotas and admitted output; retention replays source proofs and protects transcript CAS."],
        limitations=["Knowledge records are the supported target. Broader object outcomes and general semantic transcript understanding remain open; literal candidates can include negated or quoted signals and require host selection.",
            "Actor/session, timestamps, outcomes, transcript author labels and corroboration are host assertions, not authentication, independent observations or execution proof. No capture, provider call, project execution or automatic session discovery.",
            "Only explicit checked source scopes can prove an authored successor; absent third-snapshot proof refuses. Immutable transcript CAS can be orphaned after later I/O failure. No multi-resource atomicity, universal or release certification."],
        inventoryBoundary=dict(originalIdentities=353,unreconciledReviewRequired=73,backendRuntimeUnknowns=77,identitiesStatusesAndCertificationsChanged=False),
        nonadditiveFeedbackCliTests=cli_names)
    reconciliation["thirteenthWaveAcceptance"]=acceptance
    for scope in reconciliation["prioritizedRemainingScopes"]:
        if scope["id"]=="R9":
            scope["knowledgeFeedbackAddition"]="Explicit source-bound knowledge outcomes now carry exact quotes, checked View/Meaning receipt closure and unique ordered actor/session label sequences. Ranking exposes latest-sequence votes, integer hyperbolic decay and stale/proved-supersession exclusion. Host-selected plain UTF-8 transcripts support literal candidates only; no provider or automatic session discovery."
            scope["missingBehavior"]="Broader object targets and semantic transcript understanding beyond literal candidate matching remain open. Independent corroboration or execution truth is not established by host labels or supplied outcomes. The bounded knowledge-only event, ranking and explicit transcript slice has separate Windows evidence."
            scope["suggestedScope"]="Retain immutable source proofs and transparent arithmetic. Define typed outcomes for additional object targets and explicit semantic transcript candidates without granting source/write authority, inventing execution proof or replacing the central AgentMemory backend."
            scope["evidence"]=list(dict.fromkeys(scope["evidence"]+sorted(integration_paths)))
    for capability in reconciliation["reconciledCapabilities"]:
        if capability["id"]=="meaning_and_learning_partial_foundations":
            capability["current"]+=" Explicit source-bound knowledge feedback additionally supports immutable outcomes, transparent latest-label-sequence ranking with integer hyperbolic decay and host-selected UTF-8 transcript literal candidates."
            capability["boundary"]="Native Meaning inference/transport and bounded feedback have separate checked proofs. Semantic alias identity, broader mixed-source consumers, non-knowledge outcome targets and semantic transcript understanding remain open. Provider/host assertions are not execution truth or independently authenticated corroboration."
            capability["implementation"]=list(dict.fromkeys(capability["implementation"]+[name for name in sorted(integration_paths) if name.startswith("native/src/knowledge_feedback")]))
            capability["tests"]=list(dict.fromkeys(capability["tests"]+[regression_path]))
    require(all(reconciliation[key]==value for key,value in ledger.items()),"Original ledger changed")
    require(all(reconciliation[key]==value for key,value in historical_reconciliation.items()),"Historical acceptance changed")
    gates["thirteenthWaveAcceptance"]=acceptance
    gates["latestWholeNativeRun"]=acceptance;gates["wholeNativeRun"]=acceptance
    gates["packagingPython"]=phase_proof["python"]
    gates["typescript"]=dict(phase_proof["typescript"],skipped=phases["typescript"]["ignored"],typecheck="passed",interpretation="Complete fresh frozen phase only; scoped and historical checks are nonadditive.")
    require(all(gates[key]==value for key,value in historical.items()),"Historical gate acceptance changed")
    paragraph=(f"**Accepted Wave 13 Windows development checkpoint.** Tag `{TAG}` passed {native['passed']} native tests/"
        f"{native['failed']} failures/{native['ignored']} ignored across {len(targets)} Cargo targets/{native['groups']} groups; "
        f"Python {phases['python']['passed']} and TypeScript {phases['typescript']['passed']}/{phases['typescript']['ignored']} skipped. "
        f"Its {len(frozen)}-file/{index['rustFiles']}-Rust source map is `{summary['sourceManifestSha256']}`. "
        "Explicit knowledge feedback now replays original View/overlay/Meaning receipts, checks exact source/transcript quotes and preserves immutable ordered event history. "
        "Ranking exposes latest actor/session-label votes, integer hyperbolic decay and zero effective scores for stale or proved authored-superseded knowledge. "
        "Plain UTF-8 transcript import is host-selected; literal candidates create no events. Twenty-one new CLI regressions are included, never added to whole-project totals. "
        "No capture, provider, project execution or automatic session discovery occurs. Broader targets and semantic transcript understanding remain open; host labels do not prove independent corroboration or execution truth. "
        "All 353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications remain. "
        "This is an unreleased Windows development checkpoint; universal parity and release remain open. ")
    outputs={METADATA[1]:encoded(reconciliation),METADATA[2]:encoded(gates)}
    for name,link in [(METADATA[0],"validation/parity-windows-2026-10-17-wave13/index.json"),(METADATA[3],"../../docs/validation/parity-windows-2026-10-17-wave13/index.json")]:
        content=originals[name].decode("utf-8");marker="**Accepted Wave 12 Windows development checkpoint.**"
        require(content.count(marker)==1,"Status insertion marker differs")
        outputs[name]=content.replace(marker,paragraph+f"See the [preserved index]({link}).\n\n"+marker,1).encode()
    require(current_without_evidence()==frozen,"Source changed before reconciliation")
    for name,raw in outputs.items():atomic(ROOT/name,raw,originals[name])
    current=current_without_evidence()
    require(set(current)==set(frozen) and sorted(name for name in frozen if current[name]!=frozen[name])==sorted(METADATA),"Unexpected post-freeze source changes")
    require(all(sha((DEST/name).read_bytes())==digest for name,digest in pins.items()),"Archived pins differ")
    (DEST/"post-freeze-metadata.json").write_bytes(encoded(dict(indexSha256=sha(index_raw),changedFrozenPaths=sorted(METADATA),
        hashes={name:current[name] for name in METADATA},originalLedgerUnchanged=True,historicalAcceptanceUnchanged=True)))
    print(json.dumps(dict(accepted=True,index=relative_index,indexSha256=sha(index_raw),artifactPins=len(pins),nativePassed=native["passed"],nativeIgnored=native["ignored"],changedFrozenPaths=sorted(METADATA))))

if __name__=="__main__":main()
