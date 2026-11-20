"""Check every registry package and pinned native source against OSV; no exceptions."""

import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.request import Request, urlopen


def locked_queries(root=Path(".")):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    packages = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    if manifest.get("patch"):
        raise ValueError("Private dependency patches are not permitted")
    queries = []
    for package in packages:
        name, version = package["name"], package["version"]
        if name in {"chrono", "openssl-src"}:
            raise ValueError(f"{name} must not be reintroduced into the dependency tree")
        if "source" not in package:
            if (name, version) == (manifest["package"]["name"], manifest["package"]["version"]):
                continue
            raise ValueError(f"Review non-registry source for {name}")
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"Review non-registry source for {name}")
        if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
            raise ValueError(f"Missing registry checksum for {name}")
        queries.append((f"{name} {version}",
                        {"package": {"ecosystem": "crates.io", "name": name}, "version": version}))
    if not queries:
        raise ValueError("No dependencies found")
    sources = json.loads((root / ".github/scripts/native_sources.json").read_text(encoding="utf-8"))
    if {source["crate"] for source in sources} != {"libssh2-sys", "libz-sys", "openssl-sys"} or len(sources) != 3:
        raise ValueError("Review the native source inventory")
    for source in sources:
        matches = [package for package in packages if package["name"] == source["crate"]]
        if len(matches) != 1 or (matches[0]["version"], matches[0].get("checksum")) != (source["crate_version"], source["crate_checksum"]):
            raise ValueError(f"Review native source revisions before updating {source['crate']}")
        if not re.fullmatch(r"[0-9a-f]{40}", source["commit"]):
            raise ValueError("Missing native source commit")
        if source["name"] == "LibreSSL":
            if not re.fullmatch(r"\d+\.\d+\.\d+", source["version"]) or source["archive"] != f"https://cdn.openbsd.org/pub/OpenBSD/LibreSSL/libressl-{source['version']}.tar.gz" or not re.fullmatch(r"[0-9a-f]{64}", source["archive_sha256"]):
                raise ValueError("Invalid LibreSSL release provenance")
        queries.append((f"native {source['name']} {source['version']}", {"commit": source["commit"]}))
    return queries


def main():
    queries = locked_queries()
    findings = 0
    for offset in range(0, len(queries), 75):
        batch = queries[offset:offset + 75]
        request = Request("https://api.osv.dev/v1/querybatch",
                          data=json.dumps({"queries": [query for _, query in batch]}).encode(),
                          headers={"Content-Type": "application/json"})
        with urlopen(request, timeout=45) as response:
            results = json.load(response)["results"]
        for (label, _), result in zip(batch, results, strict=True):
            for advisory in result.get("vulns", []):
                print(label, advisory["id"])
                findings += 1
    print(f"Checked {len(queries) - 3} locked packages and 3 native source revisions; "
          f"{findings} blocking advisory matches; no exceptions")
    return int(findings != 0)


if __name__ == "__main__":
    sys.exit(main())
