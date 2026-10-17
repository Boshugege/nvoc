/*
 * probe-macos-rm — entry point.
 *
 * The default build permits idle queries only. The full profile additionally
 * contains memory smoke, gated performance writes, and GPU channel submission.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <unistd.h>
#include <errno.h>
#include <limits.h>

#include "nvtypes.h"
#include "nvx_suite.h"
#include "nvx_xnu.h"

static nvx_ctx g_ctx;

static int ensure_transport(void)
{
    return g_ctx.ctl_fd >= 0 ? 0 : nvx_stage0(&g_ctx);
}

static int ensure_chain(NvU32 gpuId, int explicit_gpu)
{
    if (ensure_transport() != 0) return -1;
    if (g_ctx.hSubdevice) return 0;
    return nvx_stage1(&g_ctx, explicit_gpu ? gpuId : g_ctx.gpuId);
}

static int parse_number(const char *raw, unsigned long long max,
                        unsigned long long *out)
{
    if (!raw[0] || raw[0] < '0' || raw[0] > '9') return -1;
    char *end;
    errno = 0;
    unsigned long long n = strtoull(raw, &end, 0);
    if (errno || *end || n > max) return -1;
    *out = n;
    return 0;
}

static int parse_rated_tdp_client(const char *s, NvU32 *out)
{
    static const struct { const char *name; NvU32 v; } map[] = {
        { "RM", 0 }, { "WAR", 1 }, { "WAR_BUG_1785342", 1 },
        { "GLOBAL", 2 }, { "OS", 3 }, { "PROFILE", 4 },
    };
    for (unsigned i = 0; i < sizeof map / sizeof map[0]; ++i)
        if (!strcmp(s, map[i].name)) { *out = map[i].v; return 0; }
    unsigned long long n;
    if (parse_number(s, 4, &n) == 0) { *out = (NvU32)n; return 0; }
    return -1;
}

static int parse_rated_tdp_action(const char *s, NvU32 *out)
{
    static const struct { const char *name; NvU32 v; } map[] = {
        { "DEFAULT", 0 }, { "EXCEED", 1 }, { "FORCE_EXCEED", 1 },
        { "LIMIT", 2 }, { "FORCE_LIMIT", 2 }, { "LOCK", 3 },
        { "FORCE_LOCK", 3 }, { "FLOOR", 4 }, { "FORCE_FLOOR", 4 },
    };
    for (unsigned i = 0; i < sizeof map / sizeof map[0]; ++i)
        if (!strcmp(s, map[i].name)) { *out = map[i].v; return 0; }
    unsigned long long n;
    if (parse_number(s, 4, &n) == 0) { *out = (NvU32)n; return 0; }
    return -1;
}

static int parse_rated_tdp_set(const char *spec, NvU32 *client, NvU32 *action)
{
    char buf[64];
    if (strlen(spec) >= sizeof buf) return -1;
    strcpy(buf, spec);
    char *colon = strchr(buf, ':');
    if (!colon) return -1;
    *colon = '\0';
    return (parse_rated_tdp_client(buf, client) == 0 &&
            parse_rated_tdp_action(colon + 1, action) == 0) ? 0 : -1;
}

static void on_signal(int sig)
{
    (void)sig;
    nvx_restore_run();
    _exit(130);
}

static void usage(void)
{
    printf(
        "probe-macos-rm — NullMoth NVRM kext RM-surface validation suite\n"
        "\n"
        "usage: probe-macos-rm [options]\n"
        "  --stages <set>   stage subset, e.g. 012 (default 012); 3=writes, 4=loop, 5=channel smoke\n"
        "  --write          allow stage-3 write tests (boost / powerstate / EDPp)\n"
        "  --sample <sec>   run the 1 Hz telemetry loop for <sec> seconds\n"
        "  --tierb-read     run the campaign-sourced read-only Tier-B GETs\n"
        "                   (voltage rails + V/F points; explicit opt-in)\n"
        "  --rated-tdp-read read-only PERF_RATED_TDP_GET_CONTROL (+ derived\n"
        "                   status); never sends the SET twin\n"
        "  --pm-read        read-only internal PM/PFM GETs (PM1 state, V/P-state\n"
        "                   mapping, boost-sync info; SET twins are not sent)\n"
        "  --thermal-read   read-only THERMAL_SYSTEM_EXECUTE_V2 GET batch\n"
        "                   (sensor count/readings/ranges; public opcodes)\n"
        "  --service S      pin the user client: nvrm | nvaccel | auto (default\n"
        "                   auto = NVAccel first, NVRM fallback); read-only, picks\n"
        "                   which kext answers the same control id\n"
        "  --reg-read       read-only pass over the ids the loaded NVRM really\n"
        "                   registers, each at its live NVOC paramSize (exact\n"
        "                   metadata, not a size scan; SET ids refused)\n"
        "  --clkdom-read   read-only retry of the clock-domain controls with\n"
        "                   the NVAPI (version|size) magic removed from the RM\n"
        "                   param block; short derived size list, GET only\n"
        "  --clkctl-read   read-only clock-domain offset/range read at the\n"
        "                   GSP-verified paramSize (0x2080901b@2108), with the\n"
        "                   written-dword map and the 60 + 32*64 record decode\n"
        "  --clkctl-hex    full-byte hex dump of the read-only domain blocks\n"
        "                   (0x20809019@12336, 0x20809025@7188) for offline\n"
        "                   record-layout decode\n"
        "  --clkctl-offsweep  bounded GET probe: 0xA5 background + per-header-\n"
        "                   dword 0xFF seed to locate the 0x2080901b mask field,\n"
        "                   plus the neighbouring CLK ids at their GSP sizes\n"
        "  --clklevel-read  read-only PERF_GET_LEVEL_INFO_V2 (0x2080200b\n"
        "                   @780) across levels/overclock flag; decodes\n"
        "                   per-domain current/default/min/max kHz\n"
        "  --perf-read      read-only perf/pmgr read-surface census: remaining\n"
        "                   public GETs (module info, video-engine perfmon) plus\n"
        "                   explicit skips for the ones that cannot be sent\n"
        "  --clkvf-read     read-only private ClockClient/VfPoints/PerfVfe GET\n"
        "  --clkdoc-read    read-only documented-marshalling pass over the\n"
        "  --clkobj-read    read-only object-carrier sweep: the same\n"
        "  --clk-track N    read-only 1 Hz tracker: RUSD gpc/mem clocks + the\n"
        "                   RM 0x20809061/776 B per-domain table (load A/B)\n"
        "                   documented forms against subdevice/device/client\n"
        "                   same private clock/VF read faces: one case per\n"
        "                   (paramSize, stamp, seed) triple, no size sweep\n"
        "                   census (paramSize oracle scan; SET twins never sent)\n"
        "  --rated-tdp-set C:A   gated WRITE: set RATED_TDP client C to action A\n"
        "                   (needs --write; pre-record + readback + restore)\n"
        "                   C=RM|WAR|GLOBAL|OS|PROFILE or 0-4, A=DEFAULT|EXCEED|\n"
        "                   LIMIT|LOCK|FLOOR or 0-4\n"
        "  --jsonl <file>   also write every result/sample as JSON lines\n"
        "  --gpu <id>       attach explicit gpuId (default: first from CARD_INFO)\n"
        "  --quiet          suppress text detail, keep JSONL\n"
        "  --help\n");
    if (NVX_IDLE_ONLY)
        printf("\nprofile: idle-only; --write and stages 3/5 are disabled; "
               "memory smoke, I2C and legacy probes are skipped.\n");
}

int main(int argc, char **argv)
{
    const char *stages = "012";
    const char *jsonl_path = NULL;
    int want_write = 0, sample_sec = 0, quiet = 0, explicit_gpu = 0;
    int tierb_read = 0, rated_tdp_read = 0, rated_tdp_set = 0;
    int thermal_read = 0, pm_read = 0, perf_read = 0, reg_read = 0;
    int clklevel_read = 0, clkdom_read = 0, clkctl_read = 0, clkctl_hex = 0;
    int clkctl_offsweep = 0, clkvf_read = 0;
    int clkdoc_read = 0;
    int clkobj_read = 0;
    int clk_track_sec = 0;
    NvU32 rtdp_client = 0, rtdp_action = 0;
    NvU32 gpuId = 0;

    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--stages") && i + 1 < argc) stages = argv[++i];
        else if (!strcmp(argv[i], "--write")) want_write = 1;
        else if (!strcmp(argv[i], "--sample") && i + 1 < argc) {
            unsigned long long value;
            if (parse_number(argv[++i], INT_MAX, &value) != 0 || value == 0) {
                fprintf(stderr, "invalid --sample duration\n"); return 2;
            }
            sample_sec = (int)value;
        }
        else if (!strcmp(argv[i], "--jsonl") && i + 1 < argc) jsonl_path = argv[++i];
        else if (!strcmp(argv[i], "--gpu") && i + 1 < argc) {
            unsigned long long value;
            if (parse_number(argv[++i], UINT32_MAX, &value) != 0) {
                fprintf(stderr, "invalid --gpu ID\n"); return 2;
            }
            gpuId = (NvU32)value;
            explicit_gpu = 1;
        }
        else if (!strcmp(argv[i], "--tierb-read")) tierb_read = 1;
        else if (!strcmp(argv[i], "--rated-tdp-read")) rated_tdp_read = 1;
        else if (!strcmp(argv[i], "--thermal-read")) thermal_read = 1;
        else if (!strcmp(argv[i], "--pm-read")) pm_read = 1;
        else if (!strcmp(argv[i], "--perf-read")) perf_read = 1;
        else if (!strcmp(argv[i], "--clklevel-read")) clklevel_read = 1;
        else if (!strcmp(argv[i], "--clkdom-read")) clkdom_read = 1;
        else if (!strcmp(argv[i], "--clkctl-read")) clkctl_read = 1;
        else if (!strcmp(argv[i], "--clkctl-hex")) clkctl_hex = 1;
        else if (!strcmp(argv[i], "--clkctl-offsweep")) clkctl_offsweep = 1;
        else if (!strcmp(argv[i], "--reg-read")) reg_read = 1;
        else if (!strcmp(argv[i], "--clkvf-read")) clkvf_read = 1;
        else if (!strcmp(argv[i], "--clkdoc-read")) clkdoc_read = 1;
        else if (!strcmp(argv[i], "--clkobj-read")) clkobj_read = 1;
        else if (!strcmp(argv[i], "--clk-track") && i + 1 < argc) {
            unsigned long long v;
            if (parse_number(argv[++i], 600, &v) != 0) { usage(); return 2; }
            clk_track_sec = (int)v;
        }
        else if (!strcmp(argv[i], "--rated-tdp-set") && i + 1 < argc) {
            if (parse_rated_tdp_set(argv[++i], &rtdp_client, &rtdp_action) != 0) {
                fprintf(stderr, "invalid --rated-tdp-set spec (want CLIENT:ACTION)\n");
                return 2;
            }
            rated_tdp_set = 1;
        }
        else if (!strcmp(argv[i], "--service") && i + 1 < argc) {
            const char *v = argv[++i];
            if (!strcmp(v, "nvrm")) nvx_service_pref = 1;
            else if (!strcmp(v, "nvaccel")) nvx_service_pref = 2;
            else if (!strcmp(v, "auto")) nvx_service_pref = 0;
            else { fprintf(stderr, "invalid --service (want nvrm|nvaccel|auto)\n"); return 2; }
        }
        else if (!strcmp(argv[i], "--quiet")) quiet = 1;
        else if (!strcmp(argv[i], "--help")) { usage(); return 0; }
        else { usage(); return 2; }
    }

    if (!*stages || strspn(stages, "012345") != strlen(stages)) {
        fprintf(stderr, "invalid --stages set\n"); return 2;
    }
    if (rated_tdp_set && !want_write) {
        fprintf(stderr, "--rated-tdp-set is a performance write and also requires "
                        "--write (it is refused before opening the driver)\n");
        return 2;
    }
    if (NVX_IDLE_ONLY && (want_write || strpbrk(stages, "35"))) {
        fprintf(stderr, "idle-only build refuses --write and stages 3/5 "
                        "before opening the driver\n");
        return 2;
    }

    memset(&g_ctx, 0, sizeof g_ctx);
    g_ctx.ctl_fd = g_ctx.dev_fd = -1;
    g_ctx.quiet = quiet;
    if (jsonl_path) {
        g_ctx.jsonl = fopen(jsonl_path, "a");
        if (!g_ctx.jsonl) { perror("jsonl"); return 2; }
    }

    signal(SIGINT, on_signal);
    signal(SIGTERM, on_signal);
    atexit(nvx_restore_run);

    printf("== probe-macos-rm ==\n");

    int rc = 0;
    int gated3 = 0;
    int ran2 = 0;
    for (const char *p = stages; *p; p++) {
        switch (*p) {
        case '0':
            if (ensure_transport() != 0) { rc = 1; goto out; }
            break;
        case '1':
            if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
            break;
        case '2':
            if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
            if (!ran2 && nvx_stage2(&g_ctx) != 0) { rc = 1; goto out; }
            ran2 = 1;
            break;
        case '3':
            if (!want_write) {
                nvx_emit(&g_ctx, "V3", "writes", 0, NVX_V_GATED,
                         "write tests need --write (they touch perf state; "
                         "restore-on-exit is best-effort)");
                gated3 = 1;
                break;
            }
            if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
            nvx_restore_arm(&g_ctx);
            if (nvx_stage3(&g_ctx) != 0) { rc = 1; goto out; }
            break;
        case '4':
            if (sample_sec <= 0) sample_sec = 10;
            if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
            if (!ran2 && nvx_stage2(&g_ctx) != 0) { rc = 1; goto out; }
            ran2 = 1;
            nvx_stage4(&g_ctx, sample_sec);
            break;
        case '5':
            if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
            if (nvx_stage5(&g_ctx) != 0) rc = 1;
            break;
        default:
            fprintf(stderr, "unknown stage '%c'\n", *p);
            return 2;
        }
    }

    if (tierb_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_tierb_read(&g_ctx) != 0) rc = 1;
    }

    if (rated_tdp_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_rated_tdp_read(&g_ctx) != 0) rc = 1;
    }

    if (thermal_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_thermal_read(&g_ctx) != 0) rc = 1;
    }

    if (pm_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_pm_read(&g_ctx) != 0) rc = 1;
    }

    if (perf_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_perf_read(&g_ctx) != 0) rc = 1;
    }

    if (reg_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_reg_read(&g_ctx) != 0) rc = 1;
    }

    if (clklevel_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clk_level_read(&g_ctx) != 0) rc = 1;
    }

    if (clkdom_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clkdom_read(&g_ctx) != 0) rc = 1;
    }
    if (clkctl_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clkctl_read(&g_ctx) != 0) rc = 1;
    }
    if (clkctl_hex) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clkctl_hex(&g_ctx) != 0) rc = 1;
    }
    if (clkctl_offsweep) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clkctl_offsweep(&g_ctx) != 0) rc = 1;
    }

    if (clkvf_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clk_read(&g_ctx) != 0) rc = 1;
    }
    if (clkdoc_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clk_doc(&g_ctx) != 0) rc = 1;
    }
    if (clkobj_read) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage2_clk_obj(&g_ctx) != 0) rc = 1;
    }

    if (clk_track_sec > 0) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (!ran2 && nvx_stage2(&g_ctx) != 0) { rc = 1; goto out; }
        if (nvx_stage4_clk_track(&g_ctx, clk_track_sec) != 0) rc = 1;
    }

    if (rated_tdp_set) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (nvx_stage3_rated_tdp_set(&g_ctx, rtdp_client, rtdp_action) != 0) rc = 1;
    }

    if (sample_sec > 0 && !strchr(stages, '4')) {
        if (ensure_chain(gpuId, explicit_gpu) != 0) { rc = 1; goto out; }
        if (!ran2 && nvx_stage2(&g_ctx) != 0) { rc = 1; goto out; }
        nvx_stage4(&g_ctx, sample_sec);
    }

out:
    nvx_restore_run();
    /* teardown: keep the RUSD mapping alive until the process dies; the kext
     * reclaims it with the connection. Close connections last. */
    if (g_ctx.dev_fd >= 0) nvx_close(g_ctx.dev_fd);
    if (g_ctx.ctl_fd >= 0) nvx_close(g_ctx.ctl_fd);

    if (rc == 0 && gated3) rc = 3;
    printf("== done (rc %d) ==\n", rc);
    if (g_ctx.jsonl) fclose(g_ctx.jsonl);
    return rc;
}
