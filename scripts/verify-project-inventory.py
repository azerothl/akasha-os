"""Inspect a reproducible snapshot of an actual Git repository through the native host.

The repository is never edited. A new temporary corpus and host data directory
are retained for evidence. Archive paths and links are validated before extraction.
"""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument(
        "--page-entries", type=int, choices=range(1, 5001), metavar="1..5000"
    )
    args = parser.parse_args()
    if (
        not args.repo.is_absolute()
        or not args.repo.is_dir()
        or not args.probe.is_absolute()
        or not args.probe.is_file()
    ):
        parser.error("--repo and --probe must be existing absolute paths")
    revision = subprocess.check_output(
        [
            "git",
            "-C",
            str(args.repo),
            "rev-parse",
            "--verify",
            "--end-of-options",
            args.revision + "^{commit}",
        ],
        text=True,
    ).strip()
    assert re.fullmatch(r"[0-9a-f]{40,64}", revision)
    fixture = Path(tempfile.mkdtemp(prefix="aos-project-corpus-"))
    corpus = fixture / "repository"
    corpus.mkdir()
    archive = fixture / "source.zip"
    subprocess.run(
        [
            "git",
            "-C",
            str(args.repo),
            "archive",
            "--format=zip",
            "--output",
            str(archive),
            revision,
        ],
        check=True,
        timeout=30,
    )
    files = 0
    with zipfile.ZipFile(archive) as source:
        for entry in source.infolist():
            path = PurePosixPath(entry.filename)
            assert (
                not path.is_absolute()
                and ".." not in path.parts
                and "\\" not in entry.filename
            )
            assert not stat.S_ISLNK(
                entry.external_attr >> 16
            ), "linked archive entries are refused"
            assert (corpus / entry.filename).resolve().is_relative_to(corpus.resolve())
            source.extract(entry, corpus)
            files += not entry.is_dir()
    completed = subprocess.run(
        [str(args.probe), str(corpus), str(fixture / "host")]
        + ([str(args.page_entries)] if args.page_entries else []),
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=150,
    )
    (fixture / "probe.log").write_text(
        completed.stdout + completed.stderr, encoding="utf-8"
    )
    assert completed.returncode == 0, f"native inventory failed; see {fixture}"
    native = json.loads(completed.stdout.splitlines()[-1])
    report = {
        "ok": bool(native["ok"] and native["scan"]["complete"]),
        "fixture": str(fixture),
        "source_revision": revision,
        "page_entries": args.page_entries,
        "archive_files": files,
        "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
        "probe_sha256": hashlib.sha256(args.probe.read_bytes()).hexdigest(),
        "native": native,
        "scope": "read-only native inspection of a real repository's committed snapshot; no generated source corpus or semantic retrieval benchmark",
    }
    (fixture / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report))
    assert report[
        "ok"
    ], f"inventory was partial; machine-readable report retained at {fixture}"


if __name__ == "__main__":
    main()
