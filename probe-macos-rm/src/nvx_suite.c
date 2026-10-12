#include "nvx_suite.h"
#include "nvx_defs.h"
#include "nvx_xnu.h"
#include "nvx_rm.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#include <unistd.h>
#include <time.h>

#include "nvos.h"
#include "nvstatus.h"
#include "nv-ioctl.h"
#include "nv_escape.h"
#include "nv-unix-nvos-params-wrappers.h"
#include "class/cl0080.h"
#include "class/cl2080.h"
#include "class/cl00de.h"
#include "ctrl/ctrl0000/ctrl0000gpu.h"
#include "ctrl/ctrl0080/ctrl0080gpu.h"
#include "ctrl/ctrl2080/ctrl2080fb.h"
#include "ctrl/ctrl2080/ctrl2080clk.h"
#include "ctrl/ctrl2080/ctrl2080perf.h"
#include "ctrl/ctrl2080/ctrl2080gpumon.h"
#include "ctrl/ctrl2080/ctrl2080internal.h"

/* ------------------------------------------------------------------ */
/* result plumbing                                                     */
/* ------------------------------------------------------------------ */

static const char *verdict_name(int v)
{
    switch (v) {
    case NVX_V_OK:       return "OK";
    case NVX_V_REJECTED: return "REJECTED";
    case NVX_V_TRANSPORT:return "TRANSPORT";
    case NVX_V_SKIP:     return "SKIP";
    case NVX_V_GATED:    return "GATED";
    case NVX_V_HARD:     return "HARD";
    default:             return "?";
    }
}

static void jsonl_escape_into(char *out, size_t cap, const char *s)
{
    size_t o = 0;
    for (const char *p = s; *p && o + 6 < cap; p++) {
        unsigned char ch = (unsigned char)*p;
        if (ch == '"' || ch == '\\') { out[o++] = '\\'; out[o++] = ch; }
        else if (ch == '\n') { out[o++] = '\\'; out[o++] = 'n'; }
        else if (ch < 0x20)  { out[o++] = ' '; }
        else                  { out[o++] = ch; }
    }
    out[o] = 0;
}

void nvx_emit(nvx_ctx *c, const char *id, const char *name,
              NvU32 status, int verdict, const char *fmt, ...)
{
    nvx_res_t r;
    memset(&r, 0, sizeof r);
    snprintf(r.id, sizeof r.id, "%s", id);
    snprintf(r.name, sizeof r.name, "%s", name);
    r.status = status;
    r.verdict = verdict;
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(r.detail, sizeof r.detail, fmt, ap);
    va_end(ap);

    if (!c->quiet)
        printf("[%s] %-7s %-34s status 0x%08x (%s)  %s\n",
               verdict_name(verdict), id, name, status,
               nvx_status_name(status), r.detail);

    if (c->jsonl) {
        char det[300], nm[60];
        jsonl_escape_into(det, sizeof det, r.detail);
        jsonl_escape_into(nm, sizeof nm, name);
        fprintf(c->jsonl,
                "{\"time\":%lld,\"id\":\"%s\",\"name\":\"%s\",\"status\":\"0x%08x\","
                "\"statusName\":\"%s\",\"verdict\":\"%s\",\"detail\":\"%s\"}\n",
                (long long)time(NULL), id, nm, status,
                nvx_status_name(status), verdict_name(verdict), det);
        fflush(c->jsonl);
    }
}

/* ------------------------------------------------------------------ */
/* RUSD helpers                                                        */
/* ------------------------------------------------------------------ */

/* Seq-lock read of one RUSD field (same discipline as cl00de.h macros). */
static int rusd_copy(const volatile NvU64 *ts, const void *field, void *dst, size_t sz)
{
    NvU64 t0 = *ts;
    if (t0 == RUSD_TIMESTAMP_INVALID || t0 == RUSD_TIMESTAMP_WRITE_IN_PROGRESS)
        return -1;
    memcpy(dst, (const void *)field, sz);
    NvU64 t1 = *ts;
    return (t0 == t1 && RUSD_SEQ_DATA_VALID(t0)) ? 0 : -1;
}

/* ------------------------------------------------------------------ */
/* shared sampling snapshot                                            */
/* ------------------------------------------------------------------ */

