#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <name> <git-url> <ref> [output-json]" >&2
  exit 2
fi

name="$1"
url="$2"
ref="$3"
output="${4:-benchmarks/baselines/${name}.json}"
variant="${ACI_BENCH_VARIANT:-tree-sitter-fallback}"
tmp="$(mktemp -d)"
repo="$tmp/repo"

cargo build --release -p aci-cli --bin aci >/dev/null

if git clone --depth 1 --single-branch --branch "$ref" "$url" "$repo" >/dev/null 2>&1; then
  :
else
  git clone --depth 1 "$url" "$repo" >/dev/null 2>&1
  git -C "$repo" fetch --depth 1 origin "$ref" >/dev/null 2>&1 || true
  git -C "$repo" checkout --detach "$ref" >/dev/null 2>&1 || true
fi
commit="$(git -C "$repo" rev-parse HEAD)"

bench_output="$(target/release/aci bench cold "$repo" --variant "$variant")"
mkdir -p "$(dirname "$output")"

python3 - "$name" "$url" "$ref" "$commit" "$repo" "$variant" "$output" "$bench_output" <<'PY'
import json
import sys
from collections import Counter
from pathlib import Path

name, url, ref, commit, repo, variant, output, bench_output = sys.argv[1:]
root = Path(repo)
vendor_dirs = {
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".venv",
    "__pycache__",
}

def is_vendor_or_generated(path: Path) -> bool:
    name = path.name
    return (
        any(part in vendor_dirs for part in path.parts)
        or name.endswith(".min.js")
        or name.endswith(".generated.ts")
        or name.endswith(".generated.py")
        or name.endswith(".pb.go")
    )

def is_binary(data: bytes) -> bool:
    return b"\0" in data[:8192]

def detected_language(path: Path, data: bytes) -> str:
    extension = path.suffix.lower().lstrip(".")
    filename = path.name
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        text = ""
    first = text.splitlines()[0] if text.splitlines() else ""
    if (
        extension in {"py", "pyw"}
        or filename in {"SConstruct", "SConscript"}
        or (first.startswith("#!") and "python" in first)
    ):
        return "python"
    if extension in {"ts", "tsx", "mts", "cts"}:
        return "typescript"
    if (
        extension in {"js", "jsx", "mjs", "cjs"}
        or filename in {"package.json", "vite.config.js"}
        or (first.startswith("#!") and "node" in first)
    ):
        return "javascript"
    return "unknown"

total_files = 0
extensions = Counter()
detected = Counter()
unknown_extensions = Counter()
skipped = Counter()

for path in root.rglob("*"):
    if not path.is_file():
        continue
    total_files += 1
    extension = path.suffix.lower() or "<none>"
    extensions[extension] += 1
    if is_vendor_or_generated(path):
        skipped["vendor_or_generated"] += 1
        continue
    data = path.read_bytes()
    if is_binary(data):
        skipped["binary"] += 1
        continue
    language = detected_language(path, data)
    detected[language] += 1
    if language == "unknown":
        unknown_extensions[extension] += 1

metrics = {}
for line in bench_output.splitlines():
    if "=" not in line:
        continue
    key, value = line.split("=", 1)
    if value.replace(".", "", 1).isdigit():
        metrics[key] = float(value) if "." in value else int(value)
    else:
        metrics[key] = value

baseline = {
    "name": name,
    "url": url,
    "ref": ref,
    "commit": commit,
    "variant": variant,
    "metrics": metrics,
    "coverage": {
        "total_files": total_files,
        "extensions": dict(extensions.most_common(30)),
        "detected_languages": dict(detected.most_common()),
        "skipped": dict(skipped.most_common()),
        "unknown_extensions": dict(unknown_extensions.most_common(30)),
    },
}

Path(output).write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
print(json.dumps(baseline, indent=2, sort_keys=True))
PY
