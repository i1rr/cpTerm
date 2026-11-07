"""Check every registry package in Cargo.lock against the public OSV database."""

import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.request import Request, urlopen


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
    for offset in range(0, len(queries), 75):
        batch = queries[offset:offset + 75]
        request = Request("https://api.osv.dev/v1/querybatch",
                          data=json.dumps({"queries": batch}).encode(),
                          headers={"Content-Type": "application/json"})
        with urlopen(request, timeout=45) as response:
            results = json.load(response)["results"]
        for query, result in zip(batch, results, strict=True):
            for advisory in result.get("vulns", []):
                print(query["package"]["name"], query["version"], advisory["id"])
                findings += 1
    print(f"Checked {len(queries)} locked packages; {findings} advisory matches")
    return int(findings != 0)


if __name__ == "__main__":
    sys.exit(main())
