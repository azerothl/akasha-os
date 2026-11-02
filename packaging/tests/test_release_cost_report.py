"""An actual CLI report remains exactly recoverable when artifacts are gone."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile


class ReleaseReportRecoveryTests(unittest.TestCase):
    def test_console_json_preserves_exact_archive_and_component_measurements(self):
        script = Path(__file__).resolve().parents[1] / "release_cost_report.py"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "package"
            binary = package / "bin" / "probe"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"synthetic binary" * 37)
            archive = root / "preview.zip"
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
                output.write(binary, "bin/probe")
            report_path = root / "report.json"
            summary_path = root / "summary.md"
            environment = os.environ | {"GITHUB_SHA": "abc123", "GITHUB_RUN_ID": "42"}
            completed = subprocess.run(
                [sys.executable, str(script), "--platform", "test",
                 "--package-dir", str(package), "--archive", str(archive),
                 "--report", str(report_path), "--summary", str(summary_path)],
                env=environment, capture_output=True, text=True, check=True,
            )
            payload = completed.stdout.split("AOS_RELEASE_COST_JSON_BEGIN\n", 1)[1]
            recovered = json.loads(payload.split("\nAOS_RELEASE_COST_JSON_END", 1)[0])
            self.assertEqual(recovered, json.loads(report_path.read_text(encoding="utf-8")))
            self.assertEqual(recovered["archive"]["bytes"], archive.stat().st_size)
            self.assertEqual(recovered["components"][0]["uncompressed_bytes"], binary.stat().st_size)
            self.assertEqual(recovered["commit"], "abc123")
            # The job summary stays readable, rather than embedding the raw block.
            self.assertNotIn("AOS_RELEASE_COST_JSON_BEGIN", summary_path.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
