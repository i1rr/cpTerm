"""Package native binaries and publish tested master commits using GitHub CLI."""

import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen


TARGETS = {
    "x86_64-unknown-linux-gnu": "cpt-linux-x86_64.tar.gz",
    "x86_64-pc-windows-msvc": "cpt-windows-x86_64.zip",
    "x86_64-apple-darwin": "cpt-macos-x86_64.tar.gz",
    "aarch64-apple-darwin": "cpt-macos-aarch64.tar.gz",
}


def version():
    value = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError("Release version must be a stable major.minor.patch version")
    return value


def package(target):
    archive_name = TARGETS[target]
    binary_name = "cpt.exe" if target.endswith("windows-msvc") else "cpt"
    binary = Path("target", target, "release", binary_name).resolve()
    # --help exits before reading session state. Keep even this smoke test isolated.
    with tempfile.TemporaryDirectory() as work:
        subprocess.run([str(binary), "--help"], cwd=work, check=True)
    metadata = json.dumps({"version": version(), "commit": os.environ["GITHUB_SHA"],
                           "target": target}, indent=2).encode() + b"\n"
    files = {binary_name: binary.read_bytes(),
             "README.md": Path("README.md").read_bytes(), "BUILD.json": metadata}
    Path("dist").mkdir(exist_ok=True)
    output = Path("dist", archive_name)
    if output.suffix == ".zip":
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, data in files.items():
                archive.writestr(name, data)
    else:
        with tarfile.open(output, "w:gz") as archive:
            for name, data in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if name == binary_name else 0o644
                archive.addfile(info, io.BytesIO(data))
    print(output)


def github_info(path):
    request = Request(
        f"https://api.github.com/repos/{os.environ['GH_REPO']}/{path}",
        headers={"Authorization": f"Bearer {os.environ['GH_TOKEN']}",
                 "Accept": "application/vnd.github+json", "User-Agent": "cpTerm-release"},
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        if error.code == 404:
            return None
        raise


def release_info(tag):
    return github_info(f"releases/tags/{tag}")


def choose_release(stable_tag, sha, run_id):
    stable = release_info(stable_tag)
    if stable is None or stable["target_commitish"] == sha:
        return stable_tag, False, stable
    if stable["draft"]:
        raise ValueError(f"Finish the interrupted {stable_tag} workflow before releasing another commit")
    tag = f"{stable_tag}-dev.{run_id}"
    return tag, True, release_info(tag)


def publish():
    sha = os.environ["GITHUB_SHA"]
    tag, preview, existing = choose_release(f"v{version()}", sha, os.environ["GITHUB_RUN_ID"])
    if existing is not None:
        if existing["target_commitish"] != sha:
            raise ValueError(f"Refusing to reuse {tag} for another commit")
        if not existing["draft"]:
            print(f"{tag} is already published; leaving its assets intact")
            return

    assets = sorted(Path("dist").iterdir())
    if {asset.name for asset in assets} != set(TARGETS.values()):
        raise ValueError("Expected exactly one archive for each supported target")
    checksums = Path("dist", "SHA256SUMS")
    checksums.write_text("".join(
        f"{hashlib.sha256(asset.read_bytes()).hexdigest()}  {asset.name}\n" for asset in assets
    ), encoding="utf-8")
    assets.append(checksums)

    if existing is None:
        ref = github_info(f"git/ref/tags/{tag}")
        if ref is not None and (ref["object"]["type"] != "commit" or ref["object"]["sha"] != sha):
            raise ValueError(f"Refusing to publish over existing tag {tag}")
        args = ["gh", "release", "create", tag, "--draft", "--target", sha,
                "--title", f"cpTerm {tag}", "--generate-notes"]
        if preview:
            args.append("--prerelease")
        subprocess.run(args, check=True)
    # A failed upload leaves a recoverable draft. Published assets are never replaced.
    subprocess.run(["gh", "release", "upload", tag, *map(str, assets), "--clobber"], check=True)
    subprocess.run(["gh", "release", "edit", tag, "--draft=false",
                    f"--latest={'false' if preview else 'true'}"], check=True)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "package":
        package(sys.argv[2])
    elif sys.argv[1:] == ["publish"]:
        publish()
    else:
        sys.exit("Usage: release.py package <target> | publish")
