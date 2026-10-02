"""Synthetic license-evidence checks; no network, accounts, or production data."""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/release"))
from build import verify_macos_minimum
from notices import check_expression, license_documents, reachable, read_override, write_notice_bundle


class NoticeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "crate"
        self.root.mkdir()
        self.package = {"name": "synthetic", "version": "1.0.0", "license": "MIT", "license_file": None, "manifest_path": str(self.root / "Cargo.toml"), "authors": ["Synthetic Author"]}
        self.overrides = Path(self.temp.name) / "overrides"
        self.overrides.mkdir()

    def test_unknown_license_requires_review(self):
        for expression in ["MIT", "MIT OR Apache-2.0", "(MIT OR Apache-2.0) AND Unicode-3.0", "MIT/Apache-2.0"]:
            check_expression(expression)
        for expression in [None, "", "Unknown-License", "GPL-3.0-only"]:
            with self.assertRaises(ValueError):
                check_expression(expression)

    def test_existing_upstream_text_is_preserved(self):
        for newline in ("\n", "\r\n"):
            with self.subTest(newline=repr(newline)):
                text = f"Synthetic full license text{newline}{newline}"
                data = text.encode("utf-8")
                (self.root / "LICENSE").write_bytes(data)
                docs = license_documents(self.package, {}, self.overrides)
                self.assertEqual(docs[0]["text"], text)
                self.assertEqual(docs[0]["sha256"], hashlib.sha256(data).hexdigest())

    def test_missing_text_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "Missing upstream license"):
            license_documents(self.package, {}, self.overrides)

    def test_exact_override_hash_and_identity(self):
        data = b"Synthetic upstream license\n"
        (self.overrides / "license.txt").write_bytes(data)
        (self.root / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": "a" * 40}}))
        record = {"file": "license.txt", "sha256": hashlib.sha256(data).hexdigest(), "vcsCommit": "a" * 40, "declaredLicense": "MIT", "source": "https://example.invalid/synthetic"}
        docs = license_documents(self.package, {"synthetic@1.0.0": record}, self.overrides)
        self.assertEqual(docs[0]["text"], data.decode())
        with self.assertRaisesRegex(ValueError, "Stale"):
            license_documents(self.package, {"synthetic@1.0.0": {**record, "vcsCommit": "b" * 40}}, self.overrides)
        with self.assertRaisesRegex(ValueError, "checksum"):
            read_override(self.overrides, {**record, "sha256": "b" * 64})
        with self.assertRaisesRegex(ValueError, "Missing upstream"):
            license_documents({**self.package, "version": "1.0.1"}, {"synthetic@1.0.0": record}, self.overrides)

    def test_reviewed_declaration_retains_metadata_and_placeholders(self):
        metadata = b'[package]\nlicense = "MIT"\nauthors = ["Synthetic Author"]\n'
        standard = b"Synthetic canonical license with <year> <copyright holders> placeholders\n"
        (self.root / "Cargo.toml.orig").write_bytes(metadata)
        (self.root / ".cargo_vcs_info.json").write_text(json.dumps({"git": {"sha1": "a" * 40}}))
        (self.overrides / "metadata.orig").write_bytes(metadata)
        (self.overrides / "standard.txt").write_bytes(standard)
        record = {"kind": "reviewed-spdx-declaration", "vcsCommit": "a" * 40, "declaredLicense": "MIT", "authors": ["Synthetic Author"], "metadataFile": "metadata.orig", "metadataSha256": hashlib.sha256(metadata).hexdigest(), "crateArchiveSha256": "c" * 64, "crateSource": "https://example.invalid/crate", "noticeStatus": "no-upstream-license-or-copyright-notice-file-found", "standardTexts": [{"file": "standard.txt", "sha256": hashlib.sha256(standard).hexdigest(), "source": "https://example.invalid/standard"}]}
        with patch("notices.archive_digest", return_value="c" * 64):
            docs = license_documents(self.package, {"synthetic@1.0.0": record}, self.overrides)
            self.assertEqual(docs[0]["text"], metadata.decode())
            self.assertIn("not a legal assurance", docs[1]["text"])
            self.assertEqual(docs[2]["text"], standard.decode())
        with patch("notices.archive_digest", return_value="d" * 64), self.assertRaisesRegex(ValueError, "identity changed"):
            license_documents(self.package, {"synthetic@1.0.0": record}, self.overrides)

    def test_dependency_roles_and_dev_exclusion(self):
        def package(name, source="registry", kind="lib"):
            return {"id": name, "name": name, "source": source, "targets": [{"kind": [kind]}]}
        def dep(name, kind=None):
            return {"pkg": name, "dep_kinds": [{"kind": kind}]}
        m = {"packages": [package("root", None), package("normal"), package("macro", kind="proc-macro"), package("helper"), package("builder"), package("dev")], "resolve": {"nodes": [{"id": "root", "deps": [dep("normal"), dep("macro"), dep("builder", "build"), dep("dev", "dev")]}, {"id": "macro", "deps": [dep("helper")]}, *[{"id": name, "deps": []} for name in ["normal", "builder", "helper", "dev"]]]}}
        result = reachable(m, "root")
        self.assertNotIn("dev", result)
        self.assertEqual(result["normal"]["usageKinds"], ["normal-dependency"])
        for name in ["macro", "helper", "builder"]:
            self.assertEqual(result[name]["usageKinds"], ["build/proc-macro"])
        with self.assertRaises(ValueError):
            reachable(m, "absent")

    def test_all_checked_in_notice_snapshots_match_pinned_bytes(self):
        directory = Path(__file__).resolve().parents[2] / "scripts/release/licenses"
        overrides = json.loads((directory / "overrides.json").read_bytes())
        checked = set()
        for key, record in overrides.items():
            snapshots = record["standardTexts"] + [{"file": record["metadataFile"], "sha256": record["metadataSha256"]}] if record.get("kind") == "reviewed-spdx-declaration" else [record]
            for snapshot in snapshots:
                with self.subTest(package=key, file=snapshot["file"]):
                    read_override(directory, snapshot)
                    checked.add(snapshot["file"])
        self.assertTrue(checked)

    def test_generated_notice_bundle_preserves_mixed_upstream_newlines(self):
        path = self.root / "THIRD_PARTY_LICENSES.txt"
        sections = ["Bundle heading\n", "Upstream LF text\n\n", "Upstream CRLF text\r\n\r\n", "Bundle footer\n"]
        write_notice_bundle(path, sections)
        self.assertEqual(path.read_bytes(), b"Bundle heading\nUpstream LF text\n\nUpstream CRLF text\r\n\r\nBundle footer\n")

    def test_git_autocrlf_checkout_preserves_vendored_notice_bytes(self):
        repository = self.root / "checkout"
        repository.mkdir()
        environment = {**os.environ, "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull}
        # Exercise actual Git checkout conversion without touching user/repo config.
        def git(*args):
            return subprocess.run(["git", "-c", "core.autocrlf=true", "-c", "core.safecrlf=false", *args], cwd=repository, env=environment, check=True, capture_output=True).stdout
        git("-c", "init.templateDir=", "init", "--quiet")
        attributes = Path(__file__).resolve().parents[2] / ".gitattributes"
        (repository / ".gitattributes").write_bytes(attributes.read_bytes())
        snapshots = {
            "scripts/release/licenses/synthetic-lf.txt": b"Synthetic LF notice\n\n",
            "scripts/release/licenses/synthetic-crlf.txt": b"Synthetic CRLF notice\r\n\r\n",
            "scripts/release/licenses/nested/synthetic-Cargo.toml.orig": b'[package]\nlicense = "MIT"\n',
        }
        for name, data in snapshots.items():
            file = repository / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(data)
        (repository / "normal.txt").write_bytes(b"Git conversion control\n")
        git("add", "--", ".gitattributes", "normal.txt", *snapshots)
        for name, data in snapshots.items():
            self.assertEqual(git("show", f":{name}"), data, f"Git index changed {name}")
            (repository / name).unlink()
        (repository / "normal.txt").unlink()
        git("checkout-index", "--all", "--force")
        self.assertEqual((repository / "normal.txt").read_bytes(), b"Git conversion control\r\n", "Control must prove autocrlf was active")
        for name, data in snapshots.items():
            with self.subTest(file=name):
                self.assertEqual((repository / name).read_bytes(), data)

    def test_macos_minimum_only_reads_relevant_load_commands(self):
        verify_macos_minimum("Load command 1\n cmd LC_BUILD_VERSION\n minos 11.0\n sdk 15.0\nLoad command 2\n cmd LC_SOURCE_VERSION\n version 0.0\n")
        verify_macos_minimum("Load command 1\n cmd LC_VERSION_MIN_MACOSX\n version 11.0.0\n sdk 15.0\n")
        for text in ["", "Load command 1\n cmd LC_BUILD_VERSION\n minos 15.0\n", "Load command 1\n cmd LC_SOURCE_VERSION\n version 11.0\n"]:
            with self.assertRaises(ValueError):
                verify_macos_minimum(text)


if __name__ == "__main__":
    unittest.main()
