"""Verify ordinary copies in a child whose hard-link syscalls are unavailable."""

import ctypes
import errno
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path


def without_hardlinks(executable):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("This syscall fixture requires the Linux x64 CI runner")

    class Filter(ctypes.Structure):
        _fields_ = [("code", ctypes.c_ushort), ("jt", ctypes.c_ubyte),
                    ("jf", ctypes.c_ubyte), ("k", ctypes.c_uint)]

    class Program(ctypes.Structure):
        _fields_ = [("length", ctypes.c_ushort), ("filter", ctypes.POINTER(Filter))]

    # Load seccomp_data.nr; return EOPNOTSUPP for x86_64 link/linkat, allow
    # everything else. This filter affects only this disposable test process.
    filters = (Filter * 6)(
        Filter(0x20, 0, 0, 0),
        Filter(0x15, 0, 1, 86), Filter(0x06, 0, 0, 0x00050000 | errno.EOPNOTSUPP),
        Filter(0x15, 0, 1, 265), Filter(0x06, 0, 0, 0x00050000 | errno.EOPNOTSUPP),
        Filter(0x06, 0, 0, 0x7FFF0000),
    )
    program = Program(len(filters), filters)
    libc = ctypes.CDLL(None, use_errno=True)
    libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong,
                          ctypes.c_ulong, ctypes.c_ulong]
    libc.prctl.restype = ctypes.c_int
    pointer = ctypes.cast(ctypes.pointer(program), ctypes.c_void_p).value
    for args in [(38, 1, 0, 0, 0), (22, 2, pointer, 0, 0)]:
        if libc.prctl(*args) != 0:
            raise OSError(ctypes.get_errno(), "Cannot install the hard-link test filter")
    with tempfile.TemporaryDirectory(prefix="cpt-no-hardlinks-") as work:
        source = Path(work, "source")
        source.write_bytes(b"fixture")
        directory = os.open(work, os.O_RDONLY | os.O_DIRECTORY)
        try:
            for attempt in [
                lambda: os.link(source, Path(work, "link")),
                lambda: os.link("source", "link", src_dir_fd=directory, dst_dir_fd=directory),
            ]:
                try:
                    attempt()
                except OSError as error:
                    if error.errno != errno.EOPNOTSUPP:
                        raise
                else:
                    raise RuntimeError("Hard-link fixture did not block the syscall")
        finally:
            os.close(directory)
    os.execv(executable, [executable, "copy_pipeline_resolves_repeated_conflicts_via_rename", "--exact"])


def main():
    result = subprocess.run(
        ["cargo", "+stable", "test", "--locked", "--test", "integration_ops",
         "--no-run", "--message-format=json"], check=True, text=True, capture_output=True,
    )
    artifacts = [json.loads(line) for line in result.stdout.splitlines()]
    executable = next(entry["executable"] for entry in artifacts
                      if entry.get("reason") == "compiler-artifact"
                      and entry.get("target", {}).get("name") == "integration_ops"
                      and entry.get("executable"))
    subprocess.run([sys.executable, __file__, "--child", executable], check=True)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--child":
        without_hardlinks(sys.argv[2])
    else:
        main()
