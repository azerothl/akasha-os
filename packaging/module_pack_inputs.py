#!/usr/bin/env python3
"""Compute a stable fingerprint for one official WASM module pack build."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path


MODULES = {
    "notes",
    "tasks",
    "ext-rt",
    "canvas",
    "create",
    "illustration-studio",
}
TARGET = "wasm32-unknown-unknown"
FINGERPRINT_SCHEMA = "aos-module-build-inputs/v1"
IGNORED_DIRECTORIES = {".git", "target", "node_modules", "dist", "build"}
TEXT_SUFFIXES = {
    ".c", ".css", ".html", ".js", ".json", ".lock", ".md", ".ps1",
    ".rs", ".sh", ".toml", ".ts", ".tsx", ".txt", ".wgsl", ".yaml", ".yml",
}
PACKAGING_BUILD_SCRIPTS = {
    "packaging/build-preview.ps1",
    "packaging/build-preview.sh",
    "packaging/build-preview-macos.sh",
}


def is_input(path: str, module: str) -> bool:
    generated_names = {"module.wasm", "manifest.yaml", "build-inputs.sha256"}
    if Path(path).name in generated_names and (
        path.startswith(f"share/modules/{module}.aospkg/")
        or path.startswith(f"modules/{module}.aospkg/")
    ):
        # Canvas keeps its source manifest template in the shared package dir.
        if path == "share/modules/canvas.aospkg/manifest.yaml":
            return True
        return False

    prefixes = (
        f"modules/{module}/",
        f"modules/{module}.aospkg/",
        f"share/modules/{module}.aospkg/",
        "modules/sdk/",
    )
    if path.startswith(prefixes):
        return True
    if path in {
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        "packaging/module_pack_inputs.py",
        "rust-toolchain",
        "rust-toolchain.toml",
        "modules/sdk/Cargo.toml",
        f"modules/{module}/Cargo.lock",
        f"modules/build-{module}.ps1",
        f"modules/build-{module}.sh",
    }:
        return True
    if path in PACKAGING_BUILD_SCRIPTS:
        return True
    return module == "illustration-studio" and path.startswith("crates/aos-scene/")


def input_paths(root: Path, module: str) -> list[Path]:
    scopes = [
        root / f"modules/{module}",
        root / f"modules/{module}.aospkg",
        root / f"share/modules/{module}.aospkg",
        root / "modules/sdk",
        root / "crates/aos-scene" if module == "illustration-studio" else None,
    ]
    result: set[Path] = set()
    for scope in scopes:
        if scope is None or not scope.exists():
            continue
        if scope.is_file():
            result.add(scope)
            continue
        for directory, names, files in os.walk(scope):
            names[:] = sorted(name for name in names if name not in IGNORED_DIRECTORIES)
            parent = Path(directory)
            result.update(parent / name for name in files)

    for relative in (
        "Cargo.toml",
        "Cargo.lock",
        ".cargo/config.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
        f"modules/build-{module}.ps1",
        f"modules/build-{module}.sh",
        "packaging/module_pack_inputs.py",
    ) + tuple(sorted(PACKAGING_BUILD_SCRIPTS)):
        path = root / relative
        if path.is_file():
            result.add(path)
    return sorted(
        (path for path in result if is_input(path.relative_to(root).as_posix(), module)),
        key=lambda path: path.relative_to(root).as_posix(),
    )


def stable_file_bytes(path: Path) -> bytes:
    data = path.read_bytes()
    if path.suffix.lower() in TEXT_SUFFIXES or path.name in {"Cargo.lock", "LICENSE", "Makefile"}:
        return data.replace(b"\r\n", b"\n").replace(b"\r", b"\n")
    return data


def toolchain_fingerprint(root: Path) -> bytes:
    rustc = subprocess.run(
        ["rustc", "-Vv"], check=True, text=True, capture_output=True
    ).stdout
    # The compiler host triple differs across the three release runners, while
    # the WASM target and compiler build are shared. Exclude only that host line.
    rustc = "\n".join(line for line in rustc.splitlines() if not line.startswith("host:"))
    cargo = subprocess.run(
        ["cargo", "-V"], check=True, text=True, capture_output=True
    ).stdout.strip()
    flags = {
        name: value
        for name, value in sorted(os.environ.items())
        if name in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"}
        or name.startswith("CARGO_PROFILE_RELEASE_")
        or name.startswith("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_")
    }
    proto = (root / "crates/aos-proto/src/lib.rs").read_text(encoding="utf-8")
    match = re.search(r"pub const OS_API_VERSION:\s*u32\s*=\s*(\d+)\s*;", proto)
    if not match:
        raise SystemExit("cannot find OS_API_VERSION in crates/aos-proto/src/lib.rs")
    context = {
        "target": TARGET,
        "rustc": rustc,
        "cargo": cargo,
        "release_flags": flags,
        "host_api": int(match.group(1)),
    }
    return json.dumps(context, sort_keys=True, separators=(",", ":")).encode("utf-8")


def fingerprint(root: Path, module: str) -> str:
    if module not in MODULES:
        raise SystemExit(f"unsupported official module: {module}")
    digest = hashlib.sha256()
    paths = input_paths(root, module)
    if not paths:
        raise SystemExit(f"no build input files found for {module}")
    digest.update(FINGERPRINT_SCHEMA.encode("ascii") + b"\0")
    digest.update(module.encode("ascii") + b"\0")
    digest.update(TARGET.encode("ascii") + b"\0")
    for file_path in paths:
        # Normalize text line endings so Windows, Linux, and macOS checkouts
        # produce one fingerprint for identical source content.
        path = file_path.relative_to(root).as_posix()
        data = stable_file_bytes(file_path)
        digest.update(path.encode("utf-8"))
        digest.update(b"\0")
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    toolchain = toolchain_fingerprint(root)
    digest.update(b"toolchain\0")
    digest.update(len(toolchain).to_bytes(8, "big"))
    digest.update(toolchain)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("module", choices=sorted(MODULES))
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--json", action="store_true", help="print catalogue-ready JSON metadata")
    args = parser.parse_args()
    value = fingerprint(args.root.resolve(), args.module)
    if args.json:
        print(json.dumps({"module": args.module, "target": TARGET, "input_sha256": value}))
    else:
        print(value)
    return 0


if __name__ == "__main__":
    sys.exit(main())
