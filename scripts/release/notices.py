#!/usr/bin/env python3
"""Collect locked native/WASM dependency licenses and toolchain notices, offline."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from pathlib import Path

from common import TARGETS, digest, json_bytes

PREFIXES = ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")
# New license families require a reviewed policy update, not silent inclusion.
ALLOWED = {"MIT", "Apache-2.0", "Unicode-3.0", "BSL-1.0", "Zlib", "Unlicense", "CC0-1.0", "BSD-3-Clause", "LLVM-exception"}


def reachable(metadata, package_name):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = [p["id"] for p in packages.values() if p["name"] == package_name and p["source"] is None]
    if len(roots) != 1:
        raise ValueError("Expected one local package root for license graph")
    stack, seen, result = [(identifier, "normal-dependency") for identifier in roots], set(), {}
    while stack:
        identifier, role = stack.pop()
        if (identifier, role) in seen:
            continue
        seen.add((identifier, role))
        package = packages[identifier]
        if package["source"] is not None:
            if identifier not in result:
                result[identifier] = {**package, "usageKinds": []}
            if role not in result[identifier]["usageKinds"]:
                result[identifier]["usageKinds"].append(role)
        for dependency in nodes[identifier]["deps"]:
            child = packages[dependency["pkg"]]
            macro = any("proc-macro" in target["kind"] for target in child["targets"])
            for kind in dependency["dep_kinds"]:
                if kind["kind"] == "dev":
                    continue
                child_role = "build/proc-macro" if role == "build/proc-macro" or kind["kind"] == "build" or macro else "normal-dependency"
                stack.append((dependency["pkg"], child_role))

    return result


def check_expression(expression):
    if not expression:
        raise ValueError("Dependency has no declared license")
    tokens = set(re.findall(r"[A-Za-z0-9][A-Za-z0-9.+-]*", expression)) - {"AND", "OR", "WITH"}
    if not tokens or tokens - ALLOWED:
        raise ValueError(f"License family requires maintainer review: {expression}")


def license_documents(package, overrides, override_dir):
    check_expression(package.get("license"))
    root = Path(package["manifest_path"]).parent.resolve()
    files = {file for file in root.rglob("*") if file.is_file() and (file.name.upper().startswith(PREFIXES) or "licenses" in [part.lower() for part in file.relative_to(root).parts[:-1]])}
    if package.get("license_file"):
        files.add(root / package["license_file"])
    documents = []
    for file in sorted(files):
        if file.is_symlink() or not file.is_file() or not file.resolve().is_relative_to(root):
            raise ValueError("Dependency license file escapes its registry package")
        data = file.read_bytes()
        if len(data) > 2_000_000:
            raise ValueError("Dependency license file exceeds review bound")
        documents.append({"name": file.relative_to(root).as_posix(), "sha256": hashlib.sha256(data).hexdigest(), "text": data.decode("utf-8")})
    if not any(doc["name"].split("/")[-1].upper().startswith(("LICENSE", "LICENCE", "COPYING", "UNLICENSE")) for doc in documents):
        key = f"{package['name']}@{package['version']}"
        override = overrides.get(key)
        if not override or "error" in override:
            raise ValueError(f"Missing upstream license text; add a reviewed exact-version override: {key}")
        vcs = json.loads((root / ".cargo_vcs_info.json").read_text())
        if override["vcsCommit"] != vcs["git"]["sha1"] or override["declaredLicense"] != package["license"]:
            raise ValueError(f"Stale license override: {key}")
        if override.get("kind") == "reviewed-spdx-declaration":
            metadata = (root / "Cargo.toml.orig").read_bytes()
            archive_checksum = archive_digest(package)
            if hashlib.sha256(metadata).hexdigest() != override["metadataSha256"] or archive_checksum != override["crateArchiveSha256"] or package["authors"] != override["authors"]:
                raise ValueError(f"Reviewed declaration identity changed: {key}")
            saved_metadata = read_override(override_dir, {"file": override["metadataFile"], "sha256": override["metadataSha256"]})
            if saved_metadata != metadata:
                raise ValueError("Reviewed Cargo metadata differs from published crate")
            documents.append({"name": "published-Cargo.toml.orig", "source": override["crateSource"], "sha256": override["metadataSha256"], "noticeStatus": override["noticeStatus"], "text": metadata.decode("utf-8")})
            documents.append({"name": "reviewed-declaration-explanation", "text": "This exact published crate declares the SPDX license(s) shown in its verbatim Cargo metadata. No upstream full-text copyright/license notice was found, or its recorded upstream commit is unavailable as indicated above. The following canonical standard texts preserve their template placeholders; no copyright holder or year has been invented. This is a documented distribution-notice fallback, not a legal assurance.\n", "sha256": None})
            for standard in override["standardTexts"]:
                data = read_override(override_dir, standard)
                documents.append({"name": standard["file"], "source": standard["source"], "sha256": standard["sha256"], "text": data.decode("utf-8")})
        else:
            data = read_override(override_dir, override)
            documents.append({"name": "reviewed-upstream-license", "source": override["source"], "sha256": override["sha256"], "text": data.decode("utf-8")})

    return documents


def archive_digest(package):
    cache = Path(package["manifest_path"]).parent
    archive = cache.parents[2] / "cache" / cache.parent.name / f"{package['name']}-{package['version']}.crate"
    if not archive.is_file():
        raise ValueError(f"Locked crate archive is unavailable for identity verification: {package['name']}")
    return digest(archive)


def read_override(directory, record):
    file = directory / record["file"]
    if file.is_symlink() or not file.resolve().is_relative_to(directory.resolve()):
        raise ValueError("Invalid license override path")
    data = file.read_bytes()
    if hashlib.sha256(data).hexdigest() != record["sha256"]:
        raise ValueError("License override checksum mismatch")
    return data


def write_notice_bundle(path, sections):
    # Preserve upstream LF/CRLF bytes on every host; text-mode Windows writes
    # would convert LF to CRLF and can double existing CRLF sequences.
    path.write_bytes("".join(sections).encode("utf-8"))


def collect(root, target, output):
    graphs = []
    for triple, name in [(TARGETS[target]["rustTarget"], "usage-lens"), ("wasm32-unknown-unknown", "usage-lens-ui")]:
        metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--features", "usage-lens-ui/csr", "--filter-platform", triple], cwd=root))
        graphs.append(reachable(metadata, name))
    packages = graphs[0] | graphs[1]
    override_dir = root / "scripts/release/licenses"
    overrides = json.loads((override_dir / "overrides.json").read_text())
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    checksums = {(p["name"], p["version"], p.get("source")): p.get("checksum") for p in lock["package"]}
    records = []
    text = ["Usage Lens bundled dependency licenses and notices\n", "This inventory is a conservative union of native and embedded WASM dependency graphs, including build-time code generators. It excludes dev-only edges. Inclusion does not mean every component is linked into the final binary. Each upstream license governs its component; the project MIT license does not replace these terms.\n"]
    sqlite = None
    for package in sorted(packages.values(), key=lambda p: (p["name"], p["version"])):
        documents = license_documents(package, overrides, override_dir)
        record = {key: package[key] for key in ["name", "version", "license", "repository", "authors"]}
        record["source"] = package["source"]
        record["usage"] = {label: sorted(graph[package["id"]]["usageKinds"]) for label, graph in zip(["native", "wasm"], graphs) if package["id"] in graph}
        cache = Path(package["manifest_path"]).parent
        checksum = archive_digest(package)
        if not checksum or checksum != checksums.get((package["name"], package["version"], package["source"])):
            raise ValueError("Dependency archive identity differs from Cargo.lock")
        record["crateArchiveSha256"] = checksum
        record["cargoManifestSha256"] = hashlib.sha256((cache / "Cargo.toml.orig").read_bytes()).hexdigest()
        record["documents"] = [{k: v for k, v in document.items() if k != "text"} for document in documents]
        records.append(record)
        text.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nDeclared license: {package['license']}\nRepository: {package['repository'] or '(not declared)'}\nAuthors (upstream metadata): {json.dumps(package['authors'], ensure_ascii=False)}\nDependency roles: {json.dumps(record['usage'], sort_keys=True)}\n")
        for document in documents:
            text.append(f"\n--- {document['name']} (SHA-256 {document['sha256']}) ---\n{document['text']}\n")
        if package["name"] == "libsqlite3-sys":
            sqlite = Path(package["manifest_path"]).parent / "sqlite3/sqlite3.c"
    if sqlite is None:
        raise ValueError("Expected bundled SQLite source for its public-domain notice")
    sqlite_source = sqlite.read_bytes().decode("utf-8")
    match = re.search(r"/\*\n\*\* 2001 September 15.*?\*/", sqlite_source, re.S)
    if not match or "disclaims copyright" not in match.group(0).lower():
        raise ValueError("Bundled SQLite public-domain notice was not found")
    text.append("\n" + "=" * 72 + "\nSQLite amalgamation public-domain notice (from the exact bundled source)\n" + match.group(0) + "\nSource: https://www.sqlite.org/copyright.html\n")
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    rust_docs = sysroot / "share/doc/rust"
    copyright_file = rust_docs / "COPYRIGHT-library.html"
    if not copyright_file.is_file():
        raise ValueError("Rust standard-library copyright bundle is missing")
    # Preserve the complete official standard-library notice bundle. It includes
    # upstream runtime components such as compiler builtins, allocator and musl.
    for file in sorted((rust_docs / "licenses").glob("*.txt")):
        text.append(f"\n{'=' * 72}\nRust toolchain license text: {file.name}\n{file.read_bytes().decode('utf-8')}\n")
    rust_version = subprocess.check_output(["rustc", "--version"], text=True).strip()
    wasm_version = subprocess.check_output(["wasm-bindgen", "--version"], text=True).strip()
    output.mkdir(parents=True, exist_ok=True)
    write_notice_bundle(output / "THIRD_PARTY_LICENSES.txt", text)
    (output / "RUST_STANDARD_LIBRARY_NOTICES.html").write_bytes(copyright_file.read_bytes())
    index = {"schemaVersion": 1, "platform": target, "cargoLockSha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(), "toolchain": rust_version, "wasmBindgen": wasm_version, "components": records, "sqliteNotice": "THIRD_PARTY_LICENSES.txt", "rustStandardLibraryNotices": "RUST_STANDARD_LIBRARY_NOTICES.html"}
    (output / "THIRD_PARTY_COMPONENTS.json").write_bytes(json_bytes(index))
    print(f"Collected full license texts/notices for {len(records)} locked native/WASM components")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    collect(Path(__file__).resolve().parents[2], args.target, args.out.resolve())


if __name__ == "__main__":
    main()