static NvU32 ctl_pstate(nvx_ctx *c, NvU32 *pstate)
{
    NV2080_CTRL_PERF_GET_CURRENT_PSTATE_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                              NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE, &p, sizeof p);
    if (st == 0) *pstate = p.currPstate;
    return st;
}

static NvU32 pmumon_tail(nvx_ctx *c, NvU32 *gpcKHz, NvU32 *dramKHz)
{
    static NV2080_CTRL_CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES_PARAMS p; /* ~4 KB */
    memset(&p, 0, sizeof p);
    NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                              NV2080_CTRL_CMD_CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES,
                              &p, sizeof p);
    if (st == 0 && p.super.numSamples > 0) {
        NV2080_CTRL_CLK_PMUMON_CLK_DOMAINS_SAMPLE *s =
            &p.samples[p.super.numSamples - 1];
        *gpcKHz  = s->gpcClkFreqKHz;
        *dramKHz = s->dramClkFreqKHz;
    }
    return st;
}

void nvx_sample_once(nvx_ctx *c, const char *tag)
{
    NvU32 pstate = 0xffffffff, gpc = 0, dram = 0;
    NvU32 s1 = ctl_pstate(c, &pstate);
    NvU32 s2 = pmumon_tail(c, &gpc, &dram);

    long tempC_x256 = -256, instMW = -1, enforcedMW = -1, rpm = -1, gpuBusy = -1;
    int haveRusd = 0;

    if (c->rusd) {
        NV00DE_SHARED_DATA *sd = (NV00DE_SHARED_DATA *)c->rusd;
        RUSD_TEMPERATURE t;
        RUSD_INST_POWER_USAGE pw;
        RUSD_POWER_LIMITS pl;
        RUSD_FAN_COOLER_STATUS fan;
        RUSD_PERF_DEVICE_UTILIZATION util;
        if (rusd_copy(&sd->temperatures[RUSD_TEMPERATURE_TYPE_GPU].lastModifiedTimestamp,
                      &sd->temperatures[RUSD_TEMPERATURE_TYPE_GPU], &t, sizeof t) == 0)
            { tempC_x256 = (long)t.temperature; haveRusd = 1; } /* NvTemp = SFXP24.8 */
        if (rusd_copy(&sd->instPowerUsage.lastModifiedTimestamp,
                      &sd->instPowerUsage, &pw, sizeof pw) == 0)
            instMW = (long)pw.info.instGpuPower;
        if (rusd_copy(&sd->powerLimitGpu.lastModifiedTimestamp,
                      &sd->powerLimitGpu, &pl, sizeof pl) == 0)
            enforcedMW = (long)pl.info.enforcedmW;
        if (rusd_copy(&sd->fanCoolerStatus.lastModifiedTimestamp,
                      &sd->fanCoolerStatus, &fan, sizeof fan) == 0) {
            rpm = 0;
            for (unsigned i = 0; i < RUSD_FAN_COOLER_MAX_COOLERS; i++)
                if ((long)fan.info.rpmCurr[i] > rpm) rpm = (long)fan.info.rpmCurr[i];
        }
        if (rusd_copy(&sd->perfDevUtil.lastModifiedTimestamp,
                      &sd->perfDevUtil, &util, sizeof util) == 0)
            gpuBusy = (long)util.info.gpuPercentBusy;
    }

    if (!c->quiet)
        printf("  %-6s pstate %u  gpc %u kHz  dram %u kHz  temp %ld.%ld C  "
               "pwr %ld mW  lim %ld mW  fan %ld rpm  busy %ld%%\n",
               tag,
               s1 == 0 ? pstate : 0xffffffffu,
               s2 == 0 ? gpc : 0, s2 == 0 ? dram : 0,
               tempC_x256 >> 8, (tempC_x256 & 0xff) * 100 / 256,
               instMW, enforcedMW, rpm, gpuBusy);

    if (c->jsonl)
        fprintf(c->jsonl,
                "{\"time\":%lld,\"sample\":{\"tag\":\"%s\",\"pstate\":%u,"
                "\"gpcKHz\":%u,\"dramKHz\":%u,\"tempFx256\":%ld,\"instmW\":%ld,"
                "\"limitmW\":%ld,\"fanRpm\":%ld,\"gpuBusy\":%ld,"
                "\"rusd\":%s}}\n",
                (long long)time(NULL), tag,
                s1 == 0 ? pstate : 0xffffffffu,
                s2 == 0 ? gpc : 0, s2 == 0 ? dram : 0,
                tempC_x256, instMW, enforcedMW, rpm, gpuBusy,
                haveRusd ? "true" : "false");
}

