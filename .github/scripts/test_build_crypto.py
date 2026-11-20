"""Reject unverified archives and extraction outside the native build directory."""

import hashlib
import io
import json
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import build_crypto


class NativeBuildTests(unittest.TestCase):
    def setUp(self):
        self.storage = tempfile.TemporaryDirectory()
        self.addCleanup(self.storage.cleanup)
        self.root = Path(self.storage.name)
        (self.root / ".github/scripts").mkdir(parents=True)

    def archive_spec(self, data, checksum=None):
        spec = {"name": "LibreSSL", "version": "4.4.1", "archive": "https://example.test/source.tar.gz",
                "archive_sha256": checksum or hashlib.sha256(data).hexdigest()}
        (self.root / ".github/scripts/native_sources.json").write_text(json.dumps([spec]))

    def test_changed_archive_cannot_reach_extraction_or_compilation(self):
        data = b"substituted download"
        self.archive_spec(data, "0" * 64)
        with patch.object(build_crypto, "ROOT", self.root), \
                patch.object(build_crypto, "urlopen", return_value=io.BytesIO(data)), \
                patch.object(build_crypto.tarfile, "open") as extract, \
                patch.object(build_crypto.subprocess, "run") as compile:
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                build_crypto.build(self.root / "native/sdk", "cmake")
        extract.assert_not_called()
        compile.assert_not_called()

    def test_verified_archive_still_cannot_escape_build_directory(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            member = tarfile.TarInfo("../../escape")
            member.size = 4
            archive.addfile(member, io.BytesIO(b"data"))
        data = buffer.getvalue()
        self.archive_spec(data)
        with patch.object(build_crypto, "ROOT", self.root), \
                patch.object(build_crypto, "urlopen", return_value=io.BytesIO(data)), \
                patch.object(build_crypto.subprocess, "run") as compile:
            with self.assertRaises(tarfile.OutsideDestinationError):
                build_crypto.build(self.root / "native/sdk", "cmake")
        self.assertFalse((self.root / "escape").exists())
        compile.assert_not_called()


if __name__ == "__main__":
    unittest.main()
