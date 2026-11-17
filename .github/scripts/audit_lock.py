"""Check every registry package in Cargo.lock against the public OSV database."""

import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.request import Request, urlopen


def approved_maintenance_notice(package, advisory_id):
    # Approved by the maintainer on 2026-11-17. chrono is also used transitively
    # by russh-sftp/russh-util. This exception covers maintenance status only;
    # migrating those dependencies is separate from publishing native builds.
    if (package, advisory_id) != ("chrono", "RUSTSEC-2026-0543"):
        return False
    with urlopen(f"https://api.osv.dev/v1/vulns/{advisory_id}", timeout=45) as response:
        advisory = json.load(response)
    affected = advisory.get("affected", [])
    if advisory.get("id") != advisory_id or advisory.get("severity") or len(affected) != 1:
        return False
    entry = affected[0]
    metadata = entry.get("database_specific", {})
    return (
        entry.get("package", {}).get("name") == package
        and entry.get("package", {}).get("ecosystem") == "crates.io"
        and metadata.get("informational") == "unmaintained"
        and not metadata.get("cvss")
        and not metadata.get("categories")
        and not entry.get("severity")
    )


def main():
    packages = tomllib.loads(Path("Cargo.lock").read_text(encoding="utf-8"))["package"]
    queries = []
    for package in packages:
        if "source" not in package:
            continue  # local workspace package
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            sys.exit(f"Review non-registry source for {package['name']}")
        if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
            sys.exit(f"Missing registry checksum for {package['name']}")
        queries.append({"package": {"ecosystem": "crates.io", "name": package["name"]},
                        "version": package["version"]})
    if not queries:
        sys.exit("No registry packages found")
    findings = 0
    maintenance_notices = 0
    for offset in range(0, len(queries), 75):
        batch = queries[offset:offset + 75]
        request = Request("https://api.osv.dev/v1/querybatch",
                          data=json.dumps({"queries": batch}).encode(),
                          headers={"Content-Type": "application/json"})
        with urlopen(request, timeout=45) as response:
            results = json.load(response)["results"]
        for query, result in zip(batch, results, strict=True):
            for advisory in result.get("vulns", []):
                if approved_maintenance_notice(query["package"]["name"], advisory["id"]):
                    print("warning:", query["package"]["name"], query["version"],
                          advisory["id"], "approved unmaintained notice; dependency migration pending")
                    maintenance_notices += 1
                    continue
                print(query["package"]["name"], query["version"], advisory["id"])
                findings += 1
    print(f"Checked {len(queries)} locked packages; {findings} blocking advisory matches; "
          f"{maintenance_notices} approved maintenance notices")
    return int(findings != 0)


if __name__ == "__main__":
    sys.exit(main())
