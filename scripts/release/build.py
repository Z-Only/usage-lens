#!/usr/bin/env python3
"""Build a locked native target + embedded UI and verify the executable package."""
import argparse
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

from common import TARGETS
from package import package, smoke_archive, validate_host
from notices import collect


def verify_macos_minimum(commands):
    minima = []
    for command in re.split(r"^Load command \d+\s*$", commands, flags=re.M):
        if re.search(r"^\s*cmd LC_BUILD_VERSION\s*$", command, re.M):
            minima.extend(re.findall(r"^\s*minos (\d+\.\d+(?:\.\d+)?)\s*$", command, re.M))
        elif re.search(r"^\s*cmd LC_VERSION_MIN_MACOSX\s*$", command, re.M):
            minima.extend(re.findall(r"^\s*version (\d+\.\d+(?:\.\d+)?)\s*$", command, re.M))
    if len(minima) != 1 or tuple(map(int, minima[0].split(".")))[:2] != (11, 0):
        raise ValueError("Mach-O deployment minimum must be verified as macOS 11.0")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--out", type=Path, default=Path("release-assets"))
    args = parser.parse_args()
    validate_host(args.target)
    root = Path(__file__).resolve().parents[2]
    os.chdir(root)
    version = tomllib.loads((root / "crates/usage-lens/Cargo.toml").read_text())["package"]["version"]
    target = TARGETS[args.target]["rustTarget"]
    # Metadata/license collection is offline after this locked registry fetch.
    subprocess.run(["cargo", "fetch", "--locked"], check=True)
    subprocess.run([sys.executable, "scripts/build_ui.py"], check=True)
    env = os.environ.copy()
    if args.target == "windows-x64":
        env["RUSTFLAGS"] = "-C target-feature=+crt-static"
    elif args.target.startswith("macos-"):
        env["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "usage-lens", "--target", target], env=env, check=True)
    name = "usage-lens.exe" if args.target == "windows-x64" else "usage-lens"
    binary = root / "target" / target / "release" / name
    if args.target == "linux-x64":
        headers = subprocess.check_output(["readelf", "-l", str(binary)], text=True)
        dynamic = subprocess.check_output(["readelf", "-d", str(binary)], text=True)
        if "INTERP" in headers or "(NEEDED)" in dynamic:
            raise ValueError("Linux release must be static musl, without a dynamic interpreter or shared dependencies")
    if args.target.startswith("macos-"):
        commands = subprocess.check_output(["otool", "-l", str(binary)], text=True)
        verify_macos_minimum(commands)
    collect(root, args.target, root / "target/release-notices" / args.target)
    archive = package(root, binary, args.out.resolve(), version, args.commit, args.target)
    smoke_archive(archive, root)
    print(f"Verified native build and extracted package: {archive.name}")


if __name__ == "__main__":
    main()
