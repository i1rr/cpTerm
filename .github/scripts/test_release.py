"""Release regressions use disposable files and mocked publication, never GitHub writes."""

import hashlib
import json
import os
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError

import release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory()
        self.addCleanup(self.work.cleanup)
        previous = Path.cwd()
        os.chdir(self.work.name)
        self.addCleanup(os.chdir, previous)
        self.env = patch.dict(os.environ, {
            "GITHUB_SHA": "a" * 40, "GITHUB_RUN_ID": "123", "GH_REPO": "test/repo",
            "GH_TOKEN": "test-token",
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        Path("Cargo.toml").write_text('[package]\nversion = "1.2.3"\n', encoding="utf-8")
        Path("README.md").write_text("Test instructions", encoding="utf-8")
        Path("vendor/example").mkdir(parents=True)
        Path("vendor/example/LICENSE").write_text("Test license", encoding="utf-8")
        Path("vendor/provenance.json").write_text(json.dumps({
            "example": {"version": "1.0.0", "path": "vendor/example", "archive": "https://example.test"}
        }), encoding="utf-8")

    def archives(self):
        Path("dist").mkdir()
        for name in release.TARGETS.values():
            Path("dist", name).write_bytes(name.encode())

    def info(self, *, draft=False, sha=None):
        return {"target_commitish": sha or os.environ["GITHUB_SHA"], "draft": draft}

    def test_packages_all_targets_with_executable_mode_and_source_metadata(self):
        for target, name in release.TARGETS.items():
            with self.subTest(target=target):
                binary_name = "cpt.exe" if name.endswith(".zip") else "cpt"
                binary = Path("target", target, "release", binary_name)
                binary.parent.mkdir(parents=True)
                binary.write_bytes(b"native binary")
                with patch.object(release.subprocess, "run") as run:
                    release.package(target)
                self.assertEqual(run.call_args.args[0], [str(binary.resolve()), "--help"])
                self.assertNotEqual(Path(run.call_args.kwargs["cwd"]), Path.cwd())
                archive_path = Path("dist", name)
                if name.endswith(".zip"):
                    with zipfile.ZipFile(archive_path) as archive:
                        self.assertEqual(set(archive.namelist()), {binary_name, "README.md", "BUILD.json", "VENDORED-NOTICES.txt"})
                        self.assertIn(b"Test license", archive.read("VENDORED-NOTICES.txt"))
                        self.assertEqual(archive.read(binary_name), b"native binary")
                        metadata = json.loads(archive.read("BUILD.json"))
                else:
                    with tarfile.open(archive_path) as archive:
                        self.assertEqual(set(archive.getnames()), {binary_name, "README.md", "BUILD.json", "VENDORED-NOTICES.txt"})
                        self.assertIn(b"Test license", archive.extractfile("VENDORED-NOTICES.txt").read())
                        self.assertEqual(archive.getmember(binary_name).mode, 0o755)
                        self.assertEqual(archive.extractfile(binary_name).read(), b"native binary")
                        metadata = json.load(archive.extractfile("BUILD.json"))
                self.assertEqual(metadata, {"version": "1.2.3", "commit": "a" * 40, "target": target})

    def test_failed_binary_smoke_test_stops_packaging(self):
        with patch.object(release.subprocess, "run", side_effect=RuntimeError("binary failed")):
            with self.assertRaises(RuntimeError):
                release.package("x86_64-unknown-linux-gnu")
        self.assertFalse(Path("dist").exists())

    def test_missing_release_is_stable(self):
        with patch.object(release, "release_info", return_value=None):
            self.assertEqual(release.choose_release("v1.2.3", "a" * 40, "123"),
                             ("v1.2.3", False, None))

    def test_existing_stable_leads_to_unique_preview(self):
        with patch.object(release, "release_info", side_effect=[self.info(sha="b" * 40), None]) as lookup:
            self.assertEqual(release.choose_release("v1.2.3", "a" * 40, "123"),
                             ("v1.2.3-dev.123", True, None))
        self.assertEqual(lookup.call_args.args, ("v1.2.3-dev.123",))

    def test_published_rerun_does_not_write(self):
        with patch.object(release, "release_info", return_value=self.info()), \
                patch.object(release.subprocess, "run") as run:
            release.publish()
        run.assert_not_called()

    def test_new_release_uploads_every_archive_and_checksums_before_publishing(self):
        self.archives()
        with patch.object(release, "github_info", return_value=None), \
                patch.object(release.subprocess, "run") as run:
            release.publish()
        commands = [call.args[0] for call in run.call_args_list]
        self.assertIn("--draft", commands[0])
        self.assertIn("--generate-notes", commands[0])
        self.assertEqual(commands[0][commands[0].index("--target") + 1], "a" * 40)
        self.assertEqual(commands[1][:4], ["gh", "release", "upload", "v1.2.3"])
        self.assertEqual(len(commands[1][4:-1]), 5)
        self.assertEqual(commands[2], ["gh", "release", "edit", "v1.2.3", "--draft=false", "--latest=true"])
        for line in Path("dist/SHA256SUMS").read_text(encoding="utf-8").splitlines():
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256(Path("dist", name).read_bytes()).hexdigest())

    def test_preview_does_not_replace_latest_stable(self):
        self.archives()
        with patch.object(release, "release_info", side_effect=[self.info(sha="b" * 40), None]), \
                patch.object(release, "github_info", return_value=None), \
                patch.object(release.subprocess, "run") as run:
            release.publish()
        self.assertIn("--prerelease", run.call_args_list[0].args[0])
        self.assertIn("--latest=false", run.call_args_list[-1].args[0])

    def test_draft_retry_resumes_upload(self):
        self.archives()
        with patch.object(release, "release_info", return_value=self.info(draft=True)), \
                patch.object(release.subprocess, "run") as run:
            release.publish()
        self.assertEqual([call.args[0][2] for call in run.call_args_list], ["upload", "edit"])

    def test_incomplete_downloads_cannot_publish(self):
        Path("dist").mkdir()
        with patch.object(release, "release_info", return_value=None), \
                patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "exactly one archive"):
                release.publish()
        run.assert_not_called()

    def test_upload_failure_leaves_draft_unpublished(self):
        self.archives()
        with patch.object(release, "github_info", return_value=None), \
                patch.object(release.subprocess, "run", side_effect=[None, RuntimeError("upload failed")]) as run:
            with self.assertRaises(RuntimeError):
                release.publish()
        self.assertEqual(run.call_count, 2)

    def test_existing_tag_cannot_be_repointed(self):
        self.archives()
        with patch.object(release, "github_info", side_effect=[
            None, {"object": {"type": "commit", "sha": "b" * 40}}
        ]), patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "existing tag"):
                release.publish()
        run.assert_not_called()

    def test_another_commits_draft_must_be_finished_first(self):
        with patch.object(release, "release_info", return_value=self.info(draft=True, sha="b" * 40)):
            with self.assertRaisesRegex(ValueError, "interrupted"):
                release.choose_release("v1.2.3", "a" * 40, "123")

    def test_only_not_found_is_treated_as_missing_release(self):
        for status in [404, 403, 500]:
            with self.subTest(status=status), patch.object(
                release, "urlopen", side_effect=HTTPError("url", status, "error", {}, None)
            ):
                if status == 404:
                    self.assertIsNone(release.release_info("v1.2.3"))
                else:
                    with self.assertRaises(HTTPError):
                        release.release_info("v1.2.3")

    def test_unstable_or_unsafe_version_is_rejected(self):
        for value in ["1.2.3-rc.1", "01.2.3", "../../other"]:
            with self.subTest(version=value):
                Path("Cargo.toml").write_text(f'[package]\nversion = "{value}"\n', encoding="utf-8")
                with self.assertRaises(ValueError):
                    release.version()


if __name__ == "__main__":
    unittest.main()
