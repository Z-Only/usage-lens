#!/usr/bin/env python3
"""Create an allowlisted, checksummed native release archive (no install scripts)."""
from __future__ import annotations

import argparse
import gzip
import io
import json
import platform
import subprocess
import tarfile
import tempfile
from pathlib import Path

from common import FILES, GENERATED_FILES, TARGETS, asset_base, digest, json_bytes, validate_identity, verify_archive

# Deliberately enumerate project inputs. Never copy the checkout, HOME, databases,
# logs, fixtures, credentials, build caches, source maps, or node_modules.



def validate_host(target: str) -> None:
    expected = TARGETS[target]
    if platform.system().lower() != expected["os"] or platform.machine().lower() != expected["arch"]:
        raise ValueError("Release target must match the native runner OS and architecture")


def package(root: Path, binary: Path, out: Path, version: str, commit: str, target: str) -> Path:
    validate_identity(version, commit)
    base = asset_base(version, target)
    validate_host(target)
    if binary.is_symlink() or not binary.is_file():
        raise ValueError("Expected a compiled regular native executable")
    binary_name = "usage-lens.exe" if target == "windows-x64" else "usage-lens"
    files = {binary_name: binary}
    for name in FILES:
        file = root / name
        if file.is_symlink() or not file.is_file() or any(p.is_symlink() for p in file.parents if p == root or root in p.parents):
            raise ValueError(f"Missing or linked allowlisted input: {name}")
        files[name] = file
    notices = root / "target/release-notices" / target
    for name in GENERATED_FILES:
        file = notices / name
        if file.is_symlink() or not file.is_file() or not file.resolve().is_relative_to(notices.resolve()):
            raise ValueError(f"Missing or linked generated dependency notices: {name}")
        files[name] = file
    manifest = {
        "schemaVersion": 1, "name": "usage-lens", "version": version,
        "commit": commit, "platform": target, **TARGETS[target],
        "runtimePrerequisite": "none", "ui": "embedded", "sqlite": "bundled",
        "files": {name: digest(file) for name, file in sorted(files.items())},
    }
    out.mkdir(parents=True, exist_ok=True)
    archive = out / f"{base}.tar.gz"
    if archive.exists() or (out / f"{base}.manifest.json").exists():
        raise ValueError("Refusing to overwrite an existing release artifact")
    # Stable metadata, no local usernames, no absolute paths, no executable docs.
    with archive.open("xb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as gz, tarfile.open(mode="w", fileobj=gz, format=tarfile.PAX_FORMAT) as tar:
        for name in sorted([*files, "manifest.json"]):
            data = json_bytes(manifest) if name == "manifest.json" else files[name].read_bytes()
            info = tarfile.TarInfo(f"{base}/{name}")
            info.size = len(data)
            info.mode = 0o755 if name == binary_name else 0o644
            info.mtime = info.uid = info.gid = 0
            tar.addfile(info, io.BytesIO(data))
    verify_archive(archive, manifest)
    (out / f"{base}.manifest.json").write_bytes(json_bytes(manifest))
    return archive


def smoke_archive(archive: Path, root: Path) -> None:
    manifest = json.loads(archive.with_name(archive.name.removesuffix(".tar.gz") + ".manifest.json").read_text())
    verify_archive(archive, manifest)
    with tempfile.TemporaryDirectory(prefix="usage-lens-release-smoke-") as directory:
        # All members have already been verified as regular, safe, allowlisted files.
        with tarfile.open(archive, "r:gz") as tar:
            tar.extractall(directory, filter="data")
        bundle = Path(directory) / asset_base(manifest["version"], manifest["platform"])
        binary = bundle / ("usage-lens.exe" if manifest["platform"] == "windows-x64" else "usage-lens")
        subprocess.run(["node", str(root / "scripts/release/smoke.mjs"), str(binary), manifest["version"]], check=True, cwd=directory, timeout=60)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    archive = package(args.root.resolve(), args.binary.absolute(), args.out.resolve(), args.version, args.commit, args.target)
    smoke_archive(archive, args.root.resolve())
    print(f"Verified extracted release: {archive.name}")


if __name__ == "__main__":
    main()
