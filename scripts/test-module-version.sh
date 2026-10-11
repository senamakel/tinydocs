#!/usr/bin/env bash
# Pin the release-version contract without compiling or packaging a module.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
shell_pattern="$(sed -n "s/^semver_pattern='\(.*\)'$/\1/p" scripts/build-module.sh)"
if [[ -z "$shell_pattern" ]]; then
  shell_pattern="$(sed -n 's/.*"$version" =~ \(.*\) && -n.*/\1/p' scripts/build-module.sh)"
fi
powershell_pattern="$(sed -n "s/.*-notmatch '\(.*\)'.*/\1/p" scripts/build-module.ps1 | head -1)"
# PowerShell uses an absolute end anchor because .NET's $ accepts a final newline.
[[ "${powershell_pattern%\\z}" == "${shell_pattern%\$}" ]] || {
  [[ "$powershell_pattern" == "$shell_pattern" ]] || { echo 'version validators differ' >&2; exit 1; }
}
valid=('0.0.0' '1.2.3' '1.2.3-rc.1+build.7' '1.2.3-alpha-beta.0' '1.2.3-0' '1.2.3+001.build' '1.2.3--')
invalid=('1.2.3-..' '01.2.3' '1.02.3' '1.2.03' '1.2.3-01' '1.2.3-alpha..1' '1.2.3+' '1.2.3+build..7' '1.2.3-rc_1' 'v1.2.3' '1.2.3.4' $'1.2.3\n')
for version in "${valid[@]}"; do
  [[ "$version" =~ $shell_pattern ]] || { echo "rejected valid version: $version" >&2; exit 1; }
done
for version in "${invalid[@]}"; do
  if [[ "$version" =~ $shell_pattern ]]; then
    echo "accepted invalid version: $version" >&2
    exit 1
  fi
done
if command -v pwsh >/dev/null 2>&1; then
  for version in "${valid[@]}"; do
    pwsh -NoProfile -Command 'if ($args[0] -notmatch $args[1]) { exit 1 }' "$version" "$powershell_pattern"
  done
  for version in "${invalid[@]}"; do
    pwsh -NoProfile -Command 'if ($args[0] -match $args[1]) { exit 1 }' "$version" "$powershell_pattern"
  done
fi
echo 'module version validation passed'