/* ------------------------------------------------------------------ */
/* stage 0 — transport                                                 */
/* ------------------------------------------------------------------ */

int nvx_stage0(nvx_ctx *c)
{
    c->ctl_fd = nvx_open(1);
    if (c->ctl_fd < 0) {
        nvx_emit(c, "V0.1", "open-ctl", 0, NVX_V_HARD,
                 "no NVAccel/NVRM user client — driver loaded? (kmutil showloaded | grep nullmoth)");
        return -1;
    }
    nvx_emit(c, "V0.1", "open-ctl", 0, NVX_V_OK, "ctl connection opened (fd %d)", c->ctl_fd);

    c->dev_fd = nvx_open(0);
    if (c->dev_fd < 0) {
        nvx_emit(c, "V0.2", "open-dev", 0, NVX_V_SKIP, "no device connection; RUSD map tests limited");
    } else {
        nvx_emit(c, "V0.2", "open-dev", 0, NVX_V_OK, "device connection opened (fd %d)", c->dev_fd);
    }

    /* NV_ESC_CARD_INFO needs the ctl connection; also proves esc works. */
    nv_ioctl_card_info_t ci;
    memset(&ci, 0, sizeof ci);
    if (nvx_esc(c->ctl_fd, NV_ESC_CARD_INFO, &ci, sizeof ci) != 0) {
        nvx_emit(c, "V0.3", "card-info", 0, NVX_V_TRANSPORT, "esc rejected");
        return -1;
    }
    if (!ci.valid) {
        nvx_emit(c, "V0.3", "card-info", 0, NVX_V_HARD,
                 "card info invalid — adapter never came up. Boot without -nvrmnogo, "
                 "or run 'nvrmctl go 2' (see feasibility report §3.2)");
        return -1;
    }
    c->gpuId = ci.gpu_id;
    nvx_emit(c, "V0.3", "card-info", 0, NVX_V_OK,
             "gpu_id 0x%x  reg 0x%llx/%lluMB  fb 0x%llx/%lluMB  irq %u",
             ci.gpu_id,
             (unsigned long long)ci.reg_address, (unsigned long long)(ci.reg_size >> 20),
             (unsigned long long)ci.fb_address, (unsigned long long)(ci.fb_size >> 20),
             ci.interrupt_line);

    if (c->dev_fd >= 0) {
        nv_ioctl_register_fd_t rf;
        memset(&rf, 0, sizeof rf);
        rf.ctl_fd = c->ctl_fd;
        NvU32 st = nvx_esc(c->dev_fd, NV_ESC_REGISTER_FD, &rf, sizeof rf) == 0 ? 0 : 0xffffffffu;
        nvx_emit(c, "V0.4", "register-fd", st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 st == 0 ? "device connection bound to ctl fd" : "REGISTER_FD refused");
    }
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 1 — RM object chain                                           */
/* ------------------------------------------------------------------ */

int nvx_stage1(nvx_ctx *c, NvU32 gpuId)
{
    NvU32 st = 0;
    NvHandle cl = nvx_rm_alloc(c->ctl_fd, 0, 0, 0xcafe0000, NVX_CLASS_ROOT_CLIENT,
                               NULL, 0, &st);
    if (!cl) {
        nvx_emit(c, "V1.1", "alloc-root-client", st, NVX_V_HARD,
                 "NV01_ROOT_CLIENT (0x41) rejected");
        return -1;
    }
    c->hClient = cl;
    nvx_emit(c, "V1.1", "alloc-root-client", st, NVX_V_OK, "handle 0x%08x", cl);

    NV0000_CTRL_GPU_GET_ATTACHED_IDS_PARAMS ga;
    memset(&ga, 0, sizeof ga);
    st = nvx_rm_control(c->ctl_fd, cl, cl, NV0000_CTRL_CMD_GPU_GET_ATTACHED_IDS,
                        &ga, sizeof ga);
    nvx_emit(c, "V1.2", "get-attached-ids", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "attached gpuIds 0x%x 0x%x" : "-",
             st == 0 ? ga.gpuIds[0] : 0, st == 0 ? ga.gpuIds[1] : 0);

    NV0000_CTRL_GPU_ATTACH_IDS_PARAMS at;
    memset(&at, 0, sizeof at);
    at.gpuIds[0] = gpuId;
    at.gpuIds[1] = NV0000_CTRL_GPU_INVALID_ID;
    st = nvx_rm_control(c->ctl_fd, cl, cl, NV0000_CTRL_CMD_GPU_ATTACH_IDS, &at, sizeof at);
    if (st != 0) {
        nvx_emit(c, "V1.3", "attach-ids", st, NVX_V_HARD, "gpuId 0x%x", gpuId);
        return -1;
    }
    nvx_emit(c, "V1.3", "attach-ids", st, NVX_V_OK, "gpuId 0x%x", gpuId);

    NV0080_ALLOC_PARAMETERS dp;
    memset(&dp, 0, sizeof dp);
    dp.deviceId = 0;
    dp.hClientShare = cl;
    NvHandle dv = nvx_rm_alloc(c->ctl_fd, cl, cl, 0xcafe0001, NV01_DEVICE_0,
                               &dp, sizeof dp, &st);
    if (!dv) {
        nvx_emit(c, "V1.4", "alloc-device", st, NVX_V_HARD, "NV01_DEVICE_0 rejected");
        return -1;
    }
    c->hDevice = dv;
    nvx_emit(c, "V1.4", "alloc-device", st, NVX_V_OK, "handle 0x%08x", dv);

    NV2080_ALLOC_PARAMETERS sp;
    memset(&sp, 0, sizeof sp);
    NvHandle sd = nvx_rm_alloc(c->ctl_fd, cl, dv, 0xcafe0002, NV20_SUBDEVICE_0,
                               &sp, sizeof sp, &st);
    if (!sd) {
        nvx_emit(c, "V1.5", "alloc-subdevice", st, NVX_V_HARD, "NV20_SUBDEVICE_0 rejected");
        return -1;
    }
    c->hSubdevice = sd;
    nvx_emit(c, "V1.5", "alloc-subdevice", st, NVX_V_OK, "handle 0x%08x", sd);

    /* Device-scope control: proves the device object dispatches controls. */
    static NV0080_CTRL_GPU_GET_CLASSLIST_V2_PARAMS cls; /* 804 B */
    memset(&cls, 0, sizeof cls);
    st = nvx_rm_control(c->ctl_fd, cl, dv, NV0080_CTRL_CMD_GPU_GET_CLASSLIST_V2,
                        &cls, sizeof cls);
    nvx_emit(c, "V1.6", "gpu-classlist-v2", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "%u classes (NVK-proven path)" : "-", st == 0 ? cls.numClasses : 0);
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 2 — read-only battery                                         */
/* ------------------------------------------------------------------ */

#define SUBCTL(c, cmd, params) \
    nvx_rm_control((c)->ctl_fd, (c)->hClient, (c)->hSubdevice, (cmd), (params), sizeof *(params))

static void item_fb_info(nvx_ctx *c)
{
    static const NvU32 want[] = {
        NV2080_CTRL_FB_INFO_INDEX_TOTAL_RAM_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_USABLE_RAM_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_BAR1_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_BAR1_AVAIL_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_L2CACHE_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_RAM_TYPE,
        NV2080_CTRL_FB_INFO_INDEX_HEAP_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_MAPPABLE_HEAP_SIZE,
    };
    NV2080_CTRL_FB_GET_INFO_V2_PARAMS p;
    memset(&p, 0, sizeof p);
    const NvU32 n = (NvU32)(sizeof want / sizeof want[0]);
    for (NvU32 i = 0; i < n; i++) p.fbInfoList[i].index = want[i];
    p.fbInfoListSize = n;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_FB_GET_INFO_V2, &p);
    if (st == 0) {
        nvx_emit(c, "V2.1", "fb-get-info-v2", st, NVX_V_OK,
                 "ram %uMB usable %uMB bar1 %uMB avail %uMB l2 %uKB heap %uKB mappable %uKB ramtype 0x%x (NVK-proven path)",
                 p.fbInfoList[0].data >> 10, p.fbInfoList[1].data >> 10,
                 p.fbInfoList[2].data >> 10, p.fbInfoList[3].data >> 10,
                 p.fbInfoList[4].data, p.fbInfoList[6].data >> 10,
                 p.fbInfoList[7].data >> 10, p.fbInfoList[5].data);
    } else {
        nvx_emit(c, "V2.1", "fb-get-info-v2", st, NVX_V_REJECTED, "-");
    }
}

static void item_fb_stats(nvx_ctx *c)
{
    NV2080_CTRL_CMD_FB_STATS_GET_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_FB_STATS_GET, &p);
    /* Layout beyond status is not interpreted in v1; record raw first dword. */
    nvx_emit(c, "V2.2", "fb-stats-get", st, st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "raw[0..1]=0x%08x 0x%08x", ((NvU32 *)&p)[0], ((NvU32 *)&p)[1]);
}

