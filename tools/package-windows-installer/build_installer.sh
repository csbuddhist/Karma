#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
version="${1:-0.1.7}"
bundle_dir="$repo_root/release/windows-x64-test"
output_dir="$repo_root/target/release-artifacts"
output_file="$output_dir/Karma-windows-x64-test-v${version}-setup.exe"
icon_file="$repo_root/apps/karma-ui/src-tauri/icons/icon.ico"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must use MAJOR.MINOR.PATCH format" >&2
  exit 1
fi
if ! command -v makensis >/dev/null 2>&1; then
  echo "makensis is required; install NSIS 3 before packaging" >&2
  exit 1
fi

bash "$script_dir/test_installer_contract.sh"
bash "$repo_root/tools/package-windows-test/test_bundle_contract.sh"
(
  cd "$bundle_dir"
  shasum -a 256 -c SHA256SUMS
)

mkdir -p "$output_dir"
makensis \
  -DVERSION="$version" \
  -DFILE_VERSION="${version}.0" \
  -DBUNDLE_DIR="$bundle_dir" \
  -DOUTPUT_FILE="$output_file" \
  -DICON_FILE="$icon_file" \
  "$script_dir/KarmaInstaller.nsi"

file "$output_file"
shasum -a 256 "$output_file"
if command -v 7zz >/dev/null 2>&1; then
  7zz t "$output_file"
fi
