"""CPU-only CLI regression checks: the fake suite never opens a GPU device."""

import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FAKE_SUITE = r"""
#include "nvx_suite.h"
#include <stdlib.h>

static int transport, chain, telemetry;

/* Transport-selector symbols main.c links against (see --service). */
int nvx_service_pref;
const char *nvx_service_name(void) { return "(fake)"; }


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
int nvx_stage3_rated_tdp_set(nvx_ctx *c, NvU32 client, NvU32 action,
                             int hold_sec) {
    (void)c; (void)client; (void)action; (void)hold_sec; abort();
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
int nvx_stage2_reg_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE regread");
    return 0;
}
int nvx_stage2_clk_level_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clklevel");
    return 0;
}
int nvx_stage2_clkdom_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkdom");
    return 0;
}
int nvx_stage2_clkctl_read(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkctl");
    return 0;
}
int nvx_stage2_clkctl_hex(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkctl-hex");
    return 0;
}
int nvx_stage2_clkctl_offsweep(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkctl-offsweep");
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
int nvx_stage2_clk_obj(nvx_ctx *c) {
    if (!chain || !c->hSubdevice) abort();
    puts("FAKE clkobj");
    return 0;
}
int nvx_stage4_clk_track(nvx_ctx *c, int seconds) {
    if (!telemetry || !c->hSubdevice) abort();
    printf("FAKE clktrack %d\n", seconds);
    return 0;
}
int nvx_stage2_anchor_read(nvx_ctx *c) { (void)c; abort(); }
int nvx_stage2_fan_read(nvx_ctx *c) { (void)c; abort(); }
int nvx_stage2_hwfs_read(nvx_ctx *c) { (void)c; abort(); }
int nvx_stage3_clk_offset_set(nvx_ctx *c, unsigned bit, unsigned off,
                            int delta, int bump, int hold) {
    (void)c; (void)bit; (void)off; (void)delta; (void)bump; (void)hold;
    abort();
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

    def test_unit_selector_cannot_be_mistaken_for_an_enable_bit(self):
        log = self.directory / "arm-must-not-open.jsonl"
        result = self.run_cli(
            "--write",
            "--clk-offset-set",
            "0:12:250",
            "--clk-offset-arm",
            "--jsonl",
            str(log),
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("selects units", result.stderr)
        self.assertNotIn("FAKE", result.stdout)
        self.assertFalse(log.exists())

    def test_malformed_offset_step_never_reaches_transport(self):
        for spec in (
            ":12:250",
            "0::250",
            "0:12:",
            "0:12::bump",
            "0:12:18751",
            "0:12:-18751",
            "0:12:99999999999999999999999",
        ):
            with self.subTest(spec=spec):
                result = self.run_cli("--write", "--clk-offset-set", spec)
                self.assertEqual(result.returncode, 2)
                self.assertIn("invalid --clk-offset-set spec", result.stderr)
                self.assertNotIn("FAKE", result.stdout)
        result = self.run_cli("--clk-offset-hold", "0")
        self.assertEqual(result.returncode, 2)
        self.assertIn("invalid --clk-offset-hold", result.stderr)
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

    def test_reg_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE regread", plain.stdout)
        asked = self.run_cli("--stages", "0", "--reg-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE regread"), 1)

    def test_clkctl_reads_are_explicit_opt_in(self):
        """The GSP-verified clock-domain offset read is opt-in and off by default."""
        default = self.run_cli("--stages", "0")
        self.assertNotIn("clkctl", default.stdout)
        asked = self.run_cli("--stages", "0", "--clkctl-read")
        self.assertIn("FAKE clkctl", asked.stdout)

    def test_clkctl_hex_dump_is_explicit_opt_in(self):
        """The domain-block hex dump is opt-in and off by default."""
        default = self.run_cli("--stages", "0")
        self.assertNotIn("clkctl-hex", default.stdout)
        asked = self.run_cli("--stages", "0", "--clkctl-hex")
        self.assertIn("FAKE clkctl-hex", asked.stdout)

    def test_clkctl_offsweep_is_explicit_opt_in(self):
        """The bounded value-container probe is opt-in and off by default."""
        default = self.run_cli("--stages", "0")
        self.assertNotIn("clkctl-offsweep", default.stdout)
        asked = self.run_cli("--stages", "0", "--clkctl-offsweep")
        self.assertIn("FAKE clkctl-offsweep", asked.stdout)

    def test_clkdom_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE clkdom", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clkdom-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE clkdom"), 1)

    def test_clklevel_reads_are_explicit_opt_in(self):
        plain = self.run_cli("--stages", "012")
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertNotIn("FAKE clklevel", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clklevel-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE clklevel"), 1)

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

    def test_clkobj_reads_are_explicit_opt_in(self):
        plain = self.run_cli()
        self.assertNotIn("FAKE clkobj", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clkobj-read")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE clkobj"), 1)

    def test_clk_track_is_explicit_opt_in(self):
        plain = self.run_cli()
        self.assertNotIn("FAKE clktrack", plain.stdout)
        asked = self.run_cli("--stages", "0", "--clk-track", "1")
        self.assertEqual(asked.returncode, 0, asked.stderr)
        self.assertEqual(asked.stdout.count("FAKE clktrack"), 1)

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

    def test_service_selector_is_validated_before_transport(self):
        for value in ("nvrm", "nvaccel", "auto"):
            with self.subTest(value=value):
                result = self.run_cli("--stages", "0", "--service", value)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("FAKE transport", result.stdout)
        for value in ("bogus", "NVRM", ""):
            with self.subTest(value=value):
                result = self.run_cli("--stages", "0", "--service", value)
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


class DecodeClkContainers(unittest.TestCase):
    """Offline decoder for the private clock containers (tools/decode-*.py).

    The geometry it enforces is the one the GSP table implies; a regression
    here would silently re-introduce the "guessed header + count*stride"
    layouts that the reverse-engineering notes warn about.
    """

    TOOL = ROOT / "tools" / "decode-clk-containers.py"

    def run_tool(self, *args):
        return subprocess.run(
            [sys.executable, str(self.TOOL), *args],
            capture_output=True,
            text=True,
        )

    def test_selftest_covers_geometry_and_fields(self):
        result = self.run_tool("--selftest")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("selftest ok", result.stdout)

    def test_decodes_one_captured_block(self):
        header, stride = 20, 224
        buf = bytearray(20 + 32 * stride)
        struct.pack_into("<I", buf, 4, 0x7)
        struct.pack_into("<I", buf, header, 0x01030101)
        struct.pack_into("<I", buf, header + 8, 1638400)
        struct.pack_into("<i", buf, header + 20, -18750)
        struct.pack_into("<i", buf, header + 24, 18750)
        lines = [
            "[OK] V2.clkctl freq-ctrl-info   0x20809025 @7188  status "
            "0x00000000 (NV_OK)  cmd 0x20809025 size 7188 -> NV_OK, "
            "7188-byte hex follows"
        ]
        for off in range(0, len(buf), 16):
            lines.append(
                "    %05d: %s"
                % (off, " ".join("%02x" % b for b in buf[off : off + 16]))
            )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "hex.txt"
            path.write_text("\n".join(lines) + "\n", encoding="utf-8")
            result = self.run_tool(str(path))
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("max_vf_khz)=0x00190000/1638400", result.stdout)
        self.assertIn("offset_floor_khz)=0xffffb6c2/-18750", result.stdout)
        self.assertIn("offset_ceiling_khz)=0x0000493e/18750", result.stdout)

    def test_rejects_a_size_the_gsp_table_does_not_have(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "hex.txt"
            path.write_text(
                "[OK] x 0x20809025 @2108  status 0x00000000 (NV_OK)  "
                "cmd 0x20809025 size 2108 -> NV_OK, 2108-byte hex follows\n"
                "    00000: " + " ".join(["00"] * 16) + "\n",
                encoding="utf-8",
            )
            result = self.run_tool(str(path))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not the GSP paramSize", result.stdout + result.stderr)


class ClockWriteRunbook(unittest.TestCase):
    """Exercise runbook gates and artifact integrity without a GPU or sudo."""

    def run_case(self, *, unhealthy=False, refused=False, stale=False):
        import shutil

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "probe"
            tools = root / "tools"
            tools.mkdir(parents=True)
            for name in ("clock-offset-write-run.sh", "record-run-provenance.py"):
                shutil.copy2(ROOT / "tools" / name, tools / name)
            fake_bin = root / "probe-macos-rm"
            fake_bin.write_text(
                "#!/bin/sh\n"
                'if [ "$1" = --anchor-read ]; then\n'
                '  if [ "$TEST_UNHEALTHY" = 1 ]; then echo NV_OK;\n'
                '  else echo "forward ALIVE and healthy"; fi\n'
                "else\n"
                '  touch "$TEST_WRITE_MARKER"\n'
                '  if [ "$TEST_REFUSED" = 1 ]; then status=0x1b; else status=0x0; fi\n'
                '  echo "clk-offset-verdict SET $status, readback 0x0, same-field yes, '
                'others-unchanged yes, restore verified"\n'
                "fi\n"
            )
            fake_bin.chmod(0o755)
            fake_id = root / "id"
            fake_id.write_text("#!/bin/sh\necho 0\n")
            fake_id.chmod(0o755)
            (root / "build.sh").write_text(
                '#!/bin/sh\ntouch "$TEST_BUILD_MARKER" probe-macos-rm\n'
            )
            if stale:
                (root / "src").mkdir()
                (root / "src" / "changed.c").write_text("/* newer source */\n")
                os.utime(fake_bin, (1, 1))
            env = dict(os.environ)
            env.update(
                PATH=str(root) + os.pathsep + env["PATH"],
                OUT_ROOT=str(root / "outputs"),
                TEST_UNHEALTHY=str(int(unhealthy)),
                TEST_REFUSED=str(int(refused)),
                TEST_WRITE_MARKER=str(root / "write-ran"),
                TEST_BUILD_MARKER=str(root / "build-ran"),
            )
            result = subprocess.run(
                ["sh", str(tools / "clock-offset-write-run.sh"), "0:12:250"],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            bundles = list((root / "outputs").iterdir())
            self.assertEqual(len(bundles), 1)
            bundle = bundles[0]
            import hashlib

            lines = [
                line
                for line in (bundle / "SHA256SUMS").read_text().splitlines()
                if line and not line.startswith("#")
            ]
            self.assertTrue(any(line.endswith("README.txt") for line in lines))
            for line in lines:
                digest, name = line.split(maxsplit=1)
                self.assertEqual(
                    hashlib.sha256((bundle / name.strip()).read_bytes()).hexdigest(),
                    digest,
                    name,
                )
            return result, (root / "write-ran").exists(), (root / "build-ran").exists()

    def test_unhealthy_verdict_refuses_write_even_with_an_nv_ok_line(self):
        result, wrote, _ = self.run_case(unhealthy=True)
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertFalse(wrote)

    def test_refused_set_cannot_pass_from_readback_and_restore_alone(self):
        result, wrote, _ = self.run_case(refused=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertTrue(wrote)
        self.assertNotIn("SET + read-back + restore all verified", result.stdout)

    def test_stale_binary_is_rebuilt_and_final_summary_is_in_the_digest(self):
        result, wrote, built = self.run_case(stale=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(wrote)
        self.assertTrue(built)


if __name__ == "__main__":
    unittest.main()