static void item_powerstate_get(nvx_ctx *c)
{
    NV2080_CTRL_PERF_GET_POWERSTATE_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_POWERSTATE, &p);
    nvx_emit(c, "V2.3", "perf-get-powerstate", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "powerState %u (0=AC, 1=battery)" : "-", p.powerStateInfo.powerState);
}

static void item_rated_tdp_get(nvx_ctx *c)
{
    NV2080_CTRL_PERF_RATED_TDP_CONTROL_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &p);
    nvx_emit(c, "V2.4", "rated-tdp-get-control", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "raw[0..2]=0x%08x 0x%08x 0x%08x (client/input/vpstate echo)" : "-",
             ((NvU32 *)&p)[0], ((NvU32 *)&p)[1], ((NvU32 *)&p)[2]);
}

static void item_edpp_get(nvx_ctx *c, NvU32 *curr_out)
{
    NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO, &p);
    if (st == 0 && curr_out) *curr_out = p.limitCurr;
    nvx_emit(c, "V2.5", "edpp-limit-info", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "min %u  rated %u  max %u  curr %u mW (power-limit read face)"
                     : "-",
             st == 0 ? p.limitMin : 0, st == 0 ? p.limitRated : 0,
             st == 0 ? p.limitMax : 0, st == 0 ? p.limitCurr : 0);
}

