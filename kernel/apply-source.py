#!/usr/bin/env python3
"""Apply the reviewed Linos kernel overlay only to disposable source."""
from pathlib import Path
import sys
import hashlib

def apply(src: Path, overlay: Path) -> None:
    for p in ("Kconfig", "kernel/Makefile"):
        if not (src / p).is_file():
            raise RuntimeError("Not an upstream Linux source tree: " + p)
    for name in ("linos_boot_state.c", "Kconfig.linos"):
        dest = src / "kernel" / name
        data = (overlay / name).read_bytes()
        if dest.exists() and dest.read_bytes() != data:
            raise RuntimeError("Refusing conflicting Linos overlay: " + str(dest))
        dest.write_bytes(data)
        print(f"overlay {name}: {hashlib.sha256(data).hexdigest()}")
    for name, line in (
        ("Kconfig", 'source "kernel/Kconfig.linos"'),
        ("kernel/Makefile", "obj-$(CONFIG_LINOS_BOOT_STATE) += linos_boot_state.o"),
    ):
        path = src / name
        original = path.read_text()
        if line not in original.splitlines():
            path.write_text(original.rstrip("\n") + "\n" + line + "\n")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: apply-source.py <disposable-source> <overlay-dir>")
    apply(Path(sys.argv[1]), Path(sys.argv[2]))
