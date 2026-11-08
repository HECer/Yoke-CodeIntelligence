"""Render four ignored metadata candidates from root-verified preserved Wave7 facts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat

STAGING = Path(__file__).absolute().parent
ROOT = STAGING.parent.parent.parent
INDEX = "docs/validation/parity-windows-2026-10-16-wave7/index.json"
WAVE7 = INDEX.rsplit("/", 1)[0]
RUNNER_SHA = "b51bbffbddd07be0aa6ef019fc950d772c628d77b3567f646d7a837726aa3d15"
BEFORE = {
    "docs/STATUS.md": "7d866fe8daf3ad44d9cfe0bd9127c001aa5d2f1a5a0256a44450c17beca7ba86",
    "docs/parity/native-reconciliation-2026-10-15.json": "4daf9325596e2a6aa96086032ccbcc40868305c908c071095028266d9333a70a",
    "docs/parity/windows-development-gates-2026-10-15.json": "bfcd9a50413e5978eb739c6a8ffb082825fc2fae01b528fdf5c8aca489f265ea",
    "native/docs/windows-parity-2026-10-15.md": "7fb499f0212bf4f1185a7c508ae0fd776aaed45363fb34bb49a0ed8ca3820064",
}
PHASES = ("format", "clippy", "native", "python", "typescript")
COUNTS = ("groups", "passed", "failed", "ignored")
PREVIOUS_MARKER = b"**Accepted Wave 6 Windows development checkpoint.**"
CURRENT_MARKER = "**Accepted Wave 7 Windows development checkpoint.**"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def valid_sha(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def parse(raw):
    def unique(items):
        value = {}
        for key, item in items:
            require(key not in value, "Duplicate JSON field")
            value[key] = item
        return value
    return json.loads(raw.decode("utf-8"), object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("Nonfinite JSON")))


def json_bytes(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").replace("\n", "\r\n").encode("utf-8")


def accepted_root(index_raw, verified_sha, reviewed_tag, reviewed_source_sha):
    require(valid_sha(verified_sha) and sha(index_raw) == verified_sha, "Root-verified wave7 index digest required")
    require(valid_sha(reviewed_source_sha), "Root-reviewed accepted source digest required")
    require(isinstance(reviewed_tag, str) and re.fullmatch(r"affected-r5b-[a-z0-9][a-z0-9-]{0,100}", reviewed_tag),
            "Root-reviewed affected acceptance tag required")
    index = parse(index_raw)
    require(index["schemaVersion"] == 1 and index["kind"] == "affected_windows_development_checkpoint"
            and index["claims"]["universalParity"] is False
            and index["claims"]["releaseCertified"] is False
            and index["claims"]["newParityRowsCertified"] is False
            and index["sourceManifestSha256"] == reviewed_source_sha and index["runnerSha256"] == RUNNER_SHA
            and index["acceptedPhases"] == list(PHASES) and index["rawCountsAreNonadditive"] is True,
            "Not the expected bounded five-phase Windows checkpoint")
    tag = index["acceptedTag"]
    require(tag == reviewed_tag, "Index does not describe the root-reviewed accepted tag")
    require(isinstance(index["sha256"], dict) and len(index["sha256"]) <= 20_000, "Unbounded preservation index")
    require(set(index["phaseResults"]) == set(PHASES), "Unexpected accepted phase set")
    return index, WAVE7 + "/evidence/" + tag


def needed_paths(index_raw, verified_sha, reviewed_tag, reviewed_source_sha):
    _, base = accepted_root(index_raw, verified_sha, reviewed_tag, reviewed_source_sha)
    return [base + "/source-freeze.json", base + "/summary.json",
            *[base + "/" + phase + "/receipt.json" for phase in PHASES]]


def checkpoint(index_raw, verified_sha, preserved, reviewed_tag, reviewed_source_sha):
    """Pure fact extraction; these seven pinned inputs are supplied as byte values."""
    index, base = accepted_root(index_raw, verified_sha, reviewed_tag, reviewed_source_sha)
    require(set(preserved) == set(needed_paths(index_raw, verified_sha, reviewed_tag, reviewed_source_sha)), "Missing or extra renderer fact inputs")
    def pinned(name):
        raw = preserved[name]
        require(valid_sha(index["sha256"].get(name)) and sha(raw) == index["sha256"][name], "Preserved checkpoint fact hash differs")
        return parse(raw)
    source_name = base + "/source-freeze.json"
    source = pinned(source_name)
    require(isinstance(source, dict) and 0 < len(source) <= 20_000
            and all(isinstance(name, str) and valid_sha(pin) for name, pin in source.items())
            and sha(preserved[source_name]) == reviewed_source_sha
            and type(index["sourceFiles"]) is int and len(source) == index["sourceFiles"], "Accepted source-map facts differ")
    summary = pinned(base + "/summary.json")
    require(summary["accepted"] is True and summary["sourceManifestSha256"] == reviewed_source_sha
            and summary["runnerSha256"] == RUNNER_SHA and set(summary["phaseReceipts"]) == set(PHASES), "Missing finalized acceptance")
    phases = {}
    for phase in PHASES:
        name = base + "/" + phase + "/receipt.json"
        receipt = pinned(name)
        counts = {key: receipt[key] for key in COUNTS}
        require(receipt["phase"] == phase and receipt["accepted"] is True
                and receipt["sourceManifestSha256"] == reviewed_source_sha and receipt["runnerSha256"] == RUNNER_SHA
                and receipt["sourceFiles"] == len(source)
                and receipt["sourceChanges"] == receipt["validationErrors"] == []
                and all(type(value) is int and value >= 0 for value in counts.values())
                and counts["failed"] == 0 and counts == index["phaseResults"][phase], "Actual phase facts disagree")
        proof = dict(receiptSha256=index["sha256"][name], **counts)
        require(summary["phaseReceipts"][phase] == proof, "Finalized phase pin/count differs")
        phases[phase] = dict(receipt=name, **proof)
    matrix = {key: index["nativeMatrix"][key] for key in
              ("cargoTargets", "integrationTargets", "libraryRegistered", "libraryIgnored")}
    require(all(type(value) is int and value >= 0 for value in matrix.values()), "Invalid preserved native inventory facts")
    summary_name = base + "/summary.json"
    require(index["fullSummary"] == dict(path=summary_name, sha256=index["sha256"][summary_name]),
            "Finalized full-summary pointer differs")
    return dict(status="accepted_windows_development_checkpoint", platform="Windows", acceptedTag=index["acceptedTag"],
                index=INDEX, indexSha256=verified_sha, receipt=phases["native"]["receipt"],
                sourceManifest=source_name, sourceManifestSha256=reviewed_source_sha, runnerSha256=RUNNER_SHA,
                sourceFiles=len(source), rustFiles=sum(name.endswith(".rs") for name in source),
                phaseProof=phases, nativeMatrix=matrix, cargoTargets=matrix["cargoTargets"],
                **{key: phases["native"][key] for key in COUNTS},
                claims=dict(universalParity=False, releaseCertified=False, newParityRowsCertified=False),
                affectedDetailsClaims=dict(complete=False, compilerBindingEstablished=False,
                                           runtimeDispatchEstablished=False),
                sourceChangesDuringEachGate=[],
                boundedCapabilities=[
                    "Captured TypeScript parent/type hierarchy and exact same-name member relationships produce structural candidate seeds and per-root/per-walk-seed incoming call/reference proofs with deterministic shortest witnesses.",
                    "The canonical JSON library result and git affected CLI retain captured source/analyzer bindings, explicit true-zero graph/walk/row/source/wire budgets and static recognized-command/domain refusal markers.",
                ],
                limitations=[
                    "Structural candidates do not prove override identity, compiler binding, runtime dispatch, semantic/reference completeness or full Graphify behavior/schema equivalence; complete, compilerBindingEstablished and runtimeDispatchEstablished remain false.",
                    "The new source-byte budget excludes inherited report-loader and epoch-proof reads; it is not a total I/O, memory or all-allocation bound.",
                    "Windows-only unreleased checkpoint; Linux/macOS runtime, full upstream contracts, universal parity and release certification remain open.",
                ],
                nonadditiveEvidence=dict(index=INDEX, rawCountsAreNonadditive=True,
                                         interpretation="Only the five accepted whole-phase receipts supply totals. Focused tests, qualification, causal failures and repeated gate roles never add to them."),
                inventoryBoundary=dict(originalIdentities=353, unreconciledReviewRequired=73,
                                       backendRuntimeUnknowns=77, identitiesStatusesAndCertificationsChanged=False),
                postFreezeMetadataFiles=list(BEFORE),
                metadataBoundary="Four separately reviewed metadata files follow evidence preservation; the accepted runtime source freeze and Serena mapping retain their separate identities.")


def paragraph(record, link):
    phases, native = record["phaseProof"], record["phaseProof"]["native"]
    return (f"{CURRENT_MARKER} Tag `{record['acceptedTag']}` passed {native['passed']} native tests/"
            f"0 failures/{native['ignored']} ignored across {record['cargoTargets']} Cargo targets/{native['groups']} groups; "
            f"Python {phases['python']['passed']}/{phases['python']['ignored']} ignored and "
            f"TypeScript {phases['typescript']['passed']}/{phases['typescript']['ignored']} skipped. "
            f"Its {record['sourceFiles']}-file/{record['rustFiles']}-Rust source map is `{record['sourceManifestSha256']}`; "
            f"the [preserved index]({link}) binds all five actual phase receipts. R5B adds captured TypeScript parent/hierarchy "
            "and exact same-name method structural candidates, per-root incoming call/reference proofs and the canonical JSON "
            "`git affected` CLI. Explicit budgets retain true zero and refuse capacity without clipping proofs; recognized "
            "command/domain failures use static refusal markers. These candidates do not establish override identity, compiler "
            "binding or runtime dispatch: `complete`, `compilerBindingEstablished` and `runtimeDispatchEstablished` remain false. "
            "Full Graphify behavior/schema equivalence remains open. Focused evidence is nonadditive. Wave6 and all older records, "
            "353 inventory identities, 73 unreconciled rows, 77 backend runtime unknowns and false full-contract certifications "
            "remain unchanged. This is an unreleased Windows-only checkpoint; universal parity, release and Linux/macOS "
            "certification remain open.")


def render(originals, index_raw, verified_sha, preserved, reviewed_tag, reviewed_source_sha):
    """Pure renderer: input byte values in, exactly four candidate byte values out."""
    require(set(originals) == set(BEFORE), "Expected exactly four original metadata documents")
    for name, expected in BEFORE.items():
        require(sha(originals[name]) == expected, "Metadata preimage changed: " + name)
    record = checkpoint(index_raw, verified_sha, preserved, reviewed_tag, reviewed_source_sha)
    outputs = {}
    for name, raw in originals.items():
        if name.endswith(".json"):
            before = parse(raw)
            require(json_bytes(before) == raw and "seventhWaveAcceptance" not in before, "Original JSON ordering/CRLF changed or seventh wave exists")
            aliases = ("frozenAcceptance",) if "native-reconciliation" in name else ("latestWholeNativeRun", "wholeNativeRun")
            require(all(before[key] == before["sixthWaveAcceptance"] for key in aliases), "Current acceptance aliases no longer describe retained Wave6")
            after = dict(before)
            for key in aliases:
                after[key] = record
            after["seventhWaveAcceptance"] = record
            allowed = {*aliases, "seventhWaveAcceptance"}
            require({k: v for k, v in after.items() if k not in allowed} ==
                    {k: v for k, v in before.items() if k not in allowed}, "Renderer changed a protected metadata field")
            outputs[name] = json_bytes(after)
        else:
            require(b"\r" not in raw and raw.count(PREVIOUS_MARKER) == 1 and CURRENT_MARKER.encode() not in raw, "Original prose LF/marker changed")
            link = "validation/parity-windows-2026-10-16-wave7/index.json" if name == "docs/STATUS.md" else "../../docs/validation/parity-windows-2026-10-16-wave7/index.json"
            addition = (paragraph(record, link) + "\n\n").encode("utf-8")
            outputs[name] = raw.replace(PREVIOUS_MARKER, addition + PREVIOUS_MARKER, 1)
            require(outputs[name].replace(addition, b"", 1) == raw, "Existing prose changed")
    native = parse(originals["docs/parity/native-reconciliation-2026-10-15.json"])
    ledger = native["ledgerSummary"]
    require(ledger["totalEnumeratedEntries"] == 353
            and ledger["assessmentCounts"]["unreconciled_review_required"] == 73
            and ledger["assessmentCounts"]["catalog_declared_runtime_review_required"] == 77,
            "Protected inventory counts differ from reviewed metadata")
    return outputs


def plain(path, boundary, missing=False):
    require(path.is_absolute() and path.is_relative_to(boundary) and path != boundary, "Path escaped its bounded root")
    require(boundary.resolve(strict=True) == boundary, "Noncanonical boundary")
    current = boundary
    for part in path.relative_to(boundary).parts:
        current /= part
        try:
            info = current.lstat()
        except FileNotFoundError:
            require(missing, "Missing metadata input")
            continue
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400
                and current.resolve(strict=True) == current, "Reparse/aliased path refused")
    return path


def bounded_read(path):
    plain(path, ROOT)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= 16 * 1024 * 1024, "Metadata input kind/byte limit")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        raw = stream.read(min(before.st_size + 1, 16 * 1024 * 1024 + 1))
        completed = os.fstat(stream.fileno())
    plain(path, ROOT)
    after = path.lstat()
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns)
    require(len(raw) == before.st_size and identity(before) == identity(opened) == identity(completed) == identity(after),
            "Metadata input changed during read")
    return raw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verified-wave7-index-sha256", required=True,
                        help="Exact actual preserved index digest separately verified by root")
    parser.add_argument("--reviewed-accepted-tag", required=True, help="Exact successful full-run tag separately reviewed by root")
    parser.add_argument("--reviewed-source-manifest-sha256", required=True,
                        help="Exact successful full-run source digest separately reviewed by root")
    parser.add_argument("--output-dir", required=True, help="Fresh direct directory under this ignored staging area")
    args = parser.parse_args()
    require(os.name == "nt" and ROOT.name == "Code-Intelligence", "Expected Windows normal checkout")
    output = Path(args.output_dir)
    require(output.is_absolute() and output.parent == STAGING
            and re.fullmatch(r"candidates-[A-Za-z0-9][A-Za-z0-9_-]{0,64}", output.name)
            and not output.exists() and not output.is_symlink(), "Fresh direct ignored output required")
    plain(STAGING, ROOT)
    index_raw = bounded_read(ROOT / INDEX)
    facts = needed_paths(index_raw, args.verified_wave7_index_sha256,
                         args.reviewed_accepted_tag, args.reviewed_source_manifest_sha256)
    preserved = {name: bounded_read(ROOT / name) for name in facts}
    originals = {name: bounded_read(ROOT / name) for name in BEFORE}
    outputs = render(originals, index_raw, args.verified_wave7_index_sha256, preserved,
                     args.reviewed_accepted_tag, args.reviewed_source_manifest_sha256)
    # Root separately reviews/applies these reversible candidates; there is no normal writer.
    plain(output, STAGING, missing=True)
    output.mkdir(exist_ok=False)
    for name, raw in outputs.items():
        target = output / name
        plain(target, STAGING, missing=True)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(raw)
    print(json.dumps(dict(output=str(output), normalWrites=False, verifiedIndexSha256=args.verified_wave7_index_sha256,
                          afterSha256={name: sha(raw) for name, raw in outputs.items()}), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, UnicodeError, OSError) as error:
        raise SystemExit("Metadata rendering refused: " + str(error))
