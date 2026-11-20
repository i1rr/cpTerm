"""Fail closed for dependency forks, source changes and native advisories."""

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
        for name in ["Cargo.toml", "Cargo.lock", ".github/scripts/native_sources.json"]:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)

    def test_registry_and_native_sources_are_both_audited(self):
        queries = audit_lock.locked_queries(self.root)
        native = [query for _, query in queries if "commit" in query]
        self.assertEqual(len(native), 3)
        self.assertTrue(any(label.startswith("ssh2 ") for label, _ in queries))
        self.assertTrue(any(label.startswith("native LibreSSL ") for label, _ in queries))

    def test_unknown_path_dependencies_and_private_patches_are_rejected(self):
        lock = self.root / "Cargo.lock"
        original = lock.read_text()
        lock.write_text(original + '\n[[package]]\nname="fork"\nversion="1.0.0"\n')
        with self.assertRaisesRegex(ValueError, "non-registry"):
            audit_lock.locked_queries(self.root)
        lock.write_text(original)
        with (self.root / "Cargo.toml").open("a") as manifest:
            manifest.write('\n[patch.crates-io]\nssh2={path="vendor/ssh2"}\n')
        with self.assertRaisesRegex(ValueError, "Private dependency patches"):
            audit_lock.locked_queries(self.root)

    def test_chrono_and_old_openssl_bundle_cannot_return(self):
        lock = self.root / "Cargo.lock"
        original = lock.read_text()
        for name in ["chrono", "openssl-src"]:
            lock.write_text(original + f'\n[[package]]\nname="{name}"\nversion="1.0.0"\n')
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "must not be reintroduced"):
                audit_lock.locked_queries(self.root)

    def test_native_binding_updates_require_source_review(self):
        path = self.root / ".github/scripts/native_sources.json"
        original = path.read_text()
        for field in ["crate_version", "crate_checksum", "commit"]:
            sources = json.loads(original)
            sources[0][field] = "changed"
            path.write_text(json.dumps(sources))
            with self.subTest(field=field), self.assertRaises(ValueError):
                audit_lock.locked_queries(self.root)
        path.write_text("[]")
        with self.assertRaisesRegex(ValueError, "native source inventory"):
            audit_lock.locked_queries(self.root)

    def test_every_advisory_blocks_including_native_and_maintenance_notices(self):
        for label, query in [("registry", {"package": {"name": "chrono", "ecosystem": "crates.io"}, "version": "1.0.0"}),
                             ("native", {"commit": "a" * 40})]:
            response = {"results": [{"vulns": [{"id": "TEST-ADVISORY"}]}]}
            with self.subTest(label=label), patch.object(audit_lock, "locked_queries", return_value=[(label, query)]), \
                    patch.object(audit_lock, "urlopen", return_value=io.BytesIO(json.dumps(response).encode())), \
                    contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(audit_lock.main(), 1)

    def test_lookup_failure_cannot_silently_allow_a_release(self):
        with patch.object(audit_lock, "locked_queries", return_value=[("test", {})]), \
                patch.object(audit_lock, "urlopen", side_effect=TimeoutError):
            with self.assertRaises(TimeoutError):
                audit_lock.main()


if __name__ == "__main__":
    unittest.main()
