#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo_bin=${CARGO:-/usr/local/bin/cargo}

"$cargo_bin" check --locked --all-targets
"$cargo_bin" nextest run --locked --lib
"$cargo_bin" xwin check --locked --target x86_64-pc-windows-msvc
