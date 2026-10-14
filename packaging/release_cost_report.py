#!/usr/bin/env python3
"""Summarize Preview release archive, component sizes, cache stats, and timings."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tempfile
import time
import zipfile
from pathlib import Path
from typing import Iterable


MODULES = ("notes", "tasks", "ext-rt", "canvas", "create", "illustration-studio")


def component_for(relative: str) -> str:
    path = relative.replace("\\", "/").lstrip("/")
    parts = path.split("/")
    while parts and parts[0].startswith("AgentOS-Preview-"):
        parts.pop(0)
    normalized = "/".join(parts)
    for module in MODULES:
        if normalized.startswith(f"share/modules/{module}.aospkg/"):
            return f"module:{module}"
    if normalized.startswith("share/assets/illustration/"):
        return "illustration-assets"
    if normalized.startswith("share/illustration-renderer-pack/"):
        return "illustration-renderer-pack"
    if normalized.startswith("bin/"):
        return "binaries-and-runtime"
    if normalized.startswith("share/skills/"):
        return "skills"
    if normalized.startswith("share/models/") or normalized.startswith("data/models/"):
        return "model-catalogue-and-small-assets"
    return "other"


def tree_files(root: Path) -> Iterable[tuple[str, Path, int]]:
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            yield path.relative_to(root).as_posix(), path, path.stat().st_size


def read_archive(path: Path) -> tuple[int, dict[str, int], dict[str, int], dict[str, int], list[str]]:
    raw_sizes: dict[str, int] = {}
    compressed_sizes: dict[str, int] = {}
    counts: dict[str, int] = {}
    gguf: list[str] = []
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            for info in archive.infolist():
                if info.is_dir():
                    continue
                key = component_for(info.filename)
                raw_sizes[key] = raw_sizes.get(key, 0) + info.file_size
                compressed_sizes[key] = compressed_sizes.get(key, 0) + info.compress_size
                counts[key] = counts.get(key, 0) + 1
                if info.filename.lower().endswith(".gguf"):
                    gguf.append(info.filename)
        return path.stat().st_size, raw_sizes, compressed_sizes, counts, gguf

    # tar.gz/zstd does not store compressed sizes per member. Preserve the exact
    # archive total and leave per-component compressed size absent in this case.
    import tarfile

    with tarfile.open(path, "r:*") as archive:
        for info in archive:
            if not info.isfile():
                continue
            key = component_for(info.name)
            raw_sizes[key] = raw_sizes.get(key, 0) + info.size
            counts[key] = counts.get(key, 0) + 1
            if info.name.lower().endswith(".gguf"):
                gguf.append(info.name)
    return path.stat().st_size, raw_sizes, compressed_sizes, counts, gguf


def isolated_zip_size(files: list[tuple[str, Path, int]]) -> int:
    """Measure a component as its own deflated ZIP, without retaining the file."""
    with tempfile.NamedTemporaryFile(prefix="aos-component-", suffix=".zip", delete=False) as temp:
        archive_path = Path(temp.name)
    try:
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for relative, source, _size in files:
                archive.write(source, relative)
        return archive_path.stat().st_size
    finally:
        archive_path.unlink(missing_ok=True)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_timings(path: Path | None) -> list[dict[str, object]]:
    if not path or not path.is_file():
        return []
    result = []
    for line in path.read_text(encoding="utf-8").splitlines():
        fields = line.split("\t")
        if len(fields) != 2:
            continue
        try:
            result.append({"phase": fields[0].lstrip("\ufeff"), "duration_ms": int(fields[1])})
        except ValueError:
            continue
    return result


def cache_summary(path: Path | None) -> list[str]:
    if not path or not path.is_file():
        return []
    wanted = re.compile(r"^(Compile requests|Cache hits(?: \([^)]+\))?|Cache misses(?: \([^)]+\))?|Cache hits rate|Cache misses rate|Cache writes|Cache errors)\s*[: ]")
    return [line.strip() for line in path.read_text(encoding="utf-8", errors="replace").splitlines() if wanted.match(line.strip())]


def human_bytes(value: int | None) -> str:
    if value is None:
        return "n/a"
    units = ("B", "KiB", "MiB", "GiB", "TiB")
    amount = float(value)
    for unit in units:
        if amount < 1024 or unit == units[-1]:
            return f"{amount:.1f} {unit}"
        amount /= 1024
    return f"{value} B"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--platform", required=True)
    parser.add_argument("--package-dir", required=True, type=Path)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--cache-profile", default="reuse")
    parser.add_argument("--timings", type=Path)
    parser.add_argument("--sccache-stats", type=Path)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--summary", type=Path)
    args = parser.parse_args()

    package_dir = args.package_dir.resolve()
    archive_path = args.archive.resolve()
    if not package_dir.is_dir() or not archive_path.is_file():
        parser.error("package directory and archive must exist")

    measurement_started = time.perf_counter()
    groups: dict[str, list[tuple[str, Path, int]]] = {}
    for relative, file_path, size in tree_files(package_dir):
        groups.setdefault(component_for(relative), []).append((relative, file_path, size))

    archive_bytes, archive_raw, archive_compressed, archive_counts, gguf = read_archive(archive_path)
    components = []
    for name in sorted(set(groups) | set(archive_raw)):
        files = groups.get(name, [])
        raw_bytes = archive_raw.get(name, sum(size for _relative, _path, size in files))
        components.append({
            "name": name,
            "files": archive_counts.get(name, len(files)),
            "uncompressed_bytes": raw_bytes,
            "standalone_zip_bytes": isolated_zip_size(files) if files else None,
            "archive_uncompressed_bytes": archive_raw.get(name, 0),
            "archive_compressed_bytes": archive_compressed.get(name),
        })
    components.sort(key=lambda item: int(item["uncompressed_bytes"]), reverse=True)
    measurement_overhead_ms = round((time.perf_counter() - measurement_started) * 1000)

    document = {
        "schema_version": 1,
        "platform": args.platform,
        "commit": os.environ.get("GITHUB_SHA", "unknown"),
        "run_id": os.environ.get("GITHUB_RUN_ID", "unknown"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "unknown"),
        "ref": os.environ.get("GITHUB_REF", "unknown"),
        "cache_profile_requested": args.cache_profile,
        "archive": {"path": archive_path.name, "bytes": archive_bytes, "sha256": sha256_file(archive_path)},
        "components": components,
        "component_measurement_overhead_ms": measurement_overhead_ms,
        "timings": parse_timings(args.timings),
        "sccache_summary": cache_summary(args.sccache_stats),
        "first_run_gguf": {"count": len(gguf), "paths": gguf},
        "compression_notes": (
            "archive_compressed_bytes is exact for ZIP releases. For tar.gz, per-component archive compressed sizes are unavailable; standalone_zip_bytes is a separate, reproducible comparison and is not additive to the release archive."
        ),
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

    lines = [
        f"## Release cost · {args.platform}",
        "",
        f"- Commit: `{document['commit']}` · run `{document['run_id']}` attempt `{document['run_attempt']}`",
        f"- Cache profile requested: **{args.cache_profile}**; actual cache hits below are the observed signal.",
        f"- Release archive: **{human_bytes(archive_bytes)}** ({archive_bytes:,} bytes), SHA-256 `{document['archive']['sha256']}`.",
        "",
        "| Component | Files | Uncompressed | Standalone ZIP comparison | Compressed in release archive |",
        "|---|---:|---:|---:|---:|",
    ]
    for item in components:
        lines.append(
            f"| `{item['name']}` | {item['files']} | {human_bytes(item['uncompressed_bytes'])} | {human_bytes(item['standalone_zip_bytes'])} | {human_bytes(item['archive_compressed_bytes'])} |"
        )
    lines.extend(["", "The standalone ZIP column measures each component in its own deflated ZIP; it is a comparison for future pack boundaries, not a sum of the release archive. For tar.gz, archive-level compressed size is not available per member.", ""])
    lines.append(f"Component measurement overhead: {measurement_overhead_ms / 1000:.1f} s (separate ZIP comparisons).")
    if gguf:
        lines.append(f"First-run GGUF models included: {len(gguf)} file(s).")
    else:
        lines.append("First-run GGUF models included: none; model downloads stay outside this archive.")
    timings = document["timings"]
    if timings:
        lines.extend(["", "| Build phase | Duration |", "|---|---:|"])
        for item in timings:
            lines.append(f"| `{item['phase']}` | {item['duration_ms'] / 1000:.1f} s |")
    if document["sccache_summary"]:
        lines.extend(["", "sccache:", "```text", *document["sccache_summary"], "```"])
    markdown = "\n".join(lines) + "\n"
    if args.summary:
        with args.summary.open("a", encoding="utf-8") as output:
            output.write(markdown)
    print(markdown, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
