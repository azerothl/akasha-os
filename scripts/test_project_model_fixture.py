"""Checks for the generated-code validator, without loading a model."""

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "model_fixture", Path(__file__).with_name("verify-project-model.py")
)
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)


class ModelFixtureTests(unittest.TestCase):
    def run_fixture(self, front, back, require=False):
        with tempfile.TemporaryDirectory(prefix="aos-validator-test-") as directory:
            root = Path(directory)
            self.assertEqual(
                root.resolve().parent, Path(tempfile.gettempdir()).resolve()
            )
            (root / "verify.py").write_text(fixture_module.VALIDATOR, encoding="utf-8")
            for part, source in [("frontend", front), ("backend", back)]:
                folder = root / "project" / part
                folder.mkdir(parents=True)
                (folder / "module.py").write_text(source, encoding="utf-8")
            args = [sys.executable, "-I", "-S", "-B", str(root / "verify.py")]
            if require:
                args.append("--require-refactor")
            result = subprocess.run(
                args,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=5,
            )
            self.assertFalse((root / "unexpected-write").exists())
            return result

    def test_baseline_passes_behaviour_cases(self):
        result = self.run_fixture(fixture_module.FRONT, fixture_module.BACK)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"cases": 93', result.stdout)

    def test_duplicate_replacements_are_rejected(self):
        result = self.run_fixture(
            fixture_module.FRONT, fixture_module.BACK + fixture_module.BACK
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("duplicate function", result.stderr)

    def test_unused_helpers_do_not_count_as_refactoring(self):
        result = self.run_fixture(
            fixture_module.FRONT + "\ndef unused_front(x):\n    return x\n",
            fixture_module.BACK + "\ndef unused_back(x):\n    return x\n",
            True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("shared extracted helper", result.stderr)

    def test_imports_and_io_are_rejected_before_execution(self):
        for source in [
            "import os\n" + fixture_module.FRONT,
            fixture_module.FRONT
            + "\ndef write_file():\n    return open('unexpected-write','w')\n",
        ]:
            with self.subTest(source=source[:20]):
                result = self.run_fixture(source, fixture_module.BACK)
                self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
