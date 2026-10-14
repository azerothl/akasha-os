#!/usr/bin/env python3
"""Create signed-catalogue metadata and release ZIPs for optional WASM packs."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import zipfile
from pathlib import Path


OPTIONAL_MODULES = {"canvas", "create", "illustration-studio"}
PLATFORMS = {"windows-x64", "linux-x64", "macos-arm64"}


def module_manifest(package_dir: Path, module: str) -> tuple[str, str, int]:
    manifest = (package_dir / "manifest.yaml").read_text(encoding="utf-8")
    values: dict[str, str] = {}
    for key in ("name", "version", "hash", "min_os_api"):
        match = re.search(rf"(?m)^{re.escape(key)}:\s*['\"]?([^\r\n'\"]+)['\"]?\s*$", manifest)
        if not match:
            raise SystemExit(f"{package_dir / 'manifest.yaml'} has no {key}")
        values[key] = match.group(1).strip()
    if values["name"] != module:
        raise SystemExit(f"manifest module mismatch: expected {module}, found {values['name']}")
    wasm_hash = hashlib.sha256((package_dir / "module.wasm").read_bytes()).hexdigest()
    if values["hash"].removeprefix("sha256:").lower() != wasm_hash:
        raise SystemExit(f"manifest WASM hash mismatch for {module}")
    return values["version"], wasm_hash, int(values["min_os_api"])


def catalogue_block(catalogue: str, module: str) -> tuple[int, int, str]:
    matches = list(re.finditer(r"(?m)^  - name: ([A-Za-z0-9_-]+)\s*$", catalogue))
    for index, match in enumerate(matches):
        if match.group(1) != module:
            continue
        start = match.start()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(catalogue)
        block = catalogue[start:end]
        version_match = re.search(r"(?m)^    version:\s*['\"]?([^\r\n'\"]+)['\"]?\s*$", block)
        hash_match = re.search(r"(?m)^    hash:\s*(?:sha256:)?([a-fA-F0-9]{64})\s*$", block)
        if not version_match or not hash_match:
            raise SystemExit(f"catalogue entry is incomplete for {module}")
        return start, end, block
    raise SystemExit(f"catalogue entry is missing for {module}")


def replace_module_metadata(
    catalogue: str,
    module: str,
    version: str,
    wasm_hash: str,
    min_api: int,
    platform_id: str,
    url: str,
    archive_hash: str,
    archive_size: int,
    input_hash: str,
) -> str:
    start, end, block = catalogue_block(catalogue, module)
    version_match = re.search(r"(?m)^    version:\s*['\"]?([^\r\n'\"]+)['\"]?\s*$", block)
    hash_match = re.search(r"(?m)^    hash:\s*(?:sha256:)?([a-fA-F0-9]{64})\s*$", block)
    if not version_match or version_match.group(1) != version:
        raise SystemExit(f"manifest/catalogue version mismatch for {module}")
    if not hash_match or hash_match.group(1).lower() != wasm_hash:
        raise SystemExit(f"manifest/catalogue WASM hash mismatch for {module}")

    lines = block.splitlines(keepends=True)
    kept: list[str] = []
    skip_children = False
    for line in lines:
        if re.match(r"^    (?:package|platforms|minimum_host_api|dependencies):", line):
            skip_children = True
            continue
        if skip_children:
            if not line.strip() or len(line) - len(line.lstrip(" ")) > 4:
                continue
            skip_children = False
        kept.append(line)
    block = "".join(kept)
    hash_line = re.search(r"(?m)^    hash:.*(?:\r?\n|$)", block)
    if not hash_line:
        raise SystemExit(f"catalogue hash line is missing for {module}")
    metadata = (
        f"    minimum_host_api: {min_api}\n"
        f"    platforms:\n"
        f"      - {platform_id}\n"
        "    dependencies: []\n"
        f"    package:\n"
        f"      url: {url}\n"
        f"      sha256: sha256:{archive_hash}\n"
        f"      size_bytes: {archive_size}\n"
        f"      input_sha256: sha256:{input_hash}\n"
    )
    block = block[: hash_line.end()] + metadata + block[hash_line.end() :]
    return catalogue[:start] + block + catalogue[end:]


def create_archive(package_dir: Path, destination: Path) -> tuple[str, int]:
    destination.parent.mkdir(parents=True, exist_ok=True)
    files = sorted(package_dir.rglob("*"))
    files = [source for source in files if source.is_file() or source.is_symlink()]
    if len(files) > 4096:
        raise SystemExit(f"module package contains too many files: {package_dir}")
    if sum(source.stat().st_size for source in files if not source.is_symlink()) > 128 * 1024 * 1024:
        raise SystemExit(f"module package is too large after extraction: {package_dir}")
    with zipfile.ZipFile(
        destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6
    ) as archive:
        for source in files:
            if source.is_symlink():
                raise SystemExit(f"symlink in module package: {source}")
            if not source.is_file():
                continue
            relative = source.relative_to(package_dir).as_posix()
            info = zipfile.ZipInfo(relative, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, source.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=6)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    size = destination.stat().st_size
    if size == 0 or size > 64 * 1024 * 1024:
        raise SystemExit(f"module package ZIP is outside the install limit: {destination} ({size} bytes)")
    with zipfile.ZipFile(destination) as archive:
        if archive.testzip() is not None or not {"manifest.yaml", "module.wasm"}.issubset(archive.namelist()):
            raise SystemExit(f"module package ZIP failed validation: {destination}")
    return digest, size


def platform_id() -> str:
    machine = platform.machine().lower()
    arch = "x64" if machine in {"x86_64", "amd64"} else "arm64" if machine in {"aarch64", "arm64"} else machine
    os_name = {"win32": "windows", "linux": "linux", "darwin": "macos"}.get(sys.platform)
    if not os_name:
        raise SystemExit(f"unsupported release host: {sys.platform}")
    result = f"{os_name}-{arch}"
    if result not in PLATFORMS:
        raise SystemExit(f"unsupported release platform: {result}")
    return result


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    tag = os.environ.get("AOS_MODULE_PACK_RELEASE_TAG", "").strip()
    module_names = {
        value.strip()
        for value in os.environ.get("AOS_OPTIONAL_MODULES", "").split(",")
        if value.strip()
    }
    if not tag or not module_names:
        print("Optional module pack publication is disabled for this build.")
        return 0
    if not re.fullmatch(r"v[0-9A-Za-z._-]+", tag):
        raise SystemExit(f"invalid release tag for optional module packs: {tag}")
    if not module_names <= OPTIONAL_MODULES:
        raise SystemExit(f"unsupported optional module(s): {sorted(module_names - OPTIONAL_MODULES)}")

    platform_name = os.environ.get("AOS_MODULE_PACK_PLATFORM", platform_id())
    if platform_name != platform_id():
        raise SystemExit(f"module pack platform mismatch: {platform_name} vs {platform_id()}")
    repository = os.environ.get("GITHUB_REPOSITORY", "").strip()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise SystemExit("GITHUB_REPOSITORY is required when publishing optional module packs")

    catalogue_path = root / "share/modules/catalogue.yaml"
    catalogue = catalogue_path.read_text(encoding="utf-8")
    output_dir = root / "dist/module-packs"
    records: list[dict[str, object]] = []
    for module in sorted(module_names):
        package_dir = root / f"share/modules/{module}.aospkg"
        if not (package_dir / "manifest.yaml").is_file() or not (package_dir / "module.wasm").is_file():
            raise SystemExit(f"built module package is incomplete: {package_dir}")
        version, wasm_hash, min_api = module_manifest(package_dir, module)
        input_hash = subprocess.run(
            [sys.executable, str(root / "packaging/module_pack_inputs.py"), module],
            check=True,
            capture_output=True,
            text=True,
            cwd=root,
        ).stdout.strip()
        if not re.fullmatch(r"[0-9a-f]{64}", input_hash):
            raise SystemExit(f"invalid build input fingerprint for {module}")
        asset_name = f"aos-module-{module}-{platform_name}-{input_hash}.zip"
        archive_path = output_dir / asset_name
        archive_hash, archive_size = create_archive(package_dir, archive_path)
        asset_url = f"https://github.com/{repository}/releases/download/{tag}/{asset_name}"
        catalogue = replace_module_metadata(
            catalogue,
            module,
            version,
            wasm_hash,
            min_api,
            platform_name,
            asset_url,
            archive_hash,
            archive_size,
            input_hash,
        )
        records.append(
            {
                "module": module,
                "version": version,
                "platform": platform_name,
                "asset": asset_name,
                "url": asset_url,
                "size_bytes": archive_size,
                "sha256": archive_hash,
                "input_sha256": input_hash,
            }
        )

    catalogue_path.write_text(catalogue, encoding="utf-8", newline="\n")
    manifest_path = output_dir / "module-packs.json"
    manifest_path.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    lines = ["| Module | Platform | Pack ZIP | SHA-256 | Input fingerprint |", "|---|---|---:|---|---|"]
    lines.extend(
        f"| {r['module']} | {r['platform']} | {r['size_bytes']} bytes | `{r['sha256']}` | `{r['input_sha256']}` |"
        for r in records
    )
    summary = "\n".join(["## Optional module packs", "", *lines, ""])
    print(summary)
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with Path(summary_path).open("a", encoding="utf-8") as output:
            output.write(summary)
    return 0


if __name__ == "__main__":
    sys.exit(main())