static void item_gpumon(nvx_ctx *c)
{
    static NV2080_CTRL_PERF_GET_GPUMON_PERFMON_UTIL_SAMPLES_V2_PARAMS p;
    memset(&p, 0, sizeof p);
    p.type = NV2080_CTRL_GPUMON_SAMPLE_TYPE_PERFMON_UTIL;
    p.bufSize = (NvU32)sizeof(NV2080_CTRL_PERF_GPUMON_PERFMON_UTIL_SAMPLE);
    p.count = NV2080_CTRL_PERF_GPUMON_SAMPLE_COUNT_PERFMON_UTIL;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_GPUMON_PERFMON_UTIL_SAMPLES_V2, &p);
    NvU32 grRaw = 0;
    if (st == 0 && p.tracker > 0 && p.tracker <= p.count) {
        /* newest entry sits before the tracker in the ring; base(u64) is
         * dwords 0..1, fb 2..3, gr 4..5 — dword 5 is the GR util value on
         * the RUSD layout; print raw for on-device confirmation */
        NvU32 *dw = (NvU32 *)&p.samples[(p.tracker - 1) % p.count];
        grRaw = dw[5];
    }
    nvx_emit(c, "V2.6", "gpumon-util-samples-v2", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "tracker %u  grRaw=0x%08x" : "-",
             st == 0 ? p.tracker : 0, st == 0 ? grRaw : 0);
}

static void item_pmumon(nvx_ctx *c)
{
    NvU32 gpc = 0, dram = 0;
    NvU32 st = pmumon_tail(c, &gpc, &dram);
    nvx_emit(c, "V2.7", "pmumon-clk-samples", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "tail gpc %u kHz  dram %u kHz" : "-", gpc, dram);
}

static void item_pstate(nvx_ctx *c)
{
    NvU32 ps = 0xffffffff;
    NvU32 st = ctl_pstate(c, &ps);
    nvx_emit(c, "V2.8", "perf-current-pstate", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "P%u" : "-", st == 0 ? ps : 0xffffffffu);
}

