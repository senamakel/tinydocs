#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
bash scripts/test-module-version.sh
cargo test --locked
cargo test --locked --all-features
case "${RUNNER_OS:-$(uname -s)}" in
  Windows|MINGW*|MSYS*|CYGWIN*) ;;
  *) python3 scripts/test-package-layout.py ;;
esac
