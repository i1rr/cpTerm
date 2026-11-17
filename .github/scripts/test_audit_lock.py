"""Keep the approved maintenance exception narrow and fail closed."""

import copy
import io
import json
import unittest
from unittest.mock import patch

import audit_lock


class MaintenanceExceptionTests(unittest.TestCase):
    def setUp(self):
        self.notice = {
            "id": "RUSTSEC-2026-0543",
            "affected": [{
                "package": {"name": "chrono", "ecosystem": "crates.io"},
                "database_specific": {"informational": "unmaintained", "cvss": None, "categories": []},
            }],
        }

    def allowed(self, notice):
        with patch.object(audit_lock, "urlopen", return_value=io.BytesIO(json.dumps(notice).encode())):
            return audit_lock.approved_maintenance_notice("chrono", "RUSTSEC-2026-0543")

    def test_approved_unmaintained_notice_is_allowed(self):
        self.assertTrue(self.allowed(self.notice))

    def test_other_advisories_and_packages_remain_blocking(self):
        with patch.object(audit_lock, "urlopen") as request:
            self.assertFalse(audit_lock.approved_maintenance_notice("chrono", "RUSTSEC-2026-0001"))
            self.assertFalse(audit_lock.approved_maintenance_notice("other", "RUSTSEC-2026-0543"))
        request.assert_not_called()

    def test_security_metadata_or_missing_classification_disables_exception(self):
        for field, value in [("informational", None), ("informational", "unsound"),
                             ("cvss", "CVSS:3.1/AV:N"), ("categories", ["memory-corruption"])]:
            with self.subTest(field=field, value=value):
                notice = copy.deepcopy(self.notice)
                notice["affected"][0]["database_specific"][field] = value
                self.assertFalse(self.allowed(notice))
        for entry in [False, True]:
            with self.subTest(affected_severity=entry):
                notice = copy.deepcopy(self.notice)
                container = notice["affected"][0] if entry else notice
                container["severity"] = [{"type": "CVSS_V3", "score": "CVSS:3.1/AV:N"}]
                self.assertFalse(self.allowed(notice))

    def test_changed_advisory_identity_or_package_disables_exception(self):
        for field, value in [("name", "other"), ("ecosystem", "npm")]:
            notice = copy.deepcopy(self.notice)
            notice["affected"][0]["package"][field] = value
            self.assertFalse(self.allowed(notice))
        notice = copy.deepcopy(self.notice)
        notice["id"] = "different-advisory"
        self.assertFalse(self.allowed(notice))
        self.assertFalse(self.allowed({"id": self.notice["id"]}))

    def test_lookup_failure_cannot_silently_allow_a_release(self):
        with patch.object(audit_lock, "urlopen", side_effect=TimeoutError):
            with self.assertRaises(TimeoutError):
                audit_lock.approved_maintenance_notice("chrono", "RUSTSEC-2026-0543")


if __name__ == "__main__":
    unittest.main()
