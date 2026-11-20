"""Privileged fixture cleanup must never accept a parent or nested directory."""

import os
import shutil
import subprocess
import unittest
from pathlib import Path


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "requires native Bash")
class FixtureCleanupTests(unittest.TestCase):
    def test_cleanup_guard_accepts_only_the_mktemp_directory(self):
        script = Path(__file__).with_name("test-sftp.sh").read_text(encoding="utf-8")
        guard = next(line.strip() for line in script.splitlines()
                     if line.strip().startswith('if [[ "$fixture"'))
        # Exercise the real guard without running cleanup or substituting a path
        # into shell source. Every candidate is supplied as a positional argument.
        command = 'fixture="$1"\n' + guard + '\nexit 0\nelse\nexit 1\nfi\n'
        for candidate, expected in [
            ("/tmp/cpt-sftp.Ab12Cd34", 0),
            ("/", 1), ("/tmp", 1), ("/tmp/", 1),
            ("/tmp/cpt-sftp./..", 1),
            ("/tmp/cpt-sftp.Ab12Cd34/child", 1),
            ("/tmp/cpt-sftp.Ab12Cd34/../sibling", 1),
            ("/tmp/cpt-sftp.Ab12Cd34/", 1),
        ]:
            with self.subTest(path=candidate):
                result = subprocess.run(["bash", "-c", command, "fixture-test", candidate],
                                        capture_output=True, check=False)
                self.assertEqual(result.returncode, expected, result.stderr)


if __name__ == "__main__":
    unittest.main()
