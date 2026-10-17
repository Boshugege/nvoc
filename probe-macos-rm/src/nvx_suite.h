/*
 * probe-macos-rm — staged validation suite.
 *
 * Stage 0: IOKit transport (service presence, open, card info, register fd)
 * Stage 1: RM object chain (root client / attach / device / subdevice / classlist)
 * Stage 2: read-only telemetry battery (OGKM in-tree FINN controls + RUSD page
 *          + campaign-sourced legacy GET probes)
 * Stage 3: gated write tests (PERF_BOOST cycle, power-state roundtrip,
 *          EDPp limit roundtrip) — requires --write
 * Stage 4: telemetry sampling loop (feeds JSONL for before/after diffing)
 *
 * Every item emits a nvx_res_t record; main() renders text and JSONL.
 */
#ifndef NVX_SUITE_H
#define NVX_SUITE_H

#include "nvtypes.h"
#include <stdio.h>

#ifndef NVX_IDLE_ONLY
#define NVX_IDLE_ONLY 1
#endif

enum {
    NVX_V_OK = 0,      /* NV_OK, values plausible            */
    NVX_V_REJECTED,    /* call completed, RM returned error  */
    NVX_V_TRANSPORT,   /* IOKit/esc layer failure            */
    NVX_V_SKIP,        /* prerequisite missing               */
    NVX_V_GATED,       /* needs --write                      */
    NVX_V_HARD         /* chain-breaking failure             */
};

typedef struct {
    char id[16];
    char name[48];
    NvU32 status;
    int   verdict;
    char  detail[288];
} nvx_res_t;

typedef struct nvx_ctx {
    int     ctl_fd, dev_fd;
    NvHandle hClient, hDevice, hSubdevice;
    NvU32   gpuId;
    void   *rusd;        /* mapped NV00DE_SHARED_DATA page, or NULL */
    NvU64   rusd_len;
    /* output */
    FILE   *jsonl;
    int     quiet;
} nvx_ctx;

void nvx_emit(nvx_ctx *c, const char *id, const char *name,
              NvU32 status, int verdict, const char *fmt, ...)
              __attribute__((format(printf, 6, 7)));

/* One telemetry snapshot (text line + jsonl "sample" record). */
void nvx_sample_once(nvx_ctx *c, const char *tag);

int nvx_stage0(nvx_ctx *c);                    /* transport                */
int nvx_stage1(nvx_ctx *c, NvU32 gpuId);       /* RM chain                 */
int nvx_stage2(nvx_ctx *c);                    /* read-only battery        */
/* Explicit, read-only Tier-B GET probes (voltage rails + V/F points). Their
 * command IDs are campaign-sourced rather than SDK headers, so they stay out
 * of the default battery and only run when the operator asks for them. */
int nvx_stage2_tierb_read(nvx_ctx *c);
/* Explicit, read-only RATED_TDP GET probe: the one public perf-side read pair
 * (NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL). Never sends its SET twin. */
int nvx_stage2_rated_tdp_read(nvx_ctx *c);
/* Gated write: RATED_TDP arbitration SET with mandatory pre-record, readback
 * and restore-on-exit. Only reachable as `--write --rated-tdp-set C:A`; it is
 * never sent by the default battery, by `--rated-tdp-read`, or by stage 3. */
int nvx_stage3_rated_tdp_set(nvx_ctx *c, NvU32 client, NvU32 action);
/* Explicit, read-only Thermal System V2 GET batch (public opcode interface). */
int nvx_stage2_thermal_read(nvx_ctx *c);
/* Explicit, read-only internal PM/PFM GET probes (PM1, V/P-state, boost). */
int nvx_stage2_pm_read(nvx_ctx *c);
/* Explicit, read-only perf/pmgr read-surface census: the remaining public
 * GETs plus explicit SKIPs for the ones this transport cannot or must not
 * send (kernel-pointer v1 level info, the state-changing GPU_IS_IDLE). */
int nvx_stage2_perf_read(nvx_ctx *c);
/* Read-only pass over the control ids the loaded NVRM *actually registers*,
 * each sent at exactly the paramSize recovered from its NVOC export table
 * (probe-macos-rm/tools/extract-nvrm-method-table.py).  Metadata-driven, so
 * it is not a size scan; SET ids are refused before sending. */
int nvx_stage2_reg_read(nvx_ctx *c);
/* Read-only PERF_GET_LEVEL_INFO_V2 (0x2080200b @780) pass: the one
 * clock-shaped command this build *registers*, queried across the three
 * published levels and the TYPE_OVERCLOCK flag, decoded as per-domain
 * {current, default, min, max} kHz so the clock range has a named source. */
int nvx_stage2_clk_level_read(nvx_ctx *c);
/* Explicit, read-only private ClockClient / VfPoints / PerfVfe GET census:
 * the read half of the same wires the Windows backend uses for clock-domain
 * and V/F reads. Scans candidate paramSize values (the driver answers
 * NV_ERR_INVALID_ARGUMENT for the sizes it does not want) and never sends a
 * documented SET id. */
int nvx_stage2_clk_read(nvx_ctx *c);
/* Explicit, read-only documented-MARSHALLING pass over the same private
 * ClockClient / VfPoints / ClkFreqController / VoltRails read faces:
 * one case per documented (paramSize, dword[0] stamp, input seed) triple
 * from nvapi-rs + the R610 RE docs, instead of the size sweep. The sweep
 * sent zeroed buffers, which cannot satisfy a stamp/mask-validating
 * handler; this pass is what actually tests the documented form. Never
 * sends a documented SET id. */
int nvx_stage2_clk_doc(nvx_ctx *c);
/* Same documented forms as --clkdoc-read, re-sent against each RM object
 * carrier (subdevice / device / client) so "object mismatch" and
 * "bad paramSize" stop looking alike. GETs only. */
int nvx_stage2_clk_obj(nvx_ctx *c);
/* Read-only 1 Hz tracker: RUSD gpc/mem clocks next to the RM 0x20809061/
 * 776 B per-domain table, for the load-vs-idle A/B that decides
 * whether that table is a live reading or a capability list. */
int nvx_stage4_clk_track(nvx_ctx *c, int seconds);
int nvx_stage3(nvx_ctx *c);                    /* gated writes             */
void nvx_stage4(nvx_ctx *c, int seconds);      /* sampling loop            */
int nvx_stage5(nvx_ctx *c);                    /* channel smoke (GPU exec) */

/* Restore path: clear a latched boost if stage3 armed one (atexit/SIGINT). */
void nvx_restore_arm(nvx_ctx *c);
void nvx_restore_run(void);

#endif /* NVX_SUITE_H */
