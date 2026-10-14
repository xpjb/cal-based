#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo_bin=${CARGO:-/usr/local/bin/cargo}
target=x86_64-pc-windows-msvc
version=$(python3 - <<'PY'
import tomllib
from pathlib import Path
print(tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"])
PY
)

"$cargo_bin" xwin build --locked --release --target "$target"

mkdir -p dist
source_exe="target/$target/release/cal-based.exe"
output="dist/cal-based-$version-windows-x64.exe"
cp "$source_exe" "$output"
(
    cd dist
    sha256sum "$(basename "$output")" > "$(basename "$output").sha256"
)

printf '%s\n' "$output"
cat "$output.sha256"
