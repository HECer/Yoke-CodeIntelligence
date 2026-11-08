"""Fresh frozen-source Windows checks for meaning group projection only.

This never performs or claims another whole-native acceptance. Inventory/list
results and the prior Wave8 run do not contribute to the 47-test scoped total.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
import types

ROOT = Path("G:/NN-Developed/Dev-Tools/Yoke-CodeIntelligence/Code-Intelligence").resolve()
RUNNER = Path(__file__).resolve()
PARSER = ROOT / "target/parity-validation-20261015/acceptance_wave8.py"
PARSER_SHA = "412b796dff54e94a5ba1076204c28646f85dc1a769a4e7afa0428416b07f6a6b"
BASE = ["cargo", "test", "--workspace", "--locked", "--offline", "-j", "1", "--no-fail-fast"]

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True)+"\n").encode("utf-8")

def write(path, value):
    with path.open("xb") as stream:
        stream.write(encoded(value))

def main():
    arguments = argparse.ArgumentParser()
    arguments.add_argument("--tag", required=True)
    args = arguments.parse_args()
    require(re.fullmatch(r"[a-z0-9-]{1,100}",args.tag), "Invalid scoped evidence tag")
    pinned_parser = PARSER.read_bytes()
    require(sha(pinned_parser) == PARSER_SHA, "Reviewed parser changed")
    parser = types.ModuleType("reviewed_wave8_parser")
    parser.__file__ = str(PARSER)
    # Execute exactly the hashed source bytes; never an imported bytecode cache.
    exec(compile(pinned_parser,str(PARSER),"exec"),parser.__dict__)
    require(parser.ROOT.resolve()==ROOT,"Reviewed source inventory root differs")
    output = RUNNER.parent / "final" / args.tag
    output.mkdir(parents=True,exist_ok=False)
    fixture = Path("E:/Yoke-Test-Scratch") / args.tag
    fixture.mkdir(exist_ok=False)
    environment = os.environ.copy()
    environment.update(CARGO_HOME="E:/Yoke-Build-Cache/cargo-home-yoke-20261017",
        CARGO_TARGET_DIR="E:/Yoke-Build-Cache/meaning-aliases-20261016", TEMP=str(fixture),TMP=str(fixture),
        PYTHONUTF8="1", NO_COLOR="1", CARGO_TERM_COLOR="never")
    frozen = parser.source_manifest()
    runner_sha = sha(RUNNER.read_bytes())
    source_sha = sha(encoded(frozen))
    write(output/"source-freeze.json",frozen)
    write(output/"source-before.json",frozen)
    (output/"runner.py").write_bytes(RUNNER.read_bytes())
    (output/"parser.py").write_bytes(pinned_parser)
    freeze = dict(sourceManifestSha256=source_sha,runnerSha256=runner_sha,parserSha256=PARSER_SHA,
        sourceFiles=len(frozen),rustFiles=sum(name.endswith(".rs") for name in frozen),fixtureRoot=str(fixture))
    write(output/"freeze.json",freeze)
    results = []

    def check():
        require(parser.source_manifest()==frozen,"Source inventory or bytes changed during scoped checks")
        require(sha(RUNNER.read_bytes())==runner_sha and sha(PARSER.read_bytes())==PARSER_SHA,
            "Scoped runner or reviewed parser changed")

    def run(label, command, inventory=False):
        check()
        print(json.dumps(dict(running=label,command=command,log=str(output/(label+".log")))),flush=True)
        started = time.monotonic()
        with (output/(label+".log")).open("xb") as stream:
            status = subprocess.run(["rtk","proxy",*command],cwd=ROOT,env=environment,
                stdout=stream,stderr=subprocess.STDOUT).returncode
        raw = (output/(label+".log")).read_bytes()
        result = dict(label=label,command=command,exitCode=status,inventoryOnly=inventory,
            seconds=round(time.monotonic()-started,3),logSha256=sha(raw),origin="fresh_execution",
            sourceManifestSha256=source_sha,runnerSha256=runner_sha)
        if not inventory:
            summaries = [match for line in raw.decode("utf-8",errors="strict").splitlines()
                         if (match:=parser.TEST_END.fullmatch(line)) is not None]
            result["observedHarnessTotals"] = dict(groups=len(summaries),
                **{key:sum(int(match[index]) for match in summaries)
                   for key,index in (("passed",2),("failed",3),("ignored",4))})
        results.append(result)
        check()
        require(status==0,"Scoped command failed: "+label)
        return result,raw

    errors = []
    try:
        run("format",["cargo","fmt","--all","--","--check"])
        run("clippy",["cargo","clippy","--workspace","--all-targets","--locked","--offline","-j","1","--","-D","warnings"])
        cases = [
            ("group-core",["--lib","meaning_groups::"],["yoke"],15),
            ("aliases",["--lib","meaning_aliases::"],["yoke"],12),
            ("cli-admission",["--test","meaning_cli","--test","meaning_groups_admission"],["meaning_cli","meaning_groups_admission"],20),
        ]
        for label,selectors,names,expected in cases:
            inventory_result,raw = run(label+"-inventory",BASE+selectors+["--","--list"],inventory=True)
            registered = parser.group_map(parser.harness_groups(raw,True),names)
            require(sum(len(group["rows"]) for group in registered.values())==expected,"Scoped inventory count drift")
            inventory_result["testGroups"] = registered
            result,raw = run(label,BASE+selectors+["--","--format","pretty","--test-threads=1"])
            executed = parser.group_map(parser.harness_groups(raw,False),names)
            for name in names:
                require(registered[name]["executable"]==executed[name]["executable"],"Scoped executable identity differs")
                require(set(registered[name]["rows"])==set(executed[name]["rows"]),"Scoped name coverage differs")
                require(all(state=="ok" for state in executed[name]["rows"].values()),"A scoped test did not pass")
            result.update(testGroups=executed,passed=sum(group["passed"] for group in executed.values()),
                failed=sum(group["failed"] for group in executed.values()),ignored=sum(group["ignored"] for group in executed.values()))
            require(result["passed"]==expected and result["failed"]==result["ignored"]==0,"Scoped result count drift")
    except Exception as error:
        errors.append(type(error).__name__+": "+str(error))
    after = parser.source_manifest()
    write(output/"source-after.json",after)
    changed = sorted(name for name in frozen.keys() | after.keys() if frozen.get(name)!=after.get(name))
    test_results = [result for result in results if "passed" in result]
    group_names = sorted(name for result in test_results for group in result["testGroups"].values()
        for name in group["rows"] if name.startswith("meaning_groups::") or name.startswith("meaning_group_"))
    if len(group_names)!=len(set(group_names)) or len(group_names)!=25:
        errors.append("Missing or duplicate unique meaning group regression names")
    passed = sum(result["passed"] for result in test_results)
    if passed!=47:
        errors.append("Incomplete 47-test scoped matrix")
    observed = {key:sum(result.get("observedHarnessTotals",{}).get(key,0) for result in results)
                for key in ("groups","passed","failed","ignored")}
    if not errors and observed!={"groups":4,"passed":47,"failed":0,"ignored":0}:
        errors.append("Raw completed harness summaries differ from validated scoped matrix")
    if sha(RUNNER.read_bytes())!=runner_sha or sha(PARSER.read_bytes())!=PARSER_SHA:
        errors.append("Scoped runner or parser changed at finalization")
    accepted = not errors and not changed
    artifacts = {path.name:sha(path.read_bytes()) for path in sorted(output.iterdir()) if path.is_file()}
    receipt = dict(checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),kind="windows_meaning_groups_scoped_checks",
        **freeze,results=results,sourceChanges=changed,validationErrors=errors,scopedNativePassed=passed if accepted else None,
        scopedNativeFailed=0 if accepted else None,scopedNativeIgnored=0 if accepted else None,
        observedCompletedHarnessTotals=observed,
        nonadditiveMeaningGroupTests=group_names,artifactSha256=artifacts,accepted=accepted,
        claims=dict(newWholeNativeAcceptance=False,universalParity=False,semanticIdentityEstablished=False,
            authenticatedHumanReview=False,crossGraphDeduplication=False,releaseCertified=False),
        interpretation="Fresh Windows scoped matrix only:15 meaning-group unit tests,12 alias unit tests,13 complete meaning CLI tests and7 public group-admission tests. 25 unique group tests and all scoped counts are nonadditive to the separate historical Wave8 whole-project result. Rejected runs have null validated scoped counters; observedCompletedHarnessTotals retains raw completed-summary counts, not a complete/accepted matrix.")
    write(output/"receipt.json",receipt)
    print(json.dumps({key:value for key,value in receipt.items() if key not in {"results","artifactSha256","nonadditiveMeaningGroupTests"}}),flush=True)
    return 0 if receipt["accepted"] else 1

if __name__ == "__main__":
    raise SystemExit(main())
