#!/usr/bin/env bash
# Downloads the official Microsoft Visual C++ x64 redistributable into
# target/vc-redist (cached, never committed) and prints its path after
# verifying its Authenticode signature against pinned Microsoft root CAs.
# Pass --refresh to download the latest redistributable again.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cache_dir="$repo_root/target/vc-redist"
redist="$cache_dir/vc_redist.x64.exe"
redist_url="https://aka.ms/vs/17/release/vc_redist.x64.exe"
# Karma is built with the 14.44 MSVC toolset; older runtimes are not enough.
minimum_version="14.44"
# SHA-256 fingerprints of Microsoft Root Certificate Authority 2011 (code
# signing) and 2010 (timestamping), as published by Microsoft.
roots=(
  "MicRooCerAut2011_2011_03_22.crt 847DF6A78497943F27FC72EB93F9A637320A02B561D0A91B09E87A7807ED7C61"
  "MicRooCerAut_2010-06-23.crt DF545BF919A2439C36983B54CDFC903DFA4F37D3996D8D84B4C31EEC6F3C163E"
)

for tool in curl openssl osslsigncode python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required to fetch and verify vc_redist.x64.exe (brew install osslsigncode)" >&2
    exit 1
  fi
done

mkdir -p "$cache_dir"
if [[ ! -f "$redist" || "${1:-}" == "--refresh" ]]; then
  curl -fsSL -o "$redist.download" "$redist_url"
  mv "$redist.download" "$redist"
fi

trust="$cache_dir/microsoft-roots.pem"
: > "$trust.new"
for entry in "${roots[@]}"; do
  name="${entry%% *}"
  expected="${entry##* }"
  certificate="$cache_dir/$name"
  if [[ ! -f "$certificate" ]]; then
    curl -fsSL -o "$certificate" "https://www.microsoft.com/pki/certs/$name"
  fi
  actual="$(openssl x509 -inform DER -in "$certificate" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':')"
  if [[ "$actual" != "$expected" ]]; then
    echo "unexpected Microsoft root certificate fingerprint for $name" >&2
    exit 1
  fi
  openssl x509 -inform DER -in "$certificate" >> "$trust.new"
done
mv "$trust.new" "$trust"

verification="$(osslsigncode verify -CAfile "$trust" -TSA-CAfile "$trust" -in "$redist" 2>&1 || true)"
if ! grep -q '^Succeeded' <<<"$verification" \
  || ! grep -q 'Subject: CN=Microsoft Corporation,O=Microsoft Corporation' <<<"$verification"; then
  echo "vc_redist.x64.exe failed Microsoft Authenticode verification" >&2
  exit 1
fi

version="$(python3 -I - "$redist" <<'PY'
import sys
data = open(sys.argv[1], "rb").read()
key = "ProductVersion".encode("utf-16-le") + b"\0\0"
start = data.find(key)
if start < 0:
    sys.exit("ProductVersion not found")
start += len(key)
while data[start:start + 2] == b"\0\0":
    start += 2
end = data.find(b"\0\0", start)
while (end - start) % 2:
    end = data.find(b"\0\0", end + 1)
print(data[start:end].decode("utf-16-le"))
PY
)"
if [[ "$(printf '%s\n%s\n' "$minimum_version" "$version" | sort -V | head -1)" != "$minimum_version" ]]; then
  echo "vc_redist.x64.exe version $version is older than $minimum_version" >&2
  exit 1
fi

echo "vc_redist.x64.exe $version verified: $(shasum -a 256 "$redist" | cut -d' ' -f1)" >&2
echo "$redist"