static void item_legacy_probes(nvx_ctx *c)
{
    /* Tier-B GET probes: IDs are campaign-sourced (see nvx_defs.h), layouts
     * partially decoded. Read-only; verdicts measure firmware acceptance. */
    static uint8_t buf[32 * NVX_VFPOINTS_RECORD_STRIDE];

    memset(buf, 0, NVX_RAILS_INFO_STRIDE);
    NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                              NVX_RM_CMD_RAILS_GET_INFO, buf, NVX_RAILS_INFO_STRIDE);
    nvx_emit(c, "V2.9", "rails-get-info(legacy)", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "type@0x44=0x%02x class@0x77=0x%02x" : "-",
             buf[0x44], buf[0x77]);

    memset(buf, 0, NVX_RAILS_STATUS_STRIDE);
    st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                        NVX_RM_CMD_RAILS_GET_STATUS, buf, NVX_RAILS_STATUS_STRIDE);
    nvx_emit(c, "V2.10", "rails-get-status(legacy)", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "type@88=0x%02x" : "-", buf[88]);

    memset(buf, 0, sizeof buf);
    st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                        NVX_RM_CMD_VFPOINTS_GET, buf, 10 * NVX_VFPOINTS_RECORD_STRIDE);
    nvx_emit(c, "V2.11", "vfpoints-get(legacy)", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "rec0 raw 0x%08x 0x%08x 0x%08x" : "-",
             st == 0 ? ((NvU32 *)buf)[0] : 0, st == 0 ? ((NvU32 *)buf)[1] : 0,
             st == 0 ? ((NvU32 *)buf)[2] : 0);
}

static void item_rusd(nvx_ctx *c)
{
    if (c->dev_fd < 0) {
        nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_SKIP, "no device connection");
        return;
    }
    NvU32 st = 0;
    NvHandle hRusd = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hSubdevice,
                                  0xcafe0003, NVX_CLASS_USER_SHARED_DATA,
                                  NULL, 0, &st); /* alloc without poll params first */
    if (!hRusd) {
        nvx_emit(c, "V2.12", "rusd-page", st, NVX_V_REJECTED,
                 "NV00DE alloc rejected");
        return;
    }

    /* retry with poll mask so polled sections are populated */
    NvHandle hRusd2 = 0;
    NV00DE_ALLOC_PARAMETERS ap;
    memset(&ap, 0, sizeof ap);
    ap.polledDataMask = NVX_RUSD_POLL_MASK;
    hRusd2 = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hSubdevice,
                          0xcafe0004, NVX_CLASS_USER_SHARED_DATA,
                          &ap, sizeof ap, &st);
    NvHandle use = hRusd2 ? hRusd2 : hRusd;

    NvU64 len = 0;
    void *page = nvx_map_memory(c->ctl_fd, c->dev_fd, c->hClient, c->hSubdevice,
                                use, 0x1000, &len);
    if (!page) {
        nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_REJECTED,
                 "NV00DE alloc ok (0x%08x) but MAP_MEMORY/mmap failed — RUSD read path unsupported on kext",
                 use);
        nvx_rm_free(c->ctl_fd, c->hClient, c->hSubdevice, hRusd2);
        nvx_rm_free(c->ctl_fd, c->hClient, c->hSubdevice, hRusd);
        return;
    }
    c->rusd = page;
    c->rusd_len = len;
    nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_OK,
             "NV00DE 0x%08x mapped at %p len %lluKB — poll mask 0x%x (temp/pwr/fan/clock path alive)",
             use, page, (unsigned long long)(len >> 10), NVX_RUSD_POLL_MASK);

    /* leave both objects alive for stage 3/4 sampling; freed in main teardown */
    if (hRusd2 && hRusd) nvx_rm_free(c->ctl_fd, c->hClient, c->hSubdevice, hRusd);
}

int nvx_stage2(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2", "battery", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_fb_info(c);
    item_fb_stats(c);
    item_pstate(c);
    item_powerstate_get(c);
    item_rated_tdp_get(c);
    item_edpp_get(c, NULL);
    item_gpumon(c);
    item_pmumon(c);
    item_legacy_probes(c);
    item_rusd(c);
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 3 — gated write tests                                         */
/* ------------------------------------------------------------------ */

static nvx_ctx *g_boost_ctx;

static void boost_clear(nvx_ctx *c)
{
    NV2080_CTRL_PERF_BOOST_PARAMS bp;
    memset(&bp, 0, sizeof bp);
    bp.flags = NV2080_CTRL_PERF_BOOST_FLAGS_CMD_CLEAR |
               (NV2080_CTRL_PERF_BOOST_FLAGS_CUDA_YES << 4);
    bp.duration = NV2080_CTRL_PERF_BOOST_DURATION_INFINITE;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_BOOST, &bp);
    printf("[..] boost clear -> 0x%x (%s)\n", st, nvx_status_name(st));
}

