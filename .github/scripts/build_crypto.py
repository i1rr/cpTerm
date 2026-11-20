"""Build the pinned, unmodified LibreSSL release; never patch library sources."""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import tarfile
from pathlib import Path
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]


def crypto_environment(prefix):
    environment = {
        "OPENSSL_DIR": str(prefix),
        "OPENSSL_STATIC": "1",
        "OPENSSL_LIBS": "ssl:crypto",
        "OPENSSL_NO_VENDOR": "1",
        "LIBZ_SYS_STATIC": "1",
        "VCPKGRS_DISABLE": "1",
    }
    if platform.system() == "Windows":
        environment["OPENSSL_LIBS"] = "libssl:libcrypto"
        flags = os.environ.get("RUSTFLAGS", "")
        if "+crt-static" not in flags:
            flags += " -C target-feature=+crt-static"
        environment["RUSTFLAGS"] = flags.strip()
    return environment


def build(prefix, cmake):
    sources = json.loads((ROOT / ".github/scripts/native_sources.json").read_text(encoding="utf-8"))
    spec = next(source for source in sources if source["name"] == "LibreSSL")
    if not re.fullmatch(r"\d+\.\d+\.\d+", spec["version"]):
        raise ValueError("Invalid LibreSSL release version")
    work = prefix.parent / f"libressl-{spec['version']}-build"
    work.mkdir(parents=True, exist_ok=True)
    archive = work / "source.tar.gz"
    if not archive.exists():
        request = Request(spec["archive"], headers={"User-Agent": "cpTerm-native-build"})
        with urlopen(request, timeout=60) as response, archive.open("wb") as output:
            while block := response.read(1024 * 1024):
                output.write(block)
    with archive.open("rb") as source:
        if hashlib.file_digest(source, "sha256").hexdigest() != spec["archive_sha256"]:
            raise ValueError("LibreSSL archive checksum mismatch; refusing to build")
    with tarfile.open(archive, "r:gz") as source:
        source.extractall(work, filter="data")
    source_dir = work / f"libressl-{spec['version']}"
    build_dir = work / "cmake"
    options = ["-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={prefix}",
               "-DCMAKE_INSTALL_LIBDIR=lib", "-DBUILD_SHARED_LIBS=OFF",
               "-DLIBRESSL_APPS=OFF", "-DLIBRESSL_TESTS=OFF"]
    if platform.system() == "Windows":
        options += ["-A", "x64", "-DUSE_STATIC_MSVC_RUNTIMES=ON"]
    subprocess.run([cmake, "-S", str(source_dir), "-B", str(build_dir), *options], check=True)
    subprocess.run([cmake, "--build", str(build_dir), "--config", "Release", "--parallel", "4"], check=True)
    subprocess.run([cmake, "--install", str(build_dir), "--config", "Release"], check=True)
    if platform.system() == "Windows":
        # libssh2-sys expects OpenSSL-style MSVC library names. These aliases
        # contain the exact same upstream library bytes; no sources are patched.
        for name in ["ssl", "crypto"]:
            shutil.copyfile(prefix / "lib" / f"{name}.lib", prefix / "lib" / f"lib{name}.lib")
    print(f"Built unmodified LibreSSL {spec['version']} from verified upstream archive")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=ROOT / "target/native/crypto")
    parser.add_argument("--cmake", default="cmake")
    parser.add_argument("--github-env", action="store_true")
    parser.add_argument("--cargo", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    sources = json.loads((ROOT / ".github/scripts/native_sources.json").read_text(encoding="utf-8"))
    version = next(source["version"] for source in sources if source["name"] == "LibreSSL")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Invalid LibreSSL release version")
    # A new SDK path makes Cargo rerun native feature detection after upgrades.
    prefix = args.prefix.resolve() / version
    build(prefix, args.cmake)
    environment = crypto_environment(prefix)
    if args.github_env:
        with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as output:
            for name, value in environment.items():
                if "\n" in value or "\r" in value:
                    raise ValueError("Invalid multiline build environment value")
                output.write(f"{name}={value}\n")
    if args.cargo:
        subprocess.run(["cargo", *args.cargo], env={**os.environ, **environment}, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
