#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
if [[ "${1:-}" == --archive ]]; then shift; fi
archive="${1:?usage: verify-module.sh --archive <module-archive>}"
archive="$(cd "$(dirname "$archive")" && pwd -P)/$(basename "$archive")"
cd "$root"
work="$(mktemp -d "${HOME:?HOME must be set to verify a module archive}/tinydocs-module-verify.XXXXXX")"
trap 'rm -rf "$work"' EXIT
case "$archive" in
  *.tar.gz)
    members="$(tar -tzf "$archive")"
    while IFS= read -r member; do
      case "$member" in
        /*|../*|*/../*|*/..) echo "archive member escapes the extraction directory: $member" >&2; exit 2 ;;
      esac
    done <<<"$members"
    if tar -tvzf "$archive" | awk 'substr($0, 1, 1) !~ /^[-d]$/ { exit 1 }'; then
      tar -xzf "$archive" -C "$work"
    else
      echo "archive contains a link or special file" >&2
      exit 2
    fi
    ;;
  *) echo "unsupported module archive: $archive" >&2; exit 2 ;;
esac
case "$(uname -s)" in
  Darwin) library="$work/libtinydocs_module.dylib" ;;
  Linux) library="$work/libtinydocs_module.so" ;;
  *) echo "verify-module.sh requires a Unix runner" >&2; exit 1 ;;
esac
test -f "$work/modules.toml"
TINYDOCS_TEST_MODULE="$library" cargo test --locked --release \
  --package tinydocs-module --test module_e2e -- --ignored
