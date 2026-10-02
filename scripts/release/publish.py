#!/usr/bin/env python3
"""Verify this run's complete artifact set, upload a draft, then publish atomically."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from urllib.parse import quote

from common import TARGETS, asset_base, changelog_section, digest, json_bytes, validate_identity, verify_archive
from github import GitHub, GitHubError
from guard import check, git


def assemble(directory: Path, version: str, sha: str, changelog: str) -> list[Path]:
    validate_identity(version, sha)
    expected = set()
    builds = []
    for target in TARGETS:
        base = asset_base(version, target)
        archive = directory / f"{base}.tar.gz"
        manifest_file = directory / f"{base}.manifest.json"
        expected.update([archive.name, manifest_file.name])
        manifest = json.loads(manifest_file.read_text())
        if manifest.get("version") != version or manifest.get("commit") != sha or manifest.get("platform") != target:
            raise ValueError("Artifact identity does not match the validated release")
        verify_archive(archive, manifest)
        builds.append({"platform": target, "archive": archive.name, "sha256": digest(archive), "manifest": manifest_file.name})
    if {p.name for p in directory.iterdir()} != expected or any(not p.is_file() or p.is_symlink() for p in directory.iterdir()):
        raise ValueError("Missing or unexpected release artifacts; refusing cross-run/stale files")
    (directory / "CHANGELOG.md").write_text(changelog_section(changelog, version), encoding="utf-8")
    release = {"schemaVersion": 1, "version": version, "commit": sha, "repository": "Z-Only/usage-lens", "workflowRun": os.environ.get("GITHUB_RUN_ID"), "workflowAttempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "assets": builds}
    (directory / "release.json").write_bytes(json_bytes(release))
    assets = sorted(directory.iterdir())
    checksums = "".join(f"{digest(path)}  {path.name}\n" for path in assets)
    (directory / "SHA256SUMS").write_text(checksums, encoding="ascii")
    return [*assets, directory / "SHA256SUMS"]


def verify_uploaded(assets, files):
    expected = {file.name: file for file in files}
    if len(assets) != len(expected) or {asset["name"] for asset in assets} != set(expected):
        raise ValueError("Uploaded release asset set differs from the complete verified build")
    for asset in assets:
        file = expected[asset["name"]]
        if asset.get("state") != "uploaded" or asset.get("size") != file.stat().st_size or asset.get("digest") != f"sha256:{digest(file)}":
            raise ValueError("GitHub upload digest/size/state validation failed; release stays draft")


def publish(api, tag: str, sha: str, files: list[Path], notes: str, manual=False):
    # Never overwrite or append to an existing draft/public release. An operator must
    # inspect failed drafts and explicitly clean one up before retrying publication.
    if any(release["tag_name"] == tag for release in api.pages("/releases")):
        raise ValueError("A release/draft already exists for this tag; refusing an ambiguous retry")
    if manual:
        try:
            api.request(f"/git/ref/tags/{quote(tag, safe='')}")
        except GitHubError as error:
            if error.status != 404:
                raise
            api.request("/git/refs", "POST", {"ref": f"refs/tags/{tag}", "sha": sha})
        check(api, tag, sha)
    draft = api.request("/releases", "POST", {"tag_name": tag, "target_commitish": sha, "name": f"Usage Lens {tag}", "body": notes, "draft": True, "prerelease": False})
    upload = draft["upload_url"].split("{")[0]
    for file in files:
        api.request(f"{upload}?name={quote(file.name, safe='')}", "POST", data=file.read_bytes())
    assets = api.pages(f"/releases/{draft['id']}/assets")
    verify_uploaded(assets, files)
    # Fail closed if main/CI/tag eligibility changed while builds/uploads ran.
    check(api, tag, sha)
    current = api.request(f"/releases/{draft['id']}")
    if not current.get("draft") or current.get("tag_name") != tag or current.get("target_commitish") != sha:
        raise ValueError("Draft release changed during upload")
    result = api.request(f"/releases/{draft['id']}", "PATCH", {"draft": False, "make_latest": "true"})
    if result.get("draft") or result.get("tag_name") != tag:
        raise ValueError("Publication result is uncertain; inspect GitHub before retrying")
    print(f"Published complete verified release: {result['html_url']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--sha", required=True)
    args = parser.parse_args()
    manual = os.environ.get("GITHUB_EVENT_NAME") == "workflow_dispatch"
    if manual:
        if os.environ.get("GITHUB_REF") != "refs/heads/main" or os.environ.get("GITHUB_SHA") != args.sha:
            raise ValueError("Manual publication requires its original main commit")
    elif os.environ.get("GITHUB_EVENT_NAME") != "push" or os.environ.get("GITHUB_REF_TYPE") != "tag" or os.environ.get("GITHUB_REF_NAME") != args.tag:
        raise ValueError("Publication requires the original matching tag-push event")
    api = GitHub()
    version, sha = check(api, args.tag, args.sha, allow_missing_tag=manual)
    changelog = git("show", f"{sha}:CHANGELOG.md")
    files = assemble(args.directory, version, sha, changelog)
    notes = changelog_section(changelog, version) + f"\nBuilt from `{sha}` in [GitHub Actions](https://github.com/Z-Only/usage-lens/actions/runs/{os.environ['GITHUB_RUN_ID']}).\n\nDownload the matching native archive, manifest, and SHA256SUMS. No Node or Bun runtime is required. Follow [AI-assisted installation](https://github.com/Z-Only/usage-lens/blob/{args.tag}/docs/AI_INSTALL.md).\n"
    publish(api, args.tag, sha, files, notes, manual=manual)


if __name__ == "__main__":
    main()
