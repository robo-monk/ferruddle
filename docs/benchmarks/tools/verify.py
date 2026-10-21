"""Verify preserved benchmark evidence and its relationship to the release source.

Run with Python 3.11+ from any directory. No dependencies, builds or network.
"""

import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tomllib

HERE = Path(__file__).resolve().parent
BENCHMARKS = HERE.parent
ROOT = BENCHMARKS.parents[1]


def load(path):
    return json.loads(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_archive(path):
    members = {}
    with tarfile.open(path, "r:gz") as archive:
        for member in archive.getmembers():
            name = PurePosixPath(member.name)
            assert member.isfile() and not name.is_absolute() and ".." not in name.parts
            assert member.name not in members
            members[member.name] = archive.extractfile(member).read()
    return members


evidence = load(BENCHMARKS / "evidence.json")
for name, expected in evidence["files"].items():
    assert sha((BENCHMARKS / name).read_bytes()) == expected, name

measured = load(BENCHMARKS / "measured-source.json")
archive_path = BENCHMARKS / measured["archive"]
assert sha(archive_path.read_bytes()) == measured["archiveSha256"]
source = read_archive(archive_path)
assert set(source) == set(measured["files"])
for name, expected in measured["files"].items():
    assert sha(source[name]) == expected, name

baseline = load(BENCHMARKS / "baseline/source-manifest.json")
baseline_path = BENCHMARKS / "baseline/source.tar.gz"
assert sha(baseline_path.read_bytes()) == baseline["archiveSha256"]
baseline_source = read_archive(baseline_path)
embedded = json.loads(baseline_source.pop("original-native/BENCHMARK-SOURCE.json"))
assert embedded["archiveFileHashes"] == baseline["archiveFileHashes"]
assert embedded["runtimeSourceHashes"] == baseline["runtimeSourceHashes"]
assert {name.removeprefix("original-native/"): sha(data) for name, data in baseline_source.items()} == baseline["archiveFileHashes"]
assert load(BENCHMARKS / "baseline/contract.json")["runtimeSourceHashes"] == baseline["runtimeSourceHashes"]

summaries = []
for directory in ["run-1", "run-2"]:
    results = BENCHMARKS / "results" / directory
    summary = load(results / "summary.json")
    native = load(results / "native-repeat-summary.json")
    host = load(results / "quiet-window.json")
    assert summary["caseCount"] == native["caseCount"] == 208
    assert summary["inputBytes"] == 1372181
    assert summary["passes"] == summary["warmups"] == 2 and summary["rounds"] == 7
    assert summary["measuredRoundsPerMode"] == 14
    for group in ["rustSourceHashes", "referenceAndHarnessHashes", "inputHashes"]:
        for name, expected in summary[group].items():
            assert sha(source[name]) == expected, (directory, group, name)
    assert host["runtimeAndBinaryHashesUnchanged"] and host["allCommandsExitedZero"]
    prebuild = host["prebuiltArtifacts"]
    assert prebuild["currentExecutableSha256"] == summary["executableSha256"] == native["binaryHashes"]["current"]
    assert prebuild["baselineExecutableSha256"] == native["binaryHashes"]["baseline"]
    assert native["baselineSourceHashes"] == baseline["runtimeSourceHashes"]
    assert sha((results / "vmstat.log").read_bytes()) == host["vmstatLogSha256"]
    assert sha((results / "run-quiet-bench.py").read_bytes()) == host["runnerSha256"]
    for script in ["verify-paired.py", "verify-native.py"]:
        subprocess.run([sys.executable, str(HERE / script), str(results)], check=True)
    summaries.append(summary)

for group in ["rustSourceHashes", "referenceAndHarnessHashes", "inputHashes"]:
    assert summaries[0][group] == summaries[1][group], group
for implementation in ["rust", "typescript"]:
    first = load(BENCHMARKS / f"results/run-1/{implementation}.json")
    second = load(BENCHMARKS / f"results/run-2/{implementation}.json")
    for left, right in zip(first["modes"], second["modes"]):
        assert left["mode"] == right["mode"]
        assert [(c["id"], c["output"]) for c in left["perCase"]] == [(c["id"], c["output"]) for c in right["perCase"]]

# Normalize only documentation, crate identity and CLI branding after migration.
def normalized_rust(data):
    text = data.decode().replace("defuddle::", "ferruddle::")
    text = text.replace("extern crate self as defuddle;", "extern crate self as ferruddle;")
    text = text.replace("defuddle: {error}", "ferruddle: {error}")
    text = text.replace("Usage: defuddle", "Usage: ferruddle")
    return [line for line in text.splitlines() if line.strip() and not line.lstrip().startswith(("///", "//!"))]


def current_path(name):
    return name.removeprefix("rust/").replace("src/bin/defuddle.rs", "src/bin/ferruddle.rs")


runtime_paths = {name for name in source if name.startswith("rust/src/") and name.endswith(".rs")}
assert {current_path(name) for name in runtime_paths} == {str(p.relative_to(ROOT)) for p in (ROOT / "src").rglob("*.rs")}
for name in runtime_paths:
    assert normalized_rust(source[name]) == normalized_rust((ROOT / current_path(name)).read_bytes()), name
assert normalized_rust(source["rust/examples/benchmark.rs"]) == normalized_rust((ROOT / "examples/benchmark.rs").read_bytes())
assert source["scripts/porting/benchmark.shared.mjs"] == (ROOT / "tools/benchmark/benchmark.shared.mjs").read_bytes()
for name in source:
    if name.startswith("rust/tests/data/"):
        assert source[name] == (ROOT / current_path(name)).read_bytes(), name
old_lock = tomllib.loads(source["rust/Cargo.lock"].decode())
new_lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
for package in new_lock["package"]:
    if package["name"] == "ferruddle" and "source" not in package:
        package["name"] = "defuddle"
for lock in [old_lock, new_lock]:
    lock["package"].sort(key=lambda p: (p["name"], p["version"]))
assert old_lock == new_lock, "locked dependency graph changed"
old = tomllib.loads(source["rust/Cargo.toml"].decode())
current = tomllib.loads((ROOT / "Cargo.toml").read_text())
for key in ["dependencies", "dev-dependencies", "profile"]:
    assert old[key] == current[key], key
assert old["package"]["name"] == "defuddle" and current["package"]["name"] == "ferruddle"
for key in ["version", "edition", "rust-version", "license"]:
    assert old["package"][key] == current["package"][key], key

public = load(BENCHMARKS / "summary.json")
assert public["caseCount"] == 208 and public["excludedMathCases"] == 18
for mode in public["modes"]:
    measured_modes = [next(m for m in s["modes"] if m["mode"] == mode["mode"]) for s in summaries]
    assert mode["speedupRange"] == [min(m["speedup"] for m in measured_modes), max(m["speedup"] for m in measured_modes)]
    assert mode["runs"] == [{"typescriptCorpusMedianMs": m["typescript"]["corpusMedianMs"], "rustCorpusMedianMs": m["rust"]["corpusMedianMs"], "speedup": m["speedup"]} for m in measured_modes]

print("Verified: immutable raw evidence, both source archives, speed ratios, regression flags and stable outputs.")
print("Ferruddle relationship: crate/CLI identity, layout and documentation changed; extraction code, locked dependencies and measured inputs did not.")