void nvx_restore_arm(nvx_ctx *c) { g_boost_ctx = c; }

void nvx_restore_run(void)
{
    if (g_boost_ctx && g_boost_ctx->hSubdevice) {
        printf("[..] restoring: clearing PERF_BOOST latched state\n");
        boost_clear(g_boost_ctx);
        g_boost_ctx = NULL;
    }
}

int nvx_stage3(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V3", "writes", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    /* --- V3.1 PERF_BOOST cycle (the NVK-proven write) --- */
    {
        nvx_emit(c, "V3.1", "perf-boost-cycle", 0, NVX_V_OK, "sampling baseline...");
        nvx_sample_once(c, "pre");

        NV2080_CTRL_PERF_BOOST_PARAMS bp;
        memset(&bp, 0, sizeof bp);
        bp.flags = NV2080_CTRL_PERF_BOOST_FLAGS_CMD_BOOST_TO_MAX |
                   (NV2080_CTRL_PERF_BOOST_FLAGS_CUDA_YES << 4);
        bp.duration = NV2080_CTRL_PERF_BOOST_DURATION_INFINITE;
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_BOOST, &bp);
        nvx_emit(c, "V3.1", "perf-boost-cycle", st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 st == 0 ? "BOOST_TO_MAX accepted by GSP — the §3.4 OS-privilege "
                           "question is answered YES" : "-");
        if (st == 0) {
            for (int i = 0; i < 5; i++) { sleep(1); nvx_sample_once(c, "boost"); }
            boost_clear(c);
            sleep(1);
            nvx_sample_once(c, "clear");
        }
    }

    /* --- V3.2 power-state roundtrip (write-back the value just read) --- */
    {
        NV2080_CTRL_PERF_GET_POWERSTATE_PARAMS g;
        memset(&g, 0, sizeof g);
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_POWERSTATE, &g);
        if (st != 0) {
            nvx_emit(c, "V3.2", "powerstate-roundtrip", st, NVX_V_REJECTED, "GET failed");
        } else {
            NV2080_CTRL_PERF_SET_POWERSTATE_PARAMS s;
            memset(&s, 0, sizeof s);
            s.powerStateInfo.powerState = g.powerStateInfo.powerState;
            st = SUBCTL(c, NV2080_CTRL_CMD_PERF_SET_POWERSTATE, &s);
            NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_POWERSTATE, &g);
            nvx_emit(c, "V3.2", "powerstate-roundtrip", st,
                     st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                     st == 0 ? "SET(current=%u) accepted; verify GET 0x%x"
                             : "-",
                     s.powerStateInfo.powerState, st2 == 0 ? g.powerStateInfo.powerState : 0xffffffffu);
        }
    }

    /* --- V3.3 EDPp limit roundtrip (write-back current limit) --- */
    {
        NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO_PARAMS g;
        memset(&g, 0, sizeof g);
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO, &g);
        if (st != 0) {
            nvx_emit(c, "V3.3", "edpp-roundtrip", st, NVX_V_REJECTED, "GET failed");
        } else {
            NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_UPDATE_EDPP_LIMIT_PARAMS u;
            memset(&u, 0, sizeof u);
            u.bEnable = NV_TRUE;
            u.clientLimit = g.limitCurr;
            st = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_UPDATE_EDPP_LIMIT, &u);
            NvU32 st2 = 0;
            NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO_PARAMS g2;
            memset(&g2, 0, sizeof g2);
            st2 = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PMGR_PFM_REQ_HNDLR_GET_EDPP_LIMIT_INFO, &g2);
            nvx_emit(c, "V3.3", "edpp-roundtrip", st,
                     st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                     st == 0 ? "UPDATE(curr=%u) accepted; verify GET curr=%u"
                             : "-",
                             u.clientLimit, st2 == 0 ? g2.limitCurr : 0xffffffffu);
        }
    }

    nvx_restore_run();
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 4 — sampling loop                                             */
/* ------------------------------------------------------------------ */

void nvx_stage4(nvx_ctx *c, int seconds)
{
    if (!c->hSubdevice) return;
    printf("-- sampling %d s (1 Hz)\n", seconds);
    for (int i = 0; i < seconds; i++) {
        nvx_sample_once(c, "loop");
        sleep(1);
    }
}
