"""Advisories remain blocking, including for patched dependencies."""

import contextlib
import io
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import audit_lock

ROOT = Path(__file__).resolve().parents[2]


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.storage = tempfile.TemporaryDirectory()
        self.addCleanup(self.storage.cleanup)
        self.root = Path(self.storage.name)
        for name in ["Cargo.toml", "Cargo.lock"]:
            shutil.copyfile(ROOT / name, self.root / name)
        shutil.copytree(ROOT / "vendor", self.root / "vendor")

    def test_patched_packages_are_audited_under_original_identity(self):
        identities = {(q["package"]["name"], q["version"])
                      for q in audit_lock.locked_queries(self.root)}
        self.assertIn(("russh-sftp", "2.4.0"), identities)
        self.assertIn(("russh-util", "0.52.0"), identities)
        self.assertFalse(any(name == "chrono" for name, _ in identities))

    def test_unknown_path_dependency_is_rejected(self):
        with (self.root / "Cargo.lock").open("a") as lock:
            lock.write('\n[[package]]\nname = "unexpected"\nversion = "1.0.0"\n')
        with self.assertRaisesRegex(ValueError, "non-registry source"):
            audit_lock.locked_queries(self.root)

    def test_chrono_cannot_be_reintroduced(self):
        with (self.root / "Cargo.lock").open("a") as lock:
            lock.write('\n[[package]]\nname = "chrono"\nversion = "0.4.45"\n')
        with self.assertRaisesRegex(ValueError, "chrono must not"):
            audit_lock.locked_queries(self.root)

    def test_modified_missing_and_added_vendor_files_are_rejected(self):
        directory = self.root / "vendor/russh-util-0.52.0"
        source = directory / "src/time.rs"
        original = source.read_bytes()
        for kind in ["modified", "missing", "added"]:
            with self.subTest(kind=kind):
                source.write_bytes(original)
                extra = directory / "extra.rs"
                if kind == "modified":
                    source.write_text("changed")
                elif kind == "missing":
                    source.unlink()
                else:
                    extra.write_text("added")
                with self.assertRaisesRegex(ValueError, "provenance review"):
                    audit_lock.locked_queries(self.root)
                if extra.exists():
                    extra.unlink()

    def test_patch_redirection_is_rejected(self):
        manifest = self.root / "Cargo.toml"
        manifest.write_text(manifest.read_text().replace("vendor/russh-util-0.52.0", "elsewhere"))
        with self.assertRaisesRegex(ValueError, "Unexpected patch source"):
            audit_lock.locked_queries(self.root)

    def test_every_advisory_blocks_including_previous_maintenance_notice(self):
        for name, advisory in [("chrono", "RUSTSEC-2026-0543"),
                               ("russh-sftp", "RUSTSEC-2099-0001")]:
            query = {"package": {"ecosystem": "crates.io", "name": name}, "version": "1.0.0"}
            response = {"results": [{"vulns": [{"id": advisory}]}]}
            with self.subTest(name=name), patch.object(audit_lock, "locked_queries", return_value=[query]), \
                    patch.object(audit_lock, "urlopen", return_value=io.BytesIO(json.dumps(response).encode())), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(audit_lock.main(), 1)

    def test_lookup_failure_cannot_silently_allow_a_release(self):
        with patch.object(audit_lock, "locked_queries", return_value=[{}]), \
                patch.object(audit_lock, "urlopen", side_effect=TimeoutError):
            with self.assertRaises(TimeoutError):
                audit_lock.main()


if __name__ == "__main__":
    unittest.main()
