"""Synthetic packaging/release safety tests; never call GitHub or user accounts."""
import copy
import hashlib
import io
import json
import os
import posixpath
import re
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from urllib.parse import unquote, urlsplit

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts/release"
sys.path.insert(0, str(SCRIPTS))
from common import FILES, GENERATED_FILES, TARGETS, asset_base, changelog_section, digest, json_bytes, safe_relative, validate_identity, verify_archive, version_tuple
from guard import validate_ci, validate_progress
from github import GitHub, GitHubError
from package import package, validate_host
from publish import assemble, publish, verify_uploaded

SHA = "a" * 40
VERSION = "0.1.0"
CHANGELOG = "# Changes\n\n## [Unreleased]\nFuture\n\n## [0.1.0] - 2026-10-02\n\nUseful batch.\n\n## [0.0.1]\nOlder\n"


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "project"
        self.root.mkdir()
        for name in FILES:
            file = self.root / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(f"synthetic: {name}\n")
        for target in TARGETS:
            notices = self.root / "target/release-notices" / target
            notices.mkdir(parents=True)
            for name in GENERATED_FILES:
                (notices / name).write_text("synthetic notice fixture")
        self.binary = self.root / "native-binary"
        self.binary.write_bytes(b"synthetic-native-binary-not-executed")
        self.out = self.root.parent / "out"

    def build(self, target="linux-x64", out=None):
        with patch("package.validate_host"):
            archive = package(self.root, self.binary, out or self.out, VERSION, SHA, target)
        manifest = json.loads(archive.with_name(archive.name.removesuffix(".tar.gz") + ".manifest.json").read_text())
        return archive, manifest

    def rewrite_archive(self, archive, mutate):
        with tarfile.open(archive, "r:gz") as tar:
            members = [(m, tar.extractfile(m).read()) for m in tar]
        mutate(members)
        with tarfile.open(archive, "w:gz") as tar:
            for member, data in members:
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))

    def test_stable_semver_is_strict(self):
        self.assertEqual(version_tuple("12.34.56"), (12, 34, 56))
        for invalid in ["v0.1.0", "01.0.0", "0.01.0", "1.0", "1.0.0-rc.1", "1.0.0+build", "1.0.0\n", "1.0.0;echo X", "-1.0.0"]:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                version_tuple(invalid)
        for invalid in ["HEAD", "a" * 39, "A" * 40, SHA + "\n"]:
            with self.assertRaises(ValueError):
                validate_identity(VERSION, invalid)

    def test_unsupported_target(self):
        with self.assertRaises(ValueError):
            asset_base(VERSION, "linux-arm64")

    def test_native_host_checks_prevent_cross_arch_labels(self):
        for target, info in TARGETS.items():
            with patch("package.platform.system", return_value=info["os"]), patch("package.platform.machine", return_value=info["arch"]):
                validate_host(target)
            with patch("package.platform.system", return_value="not-the-os"), self.assertRaises(ValueError):
                validate_host(target)
            with patch("package.platform.system", return_value=info["os"]), patch("package.platform.machine", return_value="not-the-cpu"), self.assertRaises(ValueError):
                validate_host(target)

    def test_packaging_is_allowlisted_and_reproducible(self):
        (self.root / "private.sqlite").write_text("synthetic private canary")
        (self.root / ".env").write_text("synthetic secret canary")
        (self.root / "runtime.log").write_text("synthetic log canary")
        archive, manifest = self.build()
        verify_archive(archive, manifest)
        self.assertEqual(set(manifest["files"]), set(FILES) | set(GENERATED_FILES) | {"usage-lens"})
        self.assertEqual(manifest["commit"], SHA)
        self.assertEqual(manifest["runtimePrerequisite"], "none")
        second, _ = self.build(out=self.root.parent / "second")
        self.assertEqual(digest(archive), digest(second))
        with tarfile.open(archive) as tar:
            for member in tar:
                self.assertTrue(member.isfile())
                self.assertEqual(member.uid, 0)
                self.assertEqual(member.mtime, 0)
                self.assertNotIn("private", member.name)

    def test_message_reading_guide_and_bundled_references_resolve_in_every_platform_archive(self):
        guide = "docs/message-reading.md"
        referring_docs = {"README.md", "CHANGELOG.md", "docs/data-contract.md", "docs/verification.md"}
        self.assertIn(guide, FILES, "The linked user guide must be explicitly reviewed for packaging")
        checkout = SCRIPTS.parents[1]
        for name in referring_docs | {guide}:
            (self.root / name).write_bytes((checkout / name).read_bytes())
        guide_bytes = (checkout / guide).read_bytes()
        for target in TARGETS:
            with self.subTest(target=target):
                archive, manifest = self.build(target)
                verify_archive(archive, manifest)
                prefix = asset_base(VERSION, target) + "/"
                with tarfile.open(archive, "r:gz") as tar:
                    bundled = {member.name.removeprefix(prefix): tar.extractfile(member).read() for member in tar}
                self.assertEqual(bundled[guide], guide_bytes)
                self.assertEqual(manifest["files"][guide], hashlib.sha256(guide_bytes).hexdigest())
                resolved_from = set()
                for name in referring_docs:
                    # These reviewed docs use ordinary inline Markdown links.
                    # Scope this regression to the new guide, not unrelated
                    # pre-existing relative links outside FILES.
                    for destination in re.findall(r"\[[^\]]*\]\(([^)]+)\)", bundled[name].decode("utf-8")):
                        url = urlsplit(destination)
                        if url.scheme or url.netloc or not url.path:
                            continue
                        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(name), unquote(url.path)))
                        if resolved == guide:
                            self.assertIn(resolved, bundled, f"Broken bundled reference in {name}")
                            resolved_from.add(name)
                self.assertEqual(resolved_from, referring_docs)

    def test_missing_message_reading_guide_blocks_packaging(self):
        (self.root / "docs/message-reading.md").unlink()
        with self.assertRaisesRegex(ValueError, "Missing or linked allowlisted input: docs/message-reading.md"):
            self.build()

    def test_windows_binary_filename(self):
        archive, manifest = self.build("windows-x64")
        self.assertIn("usage-lens.exe", manifest["files"])
        verify_archive(archive, manifest)

    def test_no_overwrite(self):
        self.build()
        with self.assertRaises(ValueError):
            self.build()

    def test_missing_file_and_link_rejected(self):
        (self.root / FILES[0]).unlink()
        with self.assertRaises(ValueError):
            self.build()
        try:
            (self.root / FILES[0]).symlink_to(self.binary)
        except OSError:
            self.skipTest("Symlinks need extra Windows privileges")
        with self.assertRaises(ValueError):
            self.build()

    def test_binary_link_rejected(self):
        link = self.root / "link"
        try:
            link.symlink_to(self.binary)
        except OSError:
            self.skipTest("Symlinks need extra Windows privileges")
        with patch("package.validate_host"), self.assertRaises(ValueError):
            package(self.root, link, self.out, VERSION, SHA, "linux-x64")

    def test_path_validation(self):
        for name in ["", "/a", "../a", "a/../b", "a//b", "./b", "a\\b", "C:/x", "a\nb", "a/./b"]:
            self.assertFalse(safe_relative(name), name)
        self.assertTrue(safe_relative("plugin/usage-lens/.codex-plugin/plugin.json"))

    def test_archive_tamper_detected(self):
        archive, manifest = self.build()
        self.rewrite_archive(archive, lambda members: members.__setitem__(0, (members[0][0], b"tampered")))
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_archive_missing_member_detected(self):
        archive, manifest = self.build()
        self.rewrite_archive(archive, lambda members: members.pop())
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_archive_duplicate_detected(self):
        archive, manifest = self.build()
        self.rewrite_archive(archive, lambda members: members.append(members[0]))
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_archive_traversal_detected(self):
        archive, manifest = self.build()
        self.rewrite_archive(archive, lambda members: setattr(members[0][0], "name", "../outside"))
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_archive_link_detected(self):
        archive, manifest = self.build()
        def mutate(members):
            members[0][0].type = tarfile.SYMTYPE
            members[0][0].linkname = "outside"
        self.rewrite_archive(archive, mutate)
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_manifest_identity_and_inventory_detected(self):
        archive, manifest = self.build()
        for field, value in [("schemaVersion", 2), ("rustTarget", "fake"), ("runtimePrerequisite", "node"), ("version", "invalid")]:
            broken = {**manifest, field: value}
            with self.assertRaises(ValueError):
                verify_archive(archive, broken)
        manifest["files"]["private.sqlite"] = "a" * 64
        with self.assertRaises(ValueError):
            verify_archive(archive, manifest)

    def test_changelog_version_required(self):
        self.assertEqual(changelog_section(CHANGELOG, VERSION), "## 0.1.0\n\nUseful batch.\n")
        with self.assertRaises(ValueError):
            changelog_section(CHANGELOG, "1.0.0")
        with self.assertRaises(ValueError):
            changelog_section("## [0.1.0]\n\n", VERSION)

    def test_complete_set_and_checksums(self):
        for target in TARGETS:
            self.build(target)
        files = assemble(self.out, VERSION, SHA, CHANGELOG)
        self.assertEqual(len(files), 11)
        for line in (self.out / "SHA256SUMS").read_text().splitlines():
            checksum, name = line.split("  ")
            self.assertEqual(checksum, digest(self.out / name))
        release = json.loads((self.out / "release.json").read_text())
        self.assertEqual({asset["platform"] for asset in release["assets"]}, set(TARGETS))

    def test_missing_platform_stops_assembly(self):
        self.build()
        with self.assertRaises(FileNotFoundError):
            assemble(self.out, VERSION, SHA, CHANGELOG)

    def test_stale_extra_files_stop_assembly(self):
        for target in TARGETS:
            self.build(target)
        (self.out / "stale.txt").write_text("stale")
        with self.assertRaises(ValueError):
            assemble(self.out, VERSION, SHA, CHANGELOG)

    def test_wrong_commit_stops_assembly(self):
        for target in TARGETS:
            self.build(target)
        with self.assertRaises(ValueError):
            assemble(self.out, VERSION, "b" * 40, CHANGELOG)

    def test_monotonic_stable_versions(self):
        validate_progress(VERSION, [{"tag_name": "v0.0.9"}, {"tag_name": "v9.0.0", "draft": True}, {"tag_name": "v9.0.0-rc.1", "prerelease": True}, {"tag_name": "misc"}])
        for tag in ["v0.1.0", "v0.2.0", "v1.0.0"]:
            with self.assertRaises(ValueError):
                validate_progress(VERSION, [{"tag_name": tag}])

    def test_exact_commit_ci_and_gate_required(self):
        run = {"id": 1, "run_number": 1, "head_sha": SHA, "head_branch": "main", "event": "push", "path": ".github/workflows/ci.yml", "status": "completed", "conclusion": "success"}
        jobs = lambda _: [{"name": "ci-gate", "conclusion": "success"}]
        validate_ci([run], jobs, SHA, run["path"], "ci-gate")
        for change in [{"head_sha": "b" * 40}, {"event": "pull_request"}, {"head_branch": "feature"}, {"path": "untrusted.yml"}, {"status": "in_progress"}, {"conclusion": "failure"}]:
            with self.assertRaises(ValueError):
                validate_ci([{**run, **change}], jobs, SHA, run["path"], "ci-gate")
        for bad_jobs in [[], [{"name": "other", "conclusion": "success"}], [{"name": "ci-gate", "conclusion": "skipped"}]]:
            with self.assertRaises(ValueError):
                validate_ci([run], lambda _: bad_jobs, SHA, run["path"], "ci-gate")
        with self.assertRaises(ValueError):
            validate_ci([run, {**run, "id": 2, "run_number": 2, "conclusion": "cancelled"}], jobs, SHA, run["path"], "ci-gate")

    def test_upload_digest_size_state_and_set(self):
        files = [self.binary]
        good = {"name": self.binary.name, "state": "uploaded", "size": self.binary.stat().st_size, "digest": "sha256:" + digest(self.binary)}
        verify_uploaded([good], files)
        for change in [{"name": "other"}, {"state": "starter"}, {"size": 0}, {"digest": None}, {"digest": "sha256:bad"}]:
            with self.assertRaises(ValueError):
                verify_uploaded([{**good, **change}], files)
        with self.assertRaises(ValueError):
            verify_uploaded([good, good], files)

    def test_unexpected_repository_or_upload_host_rejected(self):
        with self.assertRaises(ValueError):
            GitHub("other/repository", "synthetic")
        api = GitHub("Z-Only/usage-lens", "synthetic")
        for url in ["https://example.com/upload", "http://uploads.github.com/upload", "https://api.github.com.evil.test/upload"]:
            with self.assertRaises(ValueError):
                api.request(url)

    def test_publication_is_draft_first_and_never_overwrites(self):
        calls = []
        file = self.binary
        class Fake:
            def pages(self, path):
                if path == "/releases": return []
                return [{"name": file.name, "state": "uploaded", "size": file.stat().st_size, "digest": "sha256:" + digest(file)}]
            def request(self, path, method="GET", value=None, data=None):
                calls.append((method, path, value))
                if method == "POST" and path == "/releases":
                    return {"id": 1, "upload_url": "https://uploads.github.com/repos/Z-Only/usage-lens/releases/1/assets{?name}"}
                if method == "GET": return {"draft": True, "tag_name": "v0.1.0", "target_commitish": SHA}
                return {"draft": False, "tag_name": "v0.1.0", "html_url": "https://github.com/Z-Only/usage-lens/releases/tag/v0.1.0"}
        with patch("publish.check") as guard:
            publish(Fake(), "v0.1.0", SHA, [file], "synthetic notes")
            guard.assert_called_once()
        self.assertTrue(calls[0][2]["draft"])
        self.assertEqual(calls[-1][0], "PATCH")
        self.assertFalse(calls[-1][2]["draft"])
        fake = Fake()
        fake.pages = lambda _: [{"tag_name": "v0.1.0", "draft": True}]
        with self.assertRaises(ValueError):
            publish(fake, "v0.1.0", SHA, [file], "synthetic notes")

    def test_failed_upload_verification_never_publishes(self):
        calls = []
        class Fake:
            def pages(self, path): return []
            def request(self, path, method="GET", value=None, data=None):
                calls.append(method)
                return {"id": 1, "upload_url": "https://uploads.github.com/test"}
        with self.assertRaises(ValueError):
            publish(Fake(), "v0.1.0", SHA, [self.binary], "notes")
        self.assertNotIn("PATCH", calls)

    def test_manual_publication_creates_tag_only_before_draft(self):
        calls = []
        file = self.binary
        class Fake:
            def pages(self, path):
                if path == "/releases": return []
                return [{"name": file.name, "state": "uploaded", "size": file.stat().st_size, "digest": "sha256:" + digest(file)}]
            def request(self, path, method="GET", value=None, data=None):
                calls.append((method, path, value))
                if path.startswith("/git/ref/tags/"):
                    raise GitHubError(404, "synthetic missing tag")
                if path == "/git/refs":
                    return {"ref": "refs/tags/v0.1.0"}
                if method == "POST" and path == "/releases":
                    return {"id": 1, "upload_url": "https://uploads.github.com/test"}
                if method == "GET": return {"draft": True, "tag_name": "v0.1.0", "target_commitish": SHA}
                return {"draft": False, "tag_name": "v0.1.0", "html_url": "https://github.com/Z-Only/usage-lens/releases/tag/v0.1.0"}
        with patch("publish.check") as guard:
            publish(Fake(), "v0.1.0", SHA, [file], "synthetic", manual=True)
            self.assertEqual(guard.call_count, 2)
        writes = [(path, value) for method, path, value in calls if method == "POST"]
        self.assertEqual(writes[0], ("/git/refs", {"ref": "refs/tags/v0.1.0", "sha": SHA}))
        self.assertEqual(writes[1][0], "/releases")
        self.assertTrue(writes[1][1]["draft"])

    def test_workflow_has_least_privilege_and_no_arbitrary_ref(self):
        workflow = (SCRIPTS.parents[1] / ".github/workflows/release.yml").read_text()
        self.assertNotIn("pull_request_target", workflow)
        self.assertNotIn("secrets.", workflow)
        self.assertEqual(workflow.count("contents: write"), 1)
        self.assertIn("needs: [eligible, build]", workflow)
        self.assertIn("ref: main", workflow)
        self.assertIn("refs/heads/main", (SCRIPTS / "guard.py").read_text())
        for target in TARGETS.values():
            self.assertIn(target["runner"], workflow)


if __name__ == "__main__":
    unittest.main()
