#!/usr/bin/env python3
"""Linos kernel contract preflight, independent of any build host or kernel tree."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "kernel/config"

class KernelPolicy(unittest.TestCase):
    def parse(self, file: str) -> dict[str, str]:
        lines = (CONFIG / file).read_text().splitlines()
        result: dict[str, str] = {}
        for line in lines:
            if not line or line.startswith("#"):
                continue
            match = re.fullmatch(r"(CONFIG_[A-Z0-9_]+)=(y|m|n)", line)
            self.assertIsNotNone(match, f"Invalid configuration in {file}: {line}")
            key, value = match.groups()
            self.assertNotIn(key, result, f"Duplicate {key} in {file}")
            result[key] = value
        return result

    def test_common_security_recovery_and_performance(self):
        cfg = self.parse("common.config")
        for name in (
            "CONFIG_MODULES", "CONFIG_BLK_DEV_INITRD",
            "CONFIG_SECURITY_LANDLOCK", "CONFIG_SECURITY_YAMA",
            "CONFIG_HARDENED_USERCOPY", "CONFIG_SECCOMP_FILTER",
            "CONFIG_LINOS_BOOT_STATE", "CONFIG_PSI", "CONFIG_MEMCG",
            "CONFIG_CGROUP_PIDS", "CONFIG_HIGH_RES_TIMERS",
            "CONFIG_PREEMPT_DYNAMIC", "CONFIG_PSTORE",
            "CONFIG_DM_VERITY", "CONFIG_OVERLAY_FS",
            "CONFIG_DEBUG_INFO_NONE",
        ):
            self.assertEqual(cfg.get(name), "y", name)
        for name in ("CONFIG_LOCALVERSION_AUTO", "CONFIG_DEBUG_INFO_BTF"):
            self.assertEqual(cfg.get(name), "n", name)

    def test_architecture_profiles_have_console_and_graphics(self):
        for arch in ("amd64", "arm64"):
            with self.subTest(arch=arch):
                cfg = self.parse(arch + ".config")
                for symbol in (
                    "CONFIG_DRM_VIRTIO_GPU", "CONFIG_FRAMEBUFFER_CONSOLE",
                    "CONFIG_INPUT_EVDEV", "CONFIG_VT_CONSOLE",
                ):
                    self.assertEqual(cfg.get(symbol), "y", symbol)
                self.assertEqual(set(cfg).intersection(self.parse("common.config")), set())

    def test_version_and_source_contract(self):
        version = (ROOT / "kernel/source.conf").read_text()
        self.assertRegex(version, r"(?m)^LINOS_KERNEL_VERSION=\d+\.\d+\.\d+$")
        self.assertRegex(version, r"(?m)^LINOS_KERNEL_LTS=\d+\.\d+\.\d+$")
        source = (ROOT / "kernel/overlays/linos_boot_state.c").read_text()
        self.assertIn("CAP_SYS_BOOT", source)
        self.assertIn("sysfs_streq(buf, \"ready\")", source)
        self.assertIn("atomic_cmpxchg", source)

if __name__ == "__main__":
    unittest.main()
