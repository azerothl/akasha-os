"""Real worker edit/test acceptance over a newly generated, pure-Python fixture.

Automated fixture approvals do not validate the native human confirmation UI.
"""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["probe", "worker", "modeld", "model"]:
        parser.add_argument(f"--{name}", required=True, type=Path)
    args = parser.parse_args()
    paths = [args.probe, args.worker, args.modeld, args.model]
    if any(not path.is_absolute() or not path.is_file() for path in paths):
        parser.error("all paths must be existing absolute files")
    sources = runpy.run_path(str(Path(__file__).with_name("verify-project-model.py")))
    fixture = Path(tempfile.mkdtemp(prefix="aos-project-agent-"))
    (fixture / ".akasha-os-model-probe").write_text("generated acceptance fixture only", encoding="utf-8")
    (fixture / "verify.py").write_bytes(sources["VALIDATOR"].encode("utf-8"))
    for part, source in [("frontend", sources["FRONT"]), ("backend", sources["BACK"])]:
        folder = fixture / "project" / part
        folder.mkdir(parents=True)
        (folder / "module.py").write_bytes(source.encode("utf-8"))
    print(f"fixture: {fixture}", flush=True)
    result = subprocess.run(
        [str(args.probe), str(args.worker), str(args.modeld), str(args.model), str(fixture), sys.executable],
        # Leave room for GGUF hashing, model startup and the probe's own
        # 870-second worker deadline and shutdown before this outer limit.
        capture_output=True, timeout=1500,
    )
    (fixture / "probe.log").write_bytes(result.stdout + result.stderr)
    report_path = fixture / "control" / "report.json"
    if report_path.is_file():
        print(json.dumps(json.loads(report_path.read_text(encoding="utf-8")), ensure_ascii=True))
    else:
        print(result.stderr.decode("utf-8", errors="replace"), file=sys.stderr)
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
