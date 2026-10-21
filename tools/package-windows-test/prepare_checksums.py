"""Generate the release checksums before signing / 签名前生成发布哈希表。"""

import hashlib
import sys
from pathlib import Path

bundle = Path(sys.argv[1])
excluded = {"SHA256SUMS", "release-manifest.json", "release-manifest.json.sig", ".DS_Store"}
entries = []
for path in sorted(bundle.rglob("*")):
    if path.is_symlink():
        raise SystemExit(f"Bundle must not contain links / 测试包不能包含链接: {path}")
    if path.is_file() and path.name not in excluded:
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        entries.append(f"{digest}  {path.relative_to(bundle).as_posix()}\n")
(bundle / "SHA256SUMS").write_text("".join(entries), encoding="utf-8")
