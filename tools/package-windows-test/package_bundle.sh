#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"
version="$(python3 -c 'import json; print(json.load(open("apps/karma-ui/src-tauri/tauri.conf.json"))["version"])')"
signing_key="${KARMA_RELEASE_SIGNING_KEY:-$HOME/.config/karma/release-signing.key}"
bundle_dir="$repo_root/release/windows-x64-test"

# Build all executable components together so the signed version is coherent.
# 所有程序一起构建，避免签名包内出现混合版本。
(cd apps/karma-ui && npm run build)
PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH" \
  cargo xwin build --xwin-version 17 -p karma-agent-windows \
  -p karma-service-windows -p karma-ui --target x86_64-pc-windows-msvc --release
for binary in karma-agent-windows karma-ui KarmaService KarmaControl KarmaGuardian; do
  cp "target/x86_64-pc-windows-msvc/release/$binary.exe" "$bundle_dir/"
done
cp -L target/x86_64-pc-windows-msvc/release/DirectML.dll "$bundle_dir/"
cp "$script_dir"/scripts/*.ps1 "$bundle_dir/"
cp "$script_dir/README.bundle.md" "$bundle_dir/README.md"
cp docs/windows-network-enforcement-acceptance.md "$bundle_dir/Windows-Network-Acceptance.md"
cp docs/windows-update-acceptance.md "$bundle_dir/Windows-Update-Acceptance.md"
python3 "$script_dir/prepare_checksums.py" "$bundle_dir"
# SHA256SUMS excludes itself and the signature files; the signed manifest
# includes SHA256SUMS, all scripts, binaries, models, and documentation.
# 哈希表不包含自身和签名文件；签名清单包含哈希表及全部分发文件。
cargo run -p karma-update --example sign-release -- sign "$signing_key" "$bundle_dir" "$version"
bash "$script_dir/test_bundle_contract.sh"
mkdir -p target/release-artifacts
python3 - "$bundle_dir" "target/release-artifacts/Karma-windows-x64-test-v$version.zip" <<'PY'
import sys
import zipfile
from pathlib import Path

bundle = Path(sys.argv[1])
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(bundle.rglob("*")):
        if path.is_file() and path.name != ".DS_Store":
            archive.write(path, Path("windows-x64-test") / path.relative_to(bundle))
PY
