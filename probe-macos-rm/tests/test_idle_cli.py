"""CPU-only CLI regression checks: the fake suite never opens a GPU device."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FAKE_SUITE = r"""
#include "nvx_suite.h"
#include <stdlib.h>

static int transport, chain, telemetry;

int nvx_stage0(nvx_ctx *c) {
    if (transport++) abort();
    puts("FAKE transport");
    if (getenv("NVX_FAKE_NO_DRIVER")) return -1;
    c->ctl_fd = 0; /* fd zero is valid and must not cause a second open. */
    c->dev_fd = 7;
    c->gpuId = 0x2187;
    return 0;
}
int nvx_stage1(nvx_ctx *c, NvU32 gpu) {
    if (!transport || c->ctl_fd != 0 || chain++) abort();
    printf("FAKE chain gpu=%u\n", gpu);
    c->hClient = 10; c->hDevice = 11; c->hSubdevice = 12;
    return 0;
}
int nvx_stage2(nvx_ctx *c) {
    if (!chain || !c->hSubdevice || telemetry++) abort();
    puts("FAKE telemetry");
    return 0;
}
void nvx_stage4(nvx_ctx *c, int seconds) {
    if (!telemetry || !c->hSubdevice) abort();
    printf("FAKE sample %d\n", seconds);
}
int nvx_stage2_tierb_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE tierb");
    return 0;
}
int nvx_stage2_rated_tdp_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE ratedtdp");
    return 0;
}
int nvx_stage3_rated_tdp_set(nvx_ctx *c, NvU32 client, NvU32 action) {
    (void)c; (void)client; (void)action; abort();
}
int nvx_stage2_thermal_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE thermal");
    return 0;
}
int nvx_stage2_pm_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE pmread");
    return 0;
}
int nvx_stage2_perf_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE perfread");
    return 0;
}
int nvx_stage2_clk_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkvf");
    return 0;
}
int nvx_stage2_clk_doc(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkdoc");
    return 0;
}
int nvx_stage3(nvx_ctx *c) { (void)c; abort(); }
int nvx_stage5(nvx_ctx *c) { (void)c; abort(); }
int nvx_close(int fd) { printf("FAKE close %d\n", fd); return 0; }
void nvx_restore_arm(nvx_ctx *c) { (void)c; abort(); }
void nvx_restore_run(void) {}
void nvx_emit(nvx_ctx *c, const char *id, const char *name,
              NvU32 status, int verdict, const char *fmt, ...) {
    (void)c; (void)id; (void)name; (void)status; (void)verdict; (void)fmt;
    abort();
}
"""


class IdleCli(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.directory = Path(cls.temp.name)
        fake = cls.directory / "fake.c"
        fake.write_text(FAKE_SUITE)
        cls.binary = cls.directory / "probe-idle-fake"
        sdk = Path(os.environ["OGKM"]) / "src/common/sdk/nvidia/inc"
        subprocess.run(
            [
                os.environ.get("CC", "cc"),
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-DNVX_IDLE_ONLY=1",
                f"-I{sdk}",
                f"-I{ROOT / 'src'}",
                str(ROOT / "src/main.c"),
                str(fake),
                "-o",
                str(cls.binary),
            ],
            check=True,
        )

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def run_cli(self, *args, no_driver=False):
        env = dict(os.environ)
        env.pop("NVX_FAKE_NO_DRIVER", None)
        if no_driver:
            env["NVX_FAKE_NO_DRIVER"] = "1"
        return subprocess.run(
            [str(self.binary), *args],
            capture_output=True,
            text=True,
            timeout=5,
            env=env,
        )

    def test_workloads_refused_before_transport_or_log_creation(self):
        for args in [
            ("--write",),
            ("--stages", "3"),
            ("--stages", "5"),
            ("--stages", "01235", "--write"),
        ]:
            with self.subTest(args=args):
                log = self.directory / "must-not-exist.jsonl"
                result = self.run_cli(*args, "--jsonl", str(log))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("idle-only build refuses", result.stderr)
                self.assertNotIn("FAKE", result.stdout)
                self.assertFalse(log.exists())

    def test_invalid_arguments_never_open_transport(self):
        for args in [
            ("--stages", ""),
            ("--stages", "019"),
            ("--sample", "-1"),
            ("--sample", "oops"),
            ("--sample", "0"),
            ("--sample", "2147483648"),
            ("--gpu", "4294967296"),
            ("--gpu", "12oops"),
        ]:
            with self.subTest(args=args):
                result = self.run_cli(*args)
                self.assertEqual(result.returncode, 2)
                self.assertNotIn("FAKE", result.stdout)

    def test_independent_stages_prepare_prerequisites(self):
        for stage in ("1", "2", "4"):
            with self.subTest(stage=stage):
                result = self.run_cli("--stages", stage, "--sample", "1")
                self.assertEqual(result.returncode, 0, result.stderr)
                for event in ("transport", "chain", "telemetry", "sample"):
                    self.assertEqual(result.stdout.count(f"FAKE {event}"), 1)

    def test_repeated_stages_reuse_connections_and_chain(self):
        result = self.run_cli("--stages", "001122", "--sample", "1")
        self.assertEqual(result.returncode, 0, result.stderr)
        for event in ("transport", "chain", "telemetry", "close 0", "close 7"):
            self.assertEqual(result.stdout.count(f"FAKE {event}"), 1)

    def test_explicit_zero_gpu_id_is_preserved(self):
        result = self.run_cli("--stages", "1", "--gpu", "0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("FAKE chain gpu=0\n", result.stdout)

    def test_tierb_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE tierb", plain.stdout)

        asked = self.run_cli("--stages", "0", "--tierb-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE tierb"), 1)

    def test_rated_tdp_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE ratedtdp", plain.stdout)

        asked = self.run_cli("--stages", "0", "--rated-tdp-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE ratedtdp"), 1)

    def test_thermal_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE thermal", plain.stdout)

        asked = self.run_cli("--stages", "0", "--thermal-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE thermal"), 1)

    def test_pm_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE pmread", plain.stdout)
        asked = self.run_cli("--stages", "0", "--pm-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE pmread"), 1)

    def test_perf_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE perfread", plain.stdout)
        asked = self.run_cli("--stages", "0", "--perf-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE perfread"), 1)

    def test_clkvf_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE clkvf", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clkvf-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE clkvf"), 1)

    def test_clkdoc_reads_are_explicit_opt_in(self):
        plain = self.run_cli()
        self.assertNotIn("FAKE clkdoc", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clkdoc-read")
        self.assertEqual(asked.returncode, 0)
        self.assertEqual(asked.stdout.count("FAKE clkdoc"), 1)

    def test_rated_tdp_set_is_gated_like_other_writes(self):
        log = self.directory / "must-not-exist.jsonl"
        # Without --write it is refused before the driver, with a clear reason.
        for args in [("--rated-tdp-set", "GLOBAL:EXCEED")]:
            with self.subTest(args=args):
                result = self.run_cli(*args, "--jsonl", str(log))
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("requires", result.stderr)
                self.assertNotIn("FAKE", result.stdout)
                self.assertFalse(log.exists())
        # In the idle-only profile, --write itself is refused.
        result = self.run_cli("--write", "--rated-tdp-set", "GLOBAL:EXCEED")
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("idle-only build refuses", result.stderr)
        self.assertNotIn("FAKE", result.stdout)
        # A malformed spec is a usage error, also before transport.
        for spec in ("GLOBAL", "GLOBAL:BOGUS", "BOGUS:EXCEED", "9:1", "1:9", ":"):
            with self.subTest(spec=spec):
                result = self.run_cli("--write", "--rated-tdp-set", spec)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertNotIn("FAKE", result.stdout)

    def test_failed_transport_stops_before_chain_and_sampling(self):
        for stage in ("0", "1", "2", "4"):
            with self.subTest(stage=stage):
                result = self.run_cli(
                    "--stages", stage, "--sample", "1", no_driver=True
                )
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertNotIn("FAKE chain", result.stdout)
                self.assertNotIn("FAKE sample", result.stdout)
                self.assertNotIn("FAKE close", result.stdout)


if __name__ == "__main__":
    unittest.main()
