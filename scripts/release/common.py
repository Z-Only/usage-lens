"""Release metadata and archive checks. Standard library only; never reads user data."""
from __future__ import annotations

import hashlib
import json
import re
import tarfile
from pathlib import Path, PurePosixPath

FILES = [
    "LICENSE", "README.md", "CHANGELOG.md",
    "docs/AI_INSTALL.md", "docs/releases.md", "docs/privacy.md",
    "docs/data-contract.md", "docs/record-import.md", "docs/verification.md",
    "docs/third-party.md",
    "plugin/usage-lens/.codex-plugin/plugin.json",
    "plugin/usage-lens/README.md",
    "plugin/usage-lens/examples/mcp.config.example.json",
    "plugin/usage-lens/examples/hooks.config.example.json",
    "plugin/usage-lens/skills/usage-summary/SKILL.md",
]

GENERATED_FILES = ("THIRD_PARTY_LICENSES.txt", "THIRD_PARTY_COMPONENTS.json", "RUST_STANDARD_LIBRARY_NOTICES.html")

TARGETS = {
    "linux-x64": {"rustTarget": "x86_64-unknown-linux-musl", "runner": "ubuntu-24.04", "os": "linux", "arch": "x86_64", "minimumOS": "Linux x64 (static musl; tested on Ubuntu 24.04, older kernels unverified)"},
    "macos-arm64": {"rustTarget": "aarch64-apple-darwin", "runner": "macos-15", "os": "darwin", "arch": "arm64", "minimumOS": "macOS 11.0 deployment target; runtime tested on macOS 15 only"},
    "macos-x64": {"rustTarget": "x86_64-apple-darwin", "runner": "macos-15-intel", "os": "darwin", "arch": "x86_64", "minimumOS": "macOS 11.0 deployment target; runtime tested on macOS 15 only"},
    "windows-x64": {"rustTarget": "x86_64-pc-windows-msvc", "runner": "windows-2025", "os": "windows", "arch": "amd64", "minimumOS": "64-bit Windows 10/11 or Windows Server 2025; tested on Server 2025"},
}
VERSION = re.compile(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")


def version_tuple(version: str) -> tuple[int, int, int]:
    if not VERSION.fullmatch(version):
        raise ValueError("Expected a stable SemVer version MAJOR.MINOR.PATCH, without leading zeros")
    return tuple(int(x) for x in version.split("."))


def validate_identity(version: str, commit: str) -> None:
    version_tuple(version)
    if not COMMIT.fullmatch(commit):
        raise ValueError("Expected a full lowercase Git commit SHA")


def asset_base(version: str, target: str) -> str:
    version_tuple(version)
    if target not in TARGETS:
        raise ValueError("Unsupported release platform")
    return f"usage-lens-{version}-{target}"


def digest(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def safe_relative(name: str) -> bool:
    path = PurePosixPath(name)
    return bool(name) and not path.is_absolute() and all(part not in ("", ".", "..") for part in name.split("/")) and "\\" not in name and ":" not in name and not any(ord(c) < 32 for c in name)


def verify_archive(archive: Path, manifest: dict) -> None:
    """Verify every byte and member before extraction; reject links/special files."""
    validate_identity(manifest["version"], manifest["commit"])
    target = manifest["platform"]
    base = asset_base(manifest["version"], target)
    if manifest.get("schemaVersion") != 1 or manifest.get("rustTarget") != TARGETS[target]["rustTarget"]:
        raise ValueError("Unsupported manifest schema or Rust target")
    if manifest.get("runtimePrerequisite") != "none":
        raise ValueError("Unexpected runtime prerequisite")
    expected = manifest["files"]
    binary = "usage-lens.exe" if target == "windows-x64" else "usage-lens"
    if set(expected) != set(FILES) | set(GENERATED_FILES) | {binary} or not all(safe_relative(name) for name in expected):
        raise ValueError("Invalid file manifest")
    seen = set()
    with tarfile.open(archive, "r:gz") as tar:
        for member in tar:
            if not member.isfile() or not member.name.startswith(base + "/"):
                raise ValueError("Archive contains an invalid member")
            name = member.name[len(base) + 1:]
            if not safe_relative(name) or name in seen:
                raise ValueError("Archive contains an unsafe or duplicate path")
            seen.add(name)
            handle = tar.extractfile(member)
            if name == "manifest.json":
                if handle.read() != json_bytes(manifest):
                    raise ValueError("Internal and external manifests differ")
            elif name not in expected or hashlib.file_digest(handle, "sha256").hexdigest() != expected[name]:
                raise ValueError("Archive file checksum mismatch")
    if seen != set(expected) | {"manifest.json"}:
        raise ValueError("Archive file inventory mismatch")


def changelog_section(text: str, version: str) -> str:
    version_tuple(version)
    match = re.search(r"^## \[" + re.escape(version) + r"\](?:[^\n]*)\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    if not match or not match.group(1).strip():
        raise ValueError("Version needs a nonempty reviewed CHANGELOG entry")
    return f"## {version}\n\n{match.group(1).strip()}\n"
