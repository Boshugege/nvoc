/*
 * probe-macos-rm — entry point.
 *
 * Default run is READ-ONLY (stages 0,1,2). Stage 3 (write tests) is refused
 * without --write. --sample N runs the telemetry loop after the stages.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include "nvtypes.h"
#include "nvx_suite.h"
#include "nvx_xnu.h"

static nvx_ctx g_ctx;

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
        "  --stages <set>   stage subset, e.g. 012 (default 012); 3=writes, 4=loop\n"
        "  --write          allow stage-3 write tests (boost / powerstate / EDPp)\n"
        "  --sample <sec>   run the 1 Hz telemetry loop for <sec> seconds\n"
        "  --jsonl <file>   also write every result/sample as JSON lines\n"
        "  --gpu <id>       attach explicit gpuId (default: first from CARD_INFO)\n"
        "  --quiet          suppress text detail, keep JSONL\n"
        "  --help\n");
}

int main(int argc, char **argv)
{
    const char *stages = "012";
    const char *jsonl_path = NULL;
    int want_write = 0, sample_sec = 0, quiet = 0;
    NvU32 gpuId = 0;

    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--stages") && i + 1 < argc) stages = argv[++i];
        else if (!strcmp(argv[i], "--write")) want_write = 1;
        else if (!strcmp(argv[i], "--sample") && i + 1 < argc) sample_sec = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--jsonl") && i + 1 < argc) jsonl_path = argv[++i];
        else if (!strcmp(argv[i], "--gpu") && i + 1 < argc) gpuId = (NvU32)strtoul(argv[++i], NULL, 0);
        else if (!strcmp(argv[i], "--quiet")) quiet = 1;
        else if (!strcmp(argv[i], "--help")) { usage(); return 0; }
        else { usage(); return 2; }
    }

    memset(&g_ctx, 0, sizeof g_ctx);
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
    int ran3 = 0;
    for (const char *p = stages; *p; p++) {
        switch (*p) {
        case '0':
            if (nvx_stage0(&g_ctx) != 0) { rc = 1; goto out; }
            break;
        case '1':
            if (!g_ctx.ctl_fd && nvx_stage0(&g_ctx) != 0) { rc = 1; goto out; }
            if (nvx_stage1(&g_ctx, gpuId ? gpuId : g_ctx.gpuId) != 0) { rc = 1; goto out; }
            break;
        case '2':
            if (!g_ctx.hSubdevice && nvx_stage1(&g_ctx, gpuId ? gpuId : g_ctx.gpuId) != 0) { rc = 1; goto out; }
            nvx_stage2(&g_ctx);
            break;
        case '3':
            if (!want_write) {
                nvx_emit(&g_ctx, "V3", "writes", 0, NVX_V_GATED,
                         "write tests need --write (they touch perf state; "
                         "restore-on-exit is best-effort)");
                ran3 = 1;
                break;
            }
            if (!g_ctx.hSubdevice && nvx_stage1(&g_ctx, gpuId ? gpuId : g_ctx.gpuId) != 0) { rc = 1; goto out; }
            nvx_restore_arm(&g_ctx);
            nvx_stage3(&g_ctx);
            ran3 = 1;
            break;
        case '4':
            if (sample_sec <= 0) sample_sec = 10;
            if (!g_ctx.hSubdevice && nvx_stage1(&g_ctx, gpuId ? gpuId : g_ctx.gpuId) != 0) { rc = 1; goto out; }
            nvx_stage4(&g_ctx, sample_sec);
            break;
        default:
            fprintf(stderr, "unknown stage '%c'\n", *p);
            return 2;
        }
    }

    if (sample_sec > 0 && !strchr(stages, '4')) {
        if (!g_ctx.hSubdevice) nvx_stage1(&g_ctx, gpuId ? gpuId : g_ctx.gpuId);
        nvx_stage4(&g_ctx, sample_sec);
    }

out:
    /* teardown: keep the RUSD mapping alive until the process dies; the kext
     * reclaims it with the connection. Close connections last. */
    if (g_ctx.dev_fd >= 0) nvx_close(g_ctx.dev_fd);
    if (g_ctx.ctl_fd >= 0) nvx_close(g_ctx.ctl_fd);

    if (rc == 0 && ran3) rc = 3;   /* gated skips are not a hard failure */
    printf("== done (rc %d) ==\n", rc);
    if (g_ctx.jsonl) fclose(g_ctx.jsonl);
    return rc;
}
