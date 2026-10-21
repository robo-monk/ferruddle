"""Read-only verification of the final source and recorded measurement inputs."""

import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[3]


def read_json(path):
    return json.loads((ROOT / path).read_text())


def sha(path):
    return hashlib.sha256((ROOT / path).read_bytes()).hexdigest()


run = read_json("rust/reviews/hardening-run.json")
frozen = run["frozenSourceHashes"]
assert len(frozen) == 28
assert set(frozen) == {str(p.relative_to(ROOT)) for p in (ROOT / "rust/src").rglob("*.rs")}
for path, expected in frozen.items():
    assert sha(path) == expected, path

directories = ["2026-10-21-hardening", "2026-10-21-hardening-repeat"]
summaries = [read_json(f"rust/benchmarks/{d}/summary.json") for d in directories]
tracked = set(subprocess.check_output(["git", "ls-files"], cwd=ROOT, text=True).splitlines())
reference_tracked = set(summaries[0]["referenceAndHarnessHashes"]) & tracked
assert len(reference_tracked) == 69
assert len(reference_tracked - {"tsconfig.json", "vitest.config.ts"}) == 67
assert not subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=ROOT, text=True).strip()
for field in ["rustSourceHashes", "referenceAndHarnessHashes", "inputHashes"]:
    assert summaries[0][field] == summaries[1][field], field
    for path, expected in summaries[0][field].items():
        assert sha(path) == expected, path

for directory, summary in zip(directories, summaries):
    prefix = f"rust/benchmarks/{directory}"
    quiet = read_json(f"{prefix}/quiet-window.json")
    native = read_json(f"{prefix}/native-repeat-summary.json")
    assert summary["caseCount"] == native["caseCount"] == 208
    assert summary["inputBytes"] == 1372181
    assert summary["passes"] == summary["warmups"] == 2
    assert summary["rounds"] == 7
    assert quiet["runtimeAndBinaryHashesUnchanged"] and quiet["allCommandsExitedZero"]
    prebuild = quiet["prebuiltArtifacts"]
    assert prebuild["runtimeSourceHashes"] == frozen
    assert prebuild["currentExecutableSha256"] == summary["executableSha256"]
    assert native["binaryHashes"]["current"] == summary["executableSha256"]
    assert native["binaryHashes"]["baseline"] == prebuild["baselineExecutableSha256"]
    assert sha(summary["nativeExecutable"]) == summary["executableSha256"]
    assert sha(native["baselineExecutable"]) == prebuild["baselineExecutableSha256"]
    assert sha(f"{prefix}/vmstat.log") == quiet["vmstatLogSha256"]
    assert sha(f"{prefix}/run-quiet-bench.py") == quiet["runnerSha256"]
    assert (ROOT / prefix / "independent-recomputation.log").read_text().count("ALL_MATCH") == 2

archive_prefix = "rust/benchmarks/2026-10-21-hardening"
manifest = read_json(f"{archive_prefix}/original-native-source-manifest.json")
archive_path = ROOT / archive_prefix / "original-native-benchmark-source.tar.gz"
assert sha(archive_path) == manifest["archiveSha256"]
members = {}
with tarfile.open(archive_path, "r:gz") as archive:
    for member in archive.getmembers():
        path = PurePosixPath(member.name)
        assert not path.is_absolute() and ".." not in path.parts
        assert member.isfile() and path.parts[0] == "original-native"
        relative = str(path.relative_to("original-native"))
        assert relative not in members
        members[relative] = hashlib.sha256(archive.extractfile(member).read()).hexdigest()
    embedded_manifest = json.loads(archive.extractfile("original-native/BENCHMARK-SOURCE.json").read())
assert embedded_manifest["archiveFileHashes"] == manifest["archiveFileHashes"]
assert embedded_manifest["runtimeSourceHashes"] == manifest["runtimeSourceHashes"]
assert members.pop("BENCHMARK-SOURCE.json")
assert members == manifest["archiveFileHashes"]
assert len(members) == 259

validation = read_json("rust/reviews/validation/manifest.json")
for path, expected in validation["files"].items():
    assert sha(path) == expected, path

for file in ["rust/README.md", "rust/QUALITY.md", "rust/BENCHMARKS.md", "rust/reviews/hardening-final-checklist.md"]:
    for target in re.findall(r"\]\(([^)]+)\)", (ROOT / file).read_text()):
        if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("#"):
            continue
        path = unquote(target.split("#", 1)[0])
        assert (ROOT / file).parent.joinpath(path).exists(), (file, target)

print("ALL_MATCH: 28 runtime files; both source/harness/input maps; 67 reference files and two configs unchanged;")
print("both native binary pairs; monitors; 259 archived baseline files; validation logs; local documentation links.")
