"""Check registry and reviewed vendored packages against OSV; no exceptions."""

import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.request import Request, urlopen


def locked_queries(root=Path(".")):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    packages = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    vendors = json.loads((root / "vendor/provenance.json").read_text(encoding="utf-8"))
    patches = manifest.get("patch", {}).get("crates-io", {})
    if set(patches) != set(vendors):
        raise ValueError("Registry patches must match reviewed vendor provenance")
    for name, vendor in vendors.items():
        if patches[name] != {"path": vendor["path"]}:
            raise ValueError(f"Unexpected patch source for {name}")
        directory = root / vendor["path"]
        if directory.is_symlink() or not directory.resolve().is_relative_to((root / "vendor").resolve()):
            raise ValueError(f"Invalid vendor path for {name}")
        files = {}
        for path in directory.rglob("*"):
            if path.is_symlink():
                raise ValueError(f"Vendor symlink: {path}")
            if path.is_file():
                files[path.relative_to(directory).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
        if files != vendor["files"]:
            raise ValueError(f"Vendored files changed without provenance review: {name}")
        package = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        if (package["name"], package["version"]) != (name, vendor["version"]):
            raise ValueError(f"Vendor package identity mismatch: {name}")

    queries = []
    found_vendors = set()
    for package in packages:
        name, version = package["name"], package["version"]
        if name == "chrono":
            raise ValueError("chrono must not be reintroduced into the dependency tree")
        if "source" not in package:
            if (name, version) == (manifest["package"]["name"], manifest["package"]["version"]):
                continue
            if name not in vendors or version != vendors[name]["version"]:
                raise ValueError(f"Review non-registry source for {name}")
            found_vendors.add(name)
        else:
            if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
                raise ValueError(f"Review non-registry source for {name}")
            if not re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")):
                raise ValueError(f"Missing registry checksum for {name}")
        # Vendoring must never hide the original package/version from advisories.
        queries.append({"package": {"ecosystem": "crates.io", "name": name}, "version": version})
    if found_vendors != set(vendors):
        raise ValueError("Locked packages must match reviewed vendor provenance")
    if not queries:
        raise ValueError("No dependencies found")
    return queries


def main():
    queries = locked_queries()
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
    print(f"Checked {len(queries)} locked packages including reviewed vendors; "
          f"{findings} blocking advisory matches; no exceptions")
    return int(findings != 0)


if __name__ == "__main__":
    sys.exit(main())
