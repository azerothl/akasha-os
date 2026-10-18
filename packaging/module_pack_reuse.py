#!/usr/bin/env python3
"""Restore exact WASM packs from a previously published, signed release index.

Modes: off (no network), prefer (build changed/missing inputs), require (fail on
any miss). A matching but corrupt/unavailable artifact is always fatal. Preview
catalogue keys have the same development trust scope as the existing installer.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import stat
import subprocess
import tempfile
import time
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

from module_pack_inputs import MODULES, fingerprint
from module_pack_release import catalogue_block, module_manifest, platform_id, replace_module_metadata

MAX_ARCHIVE_BYTES = 64 * 1024 * 1024


def read_url(url: str, limit: int) -> bytes:
    if not url.startswith("https://"):
        raise ValueError("release asset URL must use HTTPS")
    # No GitHub token is forwarded to asset redirects or storage hosts.
    with urllib.request.urlopen(url, timeout=90) as response:
        if not response.url.startswith("https://"):
            raise ValueError("asset redirected to an insecure URL")
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("release asset exceeds its size limit")
    return data


def verify_index(data: bytes, signature: bytes, pinned_key: bytes) -> str:
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    canonical = data.replace(b"\r\n", b"\n").replace(b"\r", b"\n")
    Ed25519PublicKey.from_public_bytes(bytes.fromhex(pinned_key.decode().strip())).verify(
        bytes.fromhex(signature.decode().strip()), canonical
    )
    return canonical.decode("utf-8")


def metadata(index: str, module: str) -> dict[str, str] | None:
    try:
        _, _, block = catalogue_block(index, module)
    except SystemExit:
        return None
    result = {}
    for key in ("version", "hash", "minimum_host_api", "url", "sha256", "size_bytes", "input_sha256"):
        match = re.search(rf"(?m)^\s{{4,6}}{key}:\s*['\"]?([^\r\n'\"]+)['\"]?\s*$", block)
        if not match:
            return None
        result[key] = match.group(1).strip()
    result["block"] = block
    return result


def extract_verified(data: bytes, meta: dict[str, str], destination: Path, module: str) -> None:
    digest = meta["sha256"].removeprefix("sha256:")
    if len(data) != int(meta["size_bytes"]) or len(data) > MAX_ARCHIVE_BYTES:
        raise ValueError("module archive size mismatch")
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("module archive SHA-256 mismatch")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        infos = archive.infolist()
        if not {"manifest.yaml", "module.wasm"}.issubset(archive.namelist()):
            raise ValueError("module archive is missing its manifest or WASM")
        if len(infos) > 4096 or sum(item.file_size for item in infos) > 128 * 1024 * 1024:
            raise ValueError("module archive extraction limits exceeded")
        seen = set()
        for item in infos:
            # ZipInfo.filename can normalize backslashes on Windows. Validate
            # orig_filename so a malformed archive is refused on every host.
            raw = item.orig_filename
            path = PurePosixPath(raw)
            if (not raw or path.is_absolute() or "\\" in raw or ":" in raw
                    or any(part in {"", ".", ".."} for part in raw.rstrip("/").split("/"))
                    or any(part.endswith((".", " ")) or any(ord(c) < 32 for c in part)
                        or re.fullmatch(r"(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])", part.split(".")[0])
                        for part in path.parts)
                    or stat.S_ISLNK(item.external_attr >> 16)):
                raise ValueError(f"unsafe module archive member: {raw}")
            key = raw.rstrip("/").casefold()
            if key in seen:
                raise ValueError("duplicate module archive member")
            seen.add(key)
            target = destination.joinpath(*path.parts)
            if item.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(item) as source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
    version, wasm_hash, min_api = module_manifest(destination, module)
    if (version != meta["version"] or wasm_hash != meta["hash"].removeprefix("sha256:")
            or min_api != int(meta["minimum_host_api"])):
        raise ValueError("module manifest does not match signed catalogue")
    if module in {"create", "illustration-studio"} and not (destination / "ui/index.json").is_file():
        raise ValueError("module UI is missing")


def restore(root: Path, module: str, input_hash: str, index: str, data: bytes, platform: str) -> bool:
    meta = metadata(index, module)
    if meta is None or meta["input_sha256"].removeprefix("sha256:") != input_hash:
        return False
    if not re.search(r"(?m)^    dependencies:\s*\[\s*\]\s*$", meta["block"]):
        raise ValueError("reusable official packs must declare an empty dependency list; dependency resolution is not implicit")
    if not re.search(rf"(?m)^      - {re.escape(platform)}\s*$", meta["block"]):
        raise ValueError("matching module pack has incompatible platform")
    # The fingerprint already binds the current host API, compiler and sources.
    api_text = (root / "crates/aos-proto/src/lib.rs").read_text(encoding="utf-8")
    current_api = int(re.search(r"pub const OS_API_VERSION:\s*u32\s*=\s*(\d+)", api_text).group(1))
    if int(meta["minimum_host_api"]) > current_api:
        raise ValueError("matching module pack requires a newer host API")
    parent = (root / "share/modules").resolve()
    target = parent / f"{module}.aospkg"
    if target.is_symlink() or target.resolve().parent != parent:
        raise ValueError("module restore destination escapes the package directory")
    with tempfile.TemporaryDirectory(prefix=f".reuse-{module}-", dir=parent) as temp:
        staging = Path(temp) / "new"
        staging.mkdir()
        extract_verified(data, meta, staging, module)
        previous = Path(temp) / "previous"
        if target.exists():
            target.rename(previous)
        try:
            staging.rename(target)
        except BaseException:
            if previous.exists():
                previous.rename(target)
            raise
    archives = root / "dist/module-packs"
    archives.mkdir(parents=True, exist_ok=True)
    asset_name = f"aos-module-{module}-{platform}-{input_hash}.zip"
    (archives / asset_name).write_bytes(data)
    catalogue_path = parent / "catalogue.yaml"
    catalogue = catalogue_path.read_text(encoding="utf-8")
    start, end, block = catalogue_block(catalogue, module)
    block = re.sub(r"(?m)^    version:.*$", f'    version: "{meta["version"]}"', block)
    block = re.sub(r"(?m)^    hash:.*$", f'    hash: {meta["hash"]}', block)
    catalogue = catalogue[:start] + block + catalogue[end:]
    catalogue = replace_module_metadata(catalogue, module, meta["version"], meta["hash"].removeprefix("sha256:"),
        int(meta["minimum_host_api"]), platform, meta["url"], meta["sha256"].removeprefix("sha256:"),
        int(meta["size_bytes"]), input_hash)
    catalogue_path.write_text(catalogue, encoding="utf-8", newline="\n")
    return True


def release_indexes(root: Path, platform: str, repository: str) -> list[tuple[str, str]]:
    name = f"aos-module-catalogue-{platform}.yaml"
    response = subprocess.run(["gh", "api", f"repos/{repository}/releases?per_page=30"],
        check=True, capture_output=True, text=True)
    pinned = (root / "share/modules/catalogue.pub").read_bytes()
    indexes = []
    for release in json.loads(response.stdout):
        if release.get("draft"):
            continue
        assets = {asset["name"]: asset["browser_download_url"] for asset in release.get("assets", [])}
        if name not in assets:
            continue
        if f"{name}.sig" not in assets:
            raise ValueError(f"published module index has no signature: {release['tag_name']}")
        index = verify_index(read_url(assets[name], 512 * 1024),
            read_url(assets[f"{name}.sig"], 1024), pinned)
        indexes.append((release["tag_name"], index))
    return indexes


def prepare(root: Path, mode: str) -> list[dict]:
    output = root / "dist/module-reuse"
    output.mkdir(parents=True, exist_ok=True)
    restored_path = output / "restored.txt"
    restored_path.write_text("", encoding="utf-8")
    repository = os.environ.get("GITHUB_REPOSITORY", "azerothl/akasha-os")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("invalid GitHub repository")
    platform = platform_id()
    indexes = release_indexes(root, platform, repository) if mode != "off" else []
    records = []
    restored = []
    for module in sorted(MODULES):
        started = time.monotonic()
        value = fingerprint(root, module)
        record = {"module":module,"input_sha256":value,"mode":mode,"platform":platform,"outcome":"build"}
        for tag, index in indexes:
            meta = metadata(index, module)
            if meta is None or meta["input_sha256"].removeprefix("sha256:") != value:
                continue
            if not re.fullmatch(rf"https://github.com/{re.escape(repository)}/releases/download/[^/]+/aos-module-[A-Za-z0-9_.-]+\.zip", meta["url"]):
                raise ValueError("signed module archive URL is not an immutable repository release asset")
            data = read_url(meta["url"], MAX_ARCHIVE_BYTES)
            if restore(root, module, value, index, data, platform):
                record.update(outcome="restored", source_release=tag, archive_sha256=meta["sha256"])
                restored.append(module)
                break
        if mode == "require" and record["outcome"] != "restored":
            raise ValueError(f"no signed compatible pack for {module}, input {value}")
        record["seconds"] = round(time.monotonic() - started, 6)
        records.append(record)
        print(json.dumps(record), flush=True)
    (output / "report.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    restored_path.write_text("".join(module + "\n" for module in restored), encoding="utf-8")
    return records


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--mode", choices=["off", "prefer", "require"],
        default=os.environ.get("AOS_MODULE_REUSE_MODE", "off"))
    options = parser.parse_args()
    prepare(options.root.resolve(), options.mode)
