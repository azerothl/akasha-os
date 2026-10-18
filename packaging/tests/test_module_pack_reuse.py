import hashlib
import io
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import module_pack_inputs as inputs
import module_pack_reuse as reuse
import module_pack_release as release
from module_pack_release import create_archive
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat


class ModuleReuseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        for path, content in {
            "crates/aos-proto/src/lib.rs": "pub const OS_API_VERSION: u32 = 1;\n",
            "modules/notes/src/lib.rs": "pub fn run() {}\n",
            "modules/sdk/src/lib.rs": "pub fn api() {}\n",
            "Cargo.lock": "# fixture lock\n",
        }.items():
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
        package = self.root / "built"
        package.mkdir()
        self.wasm = b"\x00asm\x01\x00\x00\x00"
        self.wasm_hash = hashlib.sha256(self.wasm).hexdigest()
        (package / "module.wasm").write_bytes(self.wasm)
        (package / "manifest.yaml").write_text(
            f"name: notes\nversion: 1.0.0\nhash: {self.wasm_hash}\nmin_os_api: 1\n"
        )
        archive = self.root / "pack.zip"
        digest, size = create_archive(package, archive)
        self.archive = archive.read_bytes()
        self.input_hash = "a" * 64
        self.index = (
            'version: 1\nentries:\n  - name: notes\n    version: "1.0.0"\n'
            f"    kind: module\n    hash: sha256:{self.wasm_hash}\n    minimum_host_api: 1\n"
            f"    platforms:\n      - windows-x64\n    dependencies: []\n    package:\n"
            f"      url: https://github.com/azerothl/akasha-os/releases/download/v1/aos-module-notes-windows-x64-{self.input_hash}.zip\n"
            f"      sha256: sha256:{digest}\n      size_bytes: {size}\n      input_sha256: sha256:{self.input_hash}\n"
        )
        shared = self.root / "share/modules"
        shared.mkdir(parents=True)
        (shared / "catalogue.yaml").write_text(self.index)
        self.key = Ed25519PrivateKey.generate()
        self.public = self.key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw).hex().encode()

    def test_signature_is_checked_against_pinned_key_and_canonical_bytes(self):
        raw = self.index.encode()
        signature = self.key.sign(raw).hex().encode()
        self.assertEqual(reuse.verify_index(raw.replace(b"\n", b"\r\n"), signature, self.public), self.index)
        for altered in [raw + b"# altered", raw.replace(b"1.0.0", b"9.0.0")]:
            with self.assertRaises(Exception):
                reuse.verify_index(altered, signature, self.public)

    def test_restore_verifies_and_preserves_the_exact_archive(self):
        self.assertTrue(reuse.restore(self.root, "notes", self.input_hash, self.index, self.archive, "windows-x64"))
        self.assertEqual((self.root / "share/modules/notes.aospkg/module.wasm").read_bytes(), self.wasm)
        self.assertEqual(next((self.root / "dist/module-packs").glob("*.zip")).read_bytes(), self.archive)

    def test_publication_preserves_restored_bytes_and_refreshes_catalogue_url(self):
        reuse.restore(self.root, "notes", self.input_hash, self.index, self.archive, "windows-x64")
        report = self.root / "dist/module-reuse/report.json"
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps([{"module":"notes", "outcome":"restored",
            "input_sha256":self.input_hash,"archive_sha256":hashlib.sha256(self.archive).hexdigest()}]))
        from types import SimpleNamespace
        with patch.object(release, "__file__", str(self.root / "packaging/module_pack_release.py")), \
                patch.object(release, "MODULES", {"notes"}), \
                patch.object(release, "platform_id", return_value="windows-x64"), \
                patch.object(release.subprocess, "run", return_value=SimpleNamespace(stdout=self.input_hash+"\n")), \
                patch.object(release, "create_archive", side_effect=AssertionError("must not recompress")), \
                patch.dict("os.environ", {"AOS_MODULE_PACK_RELEASE_TAG":"v2", "AOS_OPTIONAL_MODULES":"notes",
                    "AOS_MODULE_PACK_PLATFORM":"windows-x64", "GITHUB_REPOSITORY":"azerothl/akasha-os",
                    "GITHUB_STEP_SUMMARY":""}):
            self.assertEqual(release.main(), 0)
        self.assertEqual(next((self.root / "dist/module-packs").glob("*.zip")).read_bytes(), self.archive)
        meta = reuse.metadata((self.root / "share/modules/catalogue.yaml").read_text(), "notes")
        self.assertIn("/download/v2/", meta["url"])
        self.assertEqual(meta["sha256"].removeprefix("sha256:"), hashlib.sha256(self.archive).hexdigest())

    def test_changed_inputs_are_a_miss_and_required_mode_fails(self):
        self.assertFalse(reuse.restore(self.root, "notes", "b" * 64, self.index, self.archive, "windows-x64"))
        with patch.object(reuse, "release_indexes", return_value=[]), patch.object(reuse, "fingerprint", return_value="b" * 64):
            with self.assertRaisesRegex(ValueError, "no signed compatible pack"):
                reuse.prepare(self.root, "require")

    def test_corrupt_zip_does_not_replace_the_previous_install(self):
        target = self.root / "share/modules/notes.aospkg"
        target.mkdir()
        (target / "old").write_text("retained")
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            reuse.restore(self.root, "notes", self.input_hash, self.index, b"x" * len(self.archive), "windows-x64")
        self.assertEqual((target / "old").read_text(), "retained")

    def test_archive_traversal_and_duplicate_paths_are_refused(self):
        for names in [["../escape"], ["module.wasm", "MODULE.WASM"], ["C:/escape"], ["dir\\escape"], ["module.wasm."], ["NUL.txt"], ["dir /file"]]:
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w") as archive:
                archive.writestr("manifest.yaml", (self.root / "built/manifest.yaml").read_bytes())
                if "module.wasm" not in names:
                    archive.writestr("module.wasm", self.wasm)
                for name in names:
                    info = zipfile.ZipInfo()
                    info.filename = name
                    archive.writestr(info, "bad")
            data = buffer.getvalue()
            meta = reuse.metadata(self.index, "notes")
            meta["sha256"] = hashlib.sha256(data).hexdigest()
            meta["size_bytes"] = str(len(data))
            dest = self.root / "extract"
            dest.mkdir(exist_ok=True)
            with self.assertRaises(ValueError):
                reuse.extract_verified(data, meta, dest, "notes")
        self.assertFalse((self.root / "escape").exists())

    def test_source_sdk_and_api_invalidate_but_generated_wasm_does_not(self):
        with patch.object(inputs, "toolchain_fingerprint", return_value=b"compiler-and-api"):
            before = inputs.fingerprint(self.root, "notes")
            generated = self.root / "share/modules/notes.aospkg"
            generated.mkdir()
            (generated / "module.wasm").write_bytes(b"generated output")
            (generated / "manifest.yaml").write_text("hash: new-generated-hash\n")
            self.assertEqual(inputs.fingerprint(self.root, "notes"), before)
            source = self.root / "modules/notes/src/lib.rs"
            source.write_text("pub fn changed() {}\n")
            self.assertNotEqual(inputs.fingerprint(self.root, "notes"), before)
            changed = inputs.fingerprint(self.root, "notes")
            (self.root / "modules/sdk/src/lib.rs").write_text("pub fn api_v2() {}\n")
            self.assertNotEqual(inputs.fingerprint(self.root, "notes"), changed)
        with patch.object(inputs, "toolchain_fingerprint", return_value=b"new-api"):
            self.assertNotEqual(inputs.fingerprint(self.root, "notes"), before)

    def test_prepare_records_real_restore_and_build_decisions(self):
        with patch.object(reuse, "release_indexes", return_value=[("v1", self.index)]), \
             patch.object(reuse, "platform_id", return_value="windows-x64"), \
             patch.object(reuse, "fingerprint", return_value=self.input_hash), \
             patch.object(reuse, "read_url", return_value=self.archive):
            records = reuse.prepare(self.root, "prefer")
        self.assertEqual([r["module"] for r in records if r["outcome"] == "restored"], ["notes"])
        self.assertEqual((self.root / "dist/module-reuse/restored.txt").read_text(), "notes\n")
        self.assertEqual(len(json.loads((self.root / "dist/module-reuse/report.json").read_text())), 6)

    def test_independent_module_uses_its_own_lock_not_host_release_versions(self):
        with patch.object(inputs, "toolchain_fingerprint", return_value=b"same-compiler-api"):
            original = inputs.fingerprint(self.root, "notes")
            (self.root / "Cargo.lock").write_text("host version changed\n")
            self.assertEqual(inputs.fingerprint(self.root, "notes"), original)
            own_lock = self.root / "modules/notes/Cargo.lock"
            own_lock.write_text("guest dependency version one\n")
            with_lock = inputs.fingerprint(self.root, "notes")
            own_lock.write_text("guest dependency version two\n")
            self.assertNotEqual(inputs.fingerprint(self.root, "notes"), with_lock)


if __name__ == "__main__":
    unittest.main()
