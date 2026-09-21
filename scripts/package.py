#!/usr/bin/env python3
"""Build and name the native library using CPA's plugin discovery convention."""

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tomllib
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Rust target triple (defaults to the host)")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    host = subprocess.check_output(["rustc", "-vV"], text=True)
    target = args.target or next(line.removeprefix("host: ") for line in host.splitlines() if line.startswith("host: "))
    if "apple-darwin" in target:
        goos, source = "darwin", "libkey_expiration.dylib"
    elif "linux" in target:
        goos, source = "linux", "libkey_expiration.so"
    elif "windows" in target:
        goos, source = "windows", "key_expiration.dll"
    else:
        parser.error(f"Unsupported platform: {target}")
    goarch = {"aarch64": "arm64", "x86_64": "amd64"}.get(target.split("-")[0])
    if not goarch:
        parser.error(f"Unsupported architecture: {target}")
    command = ["cargo", "build", "--release", "--locked"]
    if args.target:
        command += ["--target", args.target]
    subprocess.run(command, cwd=root, check=True)
    build_dir = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
    if not build_dir.is_absolute():
        build_dir = root / build_dir
    if args.target:
        build_dir /= args.target
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    library = dist / f"key-expiration{Path(source).suffix}"
    shutil.copy2(build_dir / "release" / source, library)
    archive = dist / f"key-expiration_{version}_{goos}_{goarch}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
        bundle.write(library, library.name)
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(".zip.sha256").write_text(f"{checksum}  {archive.name}\n")
    print(library)
    print(archive)


if __name__ == "__main__":
    main()
