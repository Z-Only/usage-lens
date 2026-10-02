#!/usr/bin/env python3
"""Fail-closed release eligibility guard, run from reviewed main, not the tag."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import tomllib
from pathlib import Path
from urllib.parse import quote

from common import changelog_section, validate_identity, version_tuple
from github import GitHub, GitHubError

REQUIRED_WORKFLOWS = {"ci.yml": "ci-gate", "release-check.yml": "package-gate"}


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def validate_ci(runs, jobs, sha, workflow_path, job_name):
    # Use newest matching push run. A later failed/cancelled rerun must not be hidden
    # by an older success; PR and merge-group checks are not main-commit evidence.
    eligible = [r for r in runs if r.get("head_sha") == sha and r.get("head_branch") == "main" and r.get("event") == "push" and r.get("path") == workflow_path]
    if not eligible:
        raise ValueError(f"No exact-commit main push CI: {workflow_path}")
    latest = max(eligible, key=lambda r: (r["run_number"], r.get("run_attempt", 1)))
    if latest.get("status") != "completed" or latest.get("conclusion") != "success":
        raise ValueError(f"Exact-commit CI has not succeeded: {workflow_path}")
    matches = [j for j in jobs(latest["id"]) if j["name"] == job_name]
    if len(matches) != 1 or matches[0].get("conclusion") != "success":
        raise ValueError(f"Required aggregate job did not succeed: {job_name}")


def validate_progress(version, releases):
    for release in releases:
        if release.get("draft") or release.get("prerelease"):
            continue
        tag = release.get("tag_name", "")
        if not tag.startswith("v"):
            continue
        try:
            prior = version_tuple(tag[1:])
        except ValueError:
            continue
        if version_tuple(version) <= prior:
            raise ValueError("Stable release version must advance beyond every published stable release")


def check(api, tag, expected_sha=None, allow_missing_tag=False):
    if not tag.startswith("v"):
        raise ValueError("Expected vMAJOR.MINOR.PATCH tag")
    version = tag[1:]
    version_tuple(version)
    # Fetch only origin's known main branch and tags. Never accept a caller-supplied ref.
    subprocess.run(["git", "fetch", "--no-recurse-submodules", "origin", "+refs/heads/main:refs/remotes/origin/main", "--tags"], check=True)
    try:
        sha = git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    except subprocess.CalledProcessError:
        if not allow_missing_tag or expected_sha is None:
            raise ValueError("Release tag is missing") from None
        sha = expected_sha
    validate_identity(version, sha)
    if expected_sha is not None and sha != expected_sha:
        raise ValueError("Tag moved or build commit differs from the validated release commit")
    subprocess.run(["git", "merge-base", "--is-ancestor", sha, "refs/remotes/origin/main"], check=True)
    cargo = tomllib.loads(git("show", f"{sha}:crates/usage-lens/Cargo.toml"))
    package = json.loads(git("show", f"{sha}:package.json"))
    plugin = json.loads(git("show", f"{sha}:plugin/usage-lens/.codex-plugin/plugin.json"))
    if any(actual != version for actual in [cargo["package"]["version"], package["version"], plugin["version"]]):
        raise ValueError("Tag, Cargo, frontend, and plugin versions must match")
    changelog_section(git("show", f"{sha}:CHANGELOG.md"), version)
    for workflow, job in REQUIRED_WORKFLOWS.items():
        runs = api.pages(f"/actions/workflows/{workflow}/runs?head_sha={sha}&event=push", "workflow_runs")
        validate_ci(runs, lambda run_id: api.pages(f"/actions/runs/{run_id}/jobs?filter=latest", "jobs"), sha, f".github/workflows/{workflow}", job)
    validate_progress(version, api.pages("/releases"))
    # Also compare the live remote tag, because fetched tags are not force-updated.
    try:
        remote = api.request(f"/git/ref/tags/{quote(tag, safe='')}")["object"]
    except GitHubError as error:
        if error.status == 404 and allow_missing_tag:
            return version, sha
        raise
    for _ in range(5):
        if remote["type"] == "commit":
            break
        if remote["type"] != "tag":
            raise ValueError("Release tag is not a commit/tag")
        remote = api.request(f"/git/tags/{remote['sha']}")["object"]
    if remote["type"] != "commit" or remote["sha"] != sha:
        raise ValueError("Remote tag no longer points at the validated commit")
    return version, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--expected-sha")
    args = parser.parse_args()
    manual = os.environ.get("GITHUB_EVENT_NAME") == "workflow_dispatch"
    if manual:
        if os.environ.get("GITHUB_REF") != "refs/heads/main":
            raise ValueError("Manual release must be dispatched from main")
    elif os.environ.get("GITHUB_EVENT_NAME") != "push" or os.environ.get("GITHUB_REF_TYPE") != "tag" or os.environ.get("GITHUB_REF_NAME") != args.tag:
        raise ValueError("Expected the original matching tag push or trusted main dispatch")
    event_sha = git("rev-parse", f"{os.environ['GITHUB_SHA']}^{{commit}}")
    version, sha = check(GitHub(), args.tag, args.expected_sha or event_sha, allow_missing_tag=manual)
    if os.environ.get("GITHUB_OUTPUT"):
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as handle:
            handle.write(f"version={version}\nsha={sha}\n")
    print(f"Eligible reviewed main commit: {sha}; release v{version}")


if __name__ == "__main__":
    main()
