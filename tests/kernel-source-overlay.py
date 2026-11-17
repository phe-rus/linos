#!/usr/bin/env python3
"""Fast host-independent regression tests for the Linos kernel source overlay."""
from pathlib import Path
import runpy
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
APPLY = runpy.run_path(str(ROOT / "kernel/apply-source.py"))["apply"]


class OverlayTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="linos-overlay-test-")
        self.addCleanup(self.directory.cleanup)
        self.source = Path(self.directory.name)
        (self.source / "kernel").mkdir()
        (self.source / "Kconfig").write_text('menu "Kernel"\nendmenu\n')
        (self.source / "kernel/Makefile").write_text("obj-y += fork.o\n")
        self.overlay = ROOT / "kernel/overlays"

    def test_applies_and_is_idempotent(self) -> None:
        APPLY(self.source, self.overlay)
        APPLY(self.source, self.overlay)
        config = (self.source / "Kconfig").read_text()
        makefile = (self.source / "kernel/Makefile").read_text()
        self.assertEqual(config.count('source "kernel/Kconfig.linos"'), 1)
        self.assertEqual(
            makefile.count("obj-$(CONFIG_LINOS_BOOT_STATE) += linos_boot_state.o"), 1
        )
        self.assertEqual(
            (self.source / "kernel/linos_boot_state.c").read_bytes(),
            (self.overlay / "linos_boot_state.c").read_bytes()
        )

    def test_refuses_conflicting_existing_file(self) -> None:
        (self.source / "kernel/linos_boot_state.c").write_text("different")
        with self.assertRaisesRegex(RuntimeError, "conflicting"):
            APPLY(self.source, self.overlay)

    def test_requires_kernel_source_layout(self) -> None:
        (self.source / "Kconfig").unlink()
        with self.assertRaisesRegex(RuntimeError, "upstream"):
            APPLY(self.source, self.overlay)


if __name__ == "__main__":
    unittest.main()
