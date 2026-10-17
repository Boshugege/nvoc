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
#include <stdatomic.h>

#include "nvos.h"
#include "nvstatus.h"
#include "nvmisc.h"
#include "nv-ioctl.h"
#include "nv_escape.h"
#include "nv-unix-nvos-params-wrappers.h"
#include "class/cl0080.h"
#include "class/cl2080.h"
#include "class/cl00de.h"
#include "class/clc361.h"
#include "class/cl2080_notification.h"
#include "ctrl/ctrl0000/ctrl0000gpu.h"
#include "ctrl/ctrl0000/ctrl0000client.h"
#include "ctrl/ctrl0080/ctrl0080gpu.h"
#include "ctrl/ctrl2080/ctrl2080fb.h"
#include "ctrl/ctrl2080/ctrl2080clk.h"
#include "ctrl/ctrl2080/ctrl2080perf.h"
#include "ctrl/ctrl2080/ctrl2080pmgr.h"
#include "ctrl/ctrl2080/ctrl2080thermal.h"
#include "ctrl/ctrl2080/ctrl2080gpumon.h"
#include "ctrl/ctrl2080/ctrl2080internal.h"
#include "ctrl/ctrl2080/ctrl2080bios.h"
#include "ctrl/ctrl2080/ctrl2080bus.h"
#include "ctrl/ctrl2080/ctrl2080i2c.h"
#include "ctrl/ctrla06f/ctrla06fgpfifo.h"
#include "ctrl/ctrlc36f.h"
#include "alloc/alloc_channel.h"

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
    atomic_thread_fence(memory_order_acquire);
    if (t0 == RUSD_TIMESTAMP_INVALID || t0 == RUSD_TIMESTAMP_WRITE_IN_PROGRESS)
        return -1;
    memcpy(dst, (const void *)field, sz);
    atomic_thread_fence(memory_order_acquire);
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
        RUSD_CLK_PUBLIC_DOMAIN_INFOS clocks;
        if (rusd_copy(&sd->clkPublicDomainInfos.lastModifiedTimestamp,
                      &sd->clkPublicDomainInfos, &clocks, sizeof clocks) == 0) {
            gpc = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS].actualClkKHz;
            dram = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_MEMORY].actualClkKHz;
            s2 = 0;
        }
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

    /* RM API version handshake (Linux nv.c CHECK_VERSION_STR semantics). */
    nv_ioctl_rm_api_version_t v;
    memset(&v, 0, sizeof v);
    v.cmd = NV_RM_API_VERSION_CMD_QUERY;
    strncpy(v.versionString, "610.57.04", sizeof v.versionString - 1);
    if (nvx_esc(c->ctl_fd, NV_ESC_CHECK_VERSION_STR, &v, sizeof v) != 0) {
        nvx_emit(c, "V0.5", "version-check", 0, NVX_V_TRANSPORT, "esc rejected");
    } else {
        nvx_emit(c, "V0.5", "version-check", 0,
                 v.reply == NV_RM_API_VERSION_REPLY_RECOGNIZED ? NVX_V_OK : NVX_V_REJECTED,
                 "reply %u, driver reports \"%.64s\"", v.reply, v.versionString);
    }
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 1 — RM object chain                                           */
/* ------------------------------------------------------------------ */

/* 64 KB alloc + MAP_MEMORY + pattern write/readback. vidmem=1 exercises the
 * VRAM path, vidmem=0 the sysmem/BAR1 path. */
static int mem_smoke(nvx_ctx *c, int vidmem)
{
    NvU32 st = 0;
    NV_MEMORY_ALLOCATION_PARAMS mp;
    memset(&mp, 0, sizeof mp);
    mp.owner = c->hClient;
    mp.type = NVOS32_TYPE_IMAGE;
    mp.size = 0x10000;
    mp.alignment = 0x10000;
    mp.attr = DRF_DEF(OS32, _ATTR, _PAGE_SIZE, _4KB) |
              (vidmem ? (DRF_DEF(OS32, _ATTR, _LOCATION, _VIDMEM) |
                         DRF_DEF(OS32, _ATTR, _COHERENCY, _UNCACHED))
                      : (DRF_DEF(OS32, _ATTR, _LOCATION, _PCI) |
                         DRF_DEF(OS32, _ATTR, _COHERENCY, _CACHED)));
    NvHandle hMem = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice,
                                 vidmem ? 0xcafe0021 : 0xcafe0020,
                                 vidmem ? NVX_CLASS_MEMORY_LOCAL_USER
                                        : NVX_CLASS_MEMORY_SYSTEM,
                                 &mp, sizeof mp, &st);
    if (!hMem) {
        nvx_emit(c, vidmem ? "V1.9b" : "V1.9a",
                 vidmem ? "mem-smoke-vidmem" : "mem-smoke-sysmem",
                 st, NVX_V_REJECTED, "alloc rejected");
        return -1;
    }
    NvU64 len = 0;
    volatile uint8_t *p = nvx_map_memory(c->ctl_fd, c->dev_fd, c->hClient,
                                         c->hSubdevice, hMem, 0x10000, &len);
    if (!p) {
        nvx_emit(c, vidmem ? "V1.9b" : "V1.9a",
                 vidmem ? "mem-smoke-vidmem" : "mem-smoke-sysmem",
                 0, NVX_V_REJECTED, "alloc 0x%08x ok, map failed", hMem);
        nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, hMem);
        return -1;
    }
    const NvU32 pat = vidmem ? 0xA5A5C3C3u : 0x5A5A3C3Cu;
    *(volatile NvU32 *)(p + 0x10) = pat;
    *(volatile NvU32 *)(p + len - 4) = ~pat;
    NvU32 r0 = *(volatile NvU32 *)(p + 0x10);
    NvU32 r1 = *(volatile NvU32 *)(p + len - 4);
    int ok = (r0 == pat && r1 == ~pat);
    nvx_emit(c, vidmem ? "V1.9b" : "V1.9a",
             vidmem ? "mem-smoke-vidmem" : "mem-smoke-sysmem",
             0, ok ? NVX_V_OK : NVX_V_REJECTED,
             "%lluKB mapped at %p, readback %s",
             (unsigned long long)(len >> 10), (const void *)p,
             ok ? "OK" : "MISMATCH");
    nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, hMem);
    return ok ? 0 : -1;
}

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

    /* Privilege introspection: which access rights did RM grant our plain
     * client? On the kext os_check_access() is hard-wired NV_TRUE, so this
     * shows what the RM face actually hands out. */
    NV0000_CTRL_CLIENT_GET_ACCESS_RIGHTS_PARAMS ar;
    memset(&ar, 0, sizeof ar);
    ar.hObject = cl;
    ar.hClient = cl;
    st = nvx_rm_control(c->ctl_fd, cl, cl, NV0000_CTRL_CMD_CLIENT_GET_ACCESS_RIGHTS,
                        &ar, sizeof ar);
    nvx_emit(c, "V1.7", "client-access-rights", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "mask raw 0x%08x 0x%08x 0x%08x 0x%08x" : "-",
             st == 0 ? ((NvU32 *)&ar.maskResult)[0] : 0,
             st == 0 ? ((NvU32 *)&ar.maskResult)[1] : 0,
             st == 0 ? ((NvU32 *)&ar.maskResult)[2] : 0,
             st == 0 ? ((NvU32 *)&ar.maskResult)[3] : 0);

    NV0000_CTRL_GPU_GET_ID_INFO_V2_PARAMS idv;
    memset(&idv, 0, sizeof idv);
    idv.gpuId = gpuId;
    st = nvx_rm_control(c->ctl_fd, cl, cl, NV0000_CTRL_CMD_GPU_GET_ID_INFO_V2,
                        &idv, sizeof idv);
    nvx_emit(c, "V1.8", "gpu-id-info-v2", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "flags 0x%x devInst %u boardId 0x%x numa %d" : "-",
             st == 0 ? idv.gpuFlags : 0, st == 0 ? idv.deviceInstance : 0,
             st == 0 ? idv.boardId : 0, st == 0 ? idv.numaId : 0);

    /* Memory smoke: BAR1 mapping of a sysmem + a vidmem allocation with a
     * write/readback pattern — the NV01_MEMORY_* -> MAP_MEMORY -> mmap path
     * the channel smoke and any future pushbuffer work depends on. */
    if (NVX_IDLE_ONLY)
        nvx_emit(c, "V1.9", "memory-smoke", 0, NVX_V_SKIP,
                 "idle-only build: memory smoke disabled");
    else if (mem_smoke(c, 0) == 0 && mem_smoke(c, 1) == 0)
        nvx_emit(c, "V1.9", "memory-smoke", 0, NVX_V_OK,
                 "sysmem+vidmem 64KB mapped, pattern verified");
    else
        nvx_emit(c, "V1.9", "memory-smoke", 0, NVX_V_REJECTED,
                 "see mem-smoke items above");
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
                 p.fbInfoList[4].data >> 10, p.fbInfoList[6].data,
                 p.fbInfoList[7].data, p.fbInfoList[5].data);
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
    p.bufSize = (NvU32)sizeof p.samples;
    p.count = NV2080_CTRL_PERF_GPUMON_SAMPLE_COUNT_PERFMON_UTIL;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_GPUMON_PERFMON_UTIL_SAMPLES_V2, &p);
    nvx_emit(c, "V2.6", "gpumon-util-samples-v2", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "tracker %u count %u buffer %u bytes (ring interpretation pending; use RUSD for utilization)" : "-",
             st == 0 ? p.tracker : 0, st == 0 ? p.count : 0, st == 0 ? p.bufSize : 0);
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

/* Tier-B GET probes: IDs are campaign-sourced (see nvx_defs.h), layouts
 * partially decoded. Read-only; verdicts measure firmware acceptance. Split
 * out of the default battery so they only run on an explicit request. */
static void item_tierb_reads(nvx_ctx *c)
{
    static uint8_t buf[4096], prev[4096];
    static const struct {
        const char *id, *name;
        NvU32 cmd;
        unsigned size;
    } rb[] = {
        { "V2.9",  "rails-get-info(legacy)",   NVX_RM_CMD_RAILS_GET_INFO,   NVX_RAILS_INFO_PARAM_SIZE   },
        { "V2.10", "rails-get-status(legacy)", NVX_RM_CMD_RAILS_GET_STATUS, NVX_RAILS_STATUS_PARAM_SIZE },
        { "V2.11", "vfpoints-get(legacy)",     NVX_RM_CMD_VFPOINTS_GET,     NVX_VFPOINTS_PARAM_SIZE     },
    };

    for (unsigned k = 0; k < sizeof rb / sizeof rb[0]; ++k) {
        memset(buf, 0, sizeof buf);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  rb[k].cmd, buf, rb[k].size);
        nvx_emit(c, rb[k].id, rb[k].name, st, st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 st == 0 ? "accepted at paramSize %u; w0..w2 = 0x%08x 0x%08x 0x%08x"
                         : "paramSize %u rejected",
                 rb[k].size, ((NvU32 *)buf)[0], ((NvU32 *)buf)[1], ((NvU32 *)buf)[2]);
        if (st != 0)
            continue;

        /* The call is accepted, but acceptance is not data. Re-read once and
         * report how much of the payload is populated and whether it is
         * stable; an undecoded layout must not be published as a reading. */
        memcpy(prev, buf, rb[k].size);
        memset(buf, 0, sizeof buf);
        NvU32 st2 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                   rb[k].cmd, buf, rb[k].size);
        unsigned nz = 0, moved = 0;
        for (unsigned off = 0; off + 4 <= rb[k].size; off += 4) {
            unsigned v;
            memcpy(&v, buf + off, 4);
            if (v) ++nz;
            if (memcmp(buf + off, prev + off, 4)) ++moved;
        }
        nvx_emit(c, "V2.9p", rb[k].name, st2, NVX_V_SKIP,
                 "payload %u non-zero words, %u changed on re-read: layout not decoded, treat as probe data only",
                 nz, moved);
    }
}

static void item_legacy_probes(nvx_ctx *c)
{
    item_tierb_reads(c);
}

int nvx_stage2_tierb_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.tierb", "tierb-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_tierb_reads(c);
    return 0;
}

/* RATED_TDP read-only probe (--rated-tdp-read).
 *
 * NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206e) is the only public,
 * header-defined read face of the RATED_TDP arbitration feature
 * (ctrl2080perf.h:387). It is a GET: it reports, per arbitration client, the
 * requested action RM would arbitrate. Its SET twin (0x2080206f) is never sent
 * here; the point of this item is to establish whether an end-user tool can
 * even *see* the feature through the macOS RM surface before any write path is
 * contemplated. All client indices are queried so a partly-implemented kext
 * cannot hide behind a single accepted call. */
static const char *rated_tdp_client_name(NvU32 cl)
{
    switch (cl) {
    case NV2080_CTRL_PERF_RATED_TDP_CLIENT_RM:              return "RM";
    case NV2080_CTRL_PERF_RATED_TDP_CLIENT_WAR_BUG_1785342: return "WAR_1785342";
    case NV2080_CTRL_PERF_RATED_TDP_CLIENT_GLOBAL:          return "GLOBAL";
    case NV2080_CTRL_PERF_RATED_TDP_CLIENT_OS:              return "OS";
    case NV2080_CTRL_PERF_RATED_TDP_CLIENT_PROFILE:         return "PROFILE";
    default:                                                return "?";
    }
}

static const char *rated_tdp_action_name(NvU32 a)
{
    switch (a) {
    case NV2080_CTRL_PERF_RATED_TDP_ACTION_DEFAULT:      return "DEFAULT";
    case NV2080_CTRL_PERF_RATED_TDP_ACTION_FORCE_EXCEED: return "FORCE_EXCEED";
    case NV2080_CTRL_PERF_RATED_TDP_ACTION_FORCE_LIMIT:  return "FORCE_LIMIT";
    case NV2080_CTRL_PERF_RATED_TDP_ACTION_FORCE_LOCK:   return "FORCE_LOCK";
    case NV2080_CTRL_PERF_RATED_TDP_ACTION_FORCE_FLOOR:  return "FORCE_FLOOR";
    default:                                             return "?";
    }
}

/* The STATUS command id is not exported as a CMD_ macro by the SDK header -- it
 * only ships the message id (NV2080_CTRL_PERF_RATED_TDP_STATUS_PARAMS_MESSAGE_ID
 * = 0x6D). Derive the full id from the perf interface base (0x20802000, cf.
 * GET_CURRENT_PSTATE 0x20802068 <-> msg 0x68). Read-only; a kext that does not
 * implement it will answer NV_ERR_NOT_SUPPORTED, which is itself a result. */
#define NVX_RM_CMD_PERF_RATED_TDP_GET_STATUS 0x2080206Du

static void item_rated_tdp_reads(nvx_ctx *c)
{
    for (NvU32 cl = 0; cl < NV2080_CTRL_PERF_RATED_TDP_CLIENT_NUM_CLIENTS; ++cl) {
        NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS p;
        memset(&p, 0, sizeof p);
        p.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)cl;
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &p);
        nvx_emit(c, "V2.rtdp", "rated-tdp-get-control", st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "client %s(%u): action %s(%u) vPstate %u",
                 rated_tdp_client_name(cl), cl,
                 rated_tdp_action_name((NvU32)p.input), (NvU32)p.input,
                 (NvU32)p.vPstateType);
    }

    /* Negative control: indices outside [0,NUM_CLIENTS) must be rejected by the
     * RM, which proves the accepted calls above are validated rather than a
     * blanket NV_OK. Still read-only. */
    static const NvU32 bad_clients[] = {
        NV2080_CTRL_PERF_RATED_TDP_CLIENT_NUM_CLIENTS, 0xffu,
    };
    for (unsigned bi = 0; bi < sizeof bad_clients / sizeof bad_clients[0]; ++bi) {
        NvU32 bad = bad_clients[bi];
        NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS p;
        memset(&p, 0, sizeof p);
        p.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)bad;
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &p);
        nvx_emit(c, "V2.rtdp!", "rated-tdp-get-control", st,
                 st != 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "invalid client index %u %s", bad,
                 st != 0 ? "rejected as expected (lookup is validated)"
                         : "ACCEPTED -> RM does not validate the client index");
    }

    /* Cross-check: the arbitrated output plus every client's input in one call. */
    NV2080_CTRL_PERF_RATED_TDP_STATUS_PARAMS s;
    memset(&s, 0, sizeof s);
    NvU32 st = SUBCTL(c, NVX_RM_CMD_PERF_RATED_TDP_GET_STATUS, &s);
    if (st == 0) {
        char inputs[96];
        int off = 0;
        for (NvU32 i = 0; i < NV2080_CTRL_PERF_RATED_TDP_CLIENT_NUM_CLIENTS; ++i)
            off += snprintf(inputs + off, sizeof inputs - (size_t)off, "%s%u",
                            i ? "," : "", (NvU32)s.inputs[i]);
        nvx_emit(c, "V2.rtds", "rated-tdp-status", st, NVX_V_OK,
                 "output %s(%u) vPstate %u clientActiveMask 0x%x regkeyLimit %u inputs[%s]",
                 rated_tdp_action_name((NvU32)s.output), (NvU32)s.output,
                 (NvU32)s.outputVPstate, s.rm.clientActiveMask,
                 (NvU32)s.rm.bRegkeyLimitRatedTdp, inputs);
    } else {
        nvx_emit(c, "V2.rtds", "rated-tdp-status", st, NVX_V_REJECTED,
                 "derived cmd id 0x2080206d (message id only in SDK header) rejected");
    }
}

int nvx_stage2_rated_tdp_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.rtdp", "rated-tdp-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_rated_tdp_reads(c);
    return 0;
}

/* Thermal System V2 read probe (--thermal-read).
 *
 * NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513) is a public, in-tree
 * opcode-batched interface (ctrl2080thermal.h) whose read opcodes are
 * documented to return NV_OK: TARGETS_AVAILABLE, SENSORS_AVAILABLE,
 * SENSOR_READING, SENSOR_READING_RANGE, SENSOR_TARGET, SENSOR_PROVIDER. If the
 * RM implements it on this kext this is a *header-defined* temperature/sensor
 * read face with per-sensor range (stage 3 min/max) — unlike the Tier-B GETs
 * whose IDs are campaign-sourced. All opcodes here are GET; nothing is written.
 * The batch runs with EXECUTE_FLAGS_IGNORE_FAIL so one unsupported opcode does
 * not hide the rest; each instruction's own NV_STATUS is reported. */
static void item_thermal_reads(nvx_ctx *c)
{
    static NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS p;

    /* Provide the API handshake fields the header requires. */
    memset(&p, 0, sizeof p);
    p.clientAPIVersion = THERMAL_SYSTEM_API_VER;
    p.clientAPIRevision = THERMAL_SYSTEM_API_REV;
    p.clientInstructionSizeOf = (NvU32)sizeof(NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION);
    p.executeFlags = NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_FLAGS_IGNORE_FAIL;

    /* Pass 1: ask how many targets and sensors exist. */
    p.instructionListSize = 2;
    p.instructionList[0].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_TARGETS_AVAILABLE_OPCODE;
    p.instructionList[1].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSORS_AVAILABLE_OPCODE;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2, &p);
    nvx_emit(c, "V2.thm", "thermal-targets-sensors", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "EXECUTE_V2 (0x20800513) paramSize %u: %s",
             (NvU32)sizeof p,
             st == 0 ? "batch accepted" : nvx_status_name(st));
    if (st != 0) {
        /* Try the non-privileged twin before giving up. */
        memset(&p, 0, sizeof p);
        p.clientAPIVersion = THERMAL_SYSTEM_API_VER;
        p.clientAPIRevision = THERMAL_SYSTEM_API_REV;
        p.clientInstructionSizeOf = (NvU32)sizeof(NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION);
        p.executeFlags = NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_FLAGS_IGNORE_FAIL;
        p.instructionListSize = 2;
        p.instructionList[0].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_TARGETS_AVAILABLE_OPCODE;
        p.instructionList[1].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSORS_AVAILABLE_OPCODE;
        NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2_PHYSICAL, &p);
        nvx_emit(c, "V2.thm", "thermal-targets-sensors(physical)", st2,
                 st2 == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "EXECUTE_V2_PHYSICAL (0x20808513): %s (%s)",
                 st2 == 0 ? "batch accepted" : "refused", nvx_status_name(st2));
        if (st2 != 0) return;
    }

    NvU32 targets = p.instructionList[0].operands.getInfoTargetsAvailable.availableTargets;
    NvU32 sensors = p.instructionList[1].operands.getInfoSensorsAvailable.availableSensors;
    nvx_emit(c, "V2.thm", "thermal-counts", 0, NVX_V_OK,
             "%u target(s), %u sensor(s)", targets, sensors);
    if (sensors == 0) return;

    /* Pass 2: per-sensor reading + range (cap at 8 so we stay under the
     * instruction-list limit and report compactly). */
    const NvU32 n = sensors > 8 ? 8 : sensors;
    for (NvU32 i = 0; i < n; ++i) {
        memset(&p, 0, sizeof p);
        p.clientAPIVersion = THERMAL_SYSTEM_API_VER;
        p.clientAPIRevision = THERMAL_SYSTEM_API_REV;
        p.clientInstructionSizeOf = (NvU32)sizeof(NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION);
        p.executeFlags = NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_FLAGS_IGNORE_FAIL;
        p.instructionListSize = 4;
        p.instructionList[0].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_STATUS_SENSOR_READING_OPCODE;
        p.instructionList[0].operands.getStatusSensorReading.sensorIndex = i;
        p.instructionList[1].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSOR_READING_RANGE_OPCODE;
        p.instructionList[1].operands.getInfoSensorReadingRange.sensorIndex = i;
        p.instructionList[2].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSOR_TARGET_OPCODE;
        p.instructionList[2].operands.getInfoSensorTarget.sensorIndex = i;
        p.instructionList[3].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSOR_PROVIDER_OPCODE;
        p.instructionList[3].operands.getInfoSensorProvider.sensorIndex = i;
        NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2, &p);
        if (st2 != 0) {
            nvx_emit(c, "V2.thm", "thermal-sensor", st2, NVX_V_REJECTED,
                     "sensor %u batch rejected", i);
            continue;
        }
        NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION *rd = &p.instructionList[0];
        NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION *rg = &p.instructionList[1];
        NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION *tg = &p.instructionList[2];
        NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION *pv = &p.instructionList[3];
        nvx_emit(c, "V2.thm", "thermal-sensor", rd->result,
                 (rd->executed && rd->result == 0) ? NVX_V_OK : NVX_V_REJECTED,
                 "sensor %u reading %d | range [%d,%d] | target %u provider %u "
                 "(exec %u/%u/%u/%u, rc 0x%x/0x%x/0x%x/0x%x)",
                 i,
                 rd->operands.getStatusSensorReading.value,
                 rg->operands.getInfoSensorReadingRange.minimum,
                 rg->operands.getInfoSensorReadingRange.maximum,
                 tg->operands.getInfoSensorTarget.targetIndex,
                 pv->operands.getInfoSensorProvider.providerIndex,
                 rd->executed, rg->executed, tg->executed, pv->executed,
                 rd->result, rg->result, tg->result, pv->result);
    }

    /* Pass 3: classify target 0 and provider 0 (types come from the header
     * enum: TARGET_GPU/MEMORY/POWER_SUPPLY/BOARD...). */
    memset(&p, 0, sizeof p);
    p.clientAPIVersion = THERMAL_SYSTEM_API_VER;
    p.clientAPIRevision = THERMAL_SYSTEM_API_REV;
    p.clientInstructionSizeOf = (NvU32)sizeof(NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION);
    p.executeFlags = NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_FLAGS_IGNORE_FAIL;
    p.instructionListSize = 2;
    p.instructionList[0].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_TARGET_TYPE_OPCODE;
    p.instructionList[1].opcode = NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_PROVIDER_TYPE_OPCODE;
    NvU32 st3 = SUBCTL(c, NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2, &p);
    if (st3 == 0) {
        nvx_emit(c, "V2.thm", "thermal-classify", 0, NVX_V_OK,
                 "target 0 type %u (1=GPU 2=MEMORY 4=POWER_SUPPLY 8=BOARD) "
                 "provider 0 type %u",
                 p.instructionList[0].operands.getInfoTargetType.type,
                 p.instructionList[1].operands.getInfoProviderType.type);
    } else {
        nvx_emit(c, "V2.thm", "thermal-classify", st3, NVX_V_REJECTED,
                 "target/provider type batch rejected");
    }
}

int nvx_stage2_thermal_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.thm", "thermal-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_thermal_reads(c);
    return 0;
}

/* Internal PM/PFM read probe (--pm-read).
 *
 * ctrl2080internal.h also defines a GSP-internal PFM request-handler family
 * (INTERNAL_INTERFACE_ID). Only its GET faces run here — the SET/UPDATE twins
 * (SET_PM1_STATE, SET_VPSTATE, UPDATE_TGPU_LIMIT, CONFIGURE_TGP_MODE,
 * CONFIGURE_TURBO_V2, UNSET_DYNAMIC_BOOST_LIMIT) are performance writes and
 * are not sent. The point is to learn whether this family is reachable at all:
 * a single NOT_SUPPORTED on one command (the earlier EDPp GET) is not proof
 * that every command in the block is absent. */
static void item_pm_reads(nvx_ctx *c)
{
    /* PM1 state: flag 0 = "PM1 forced off", 1 = "PM1 available". */
    for (NvU8 flag = 0; flag <= 1; ++flag) {
        NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_PM1_STATE_PARAMS p;
        memset(&p, 0, sizeof p);
        p.flag = flag;
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_PM1_STATE, &p);
        nvx_emit(c, "V2.pm", "internal-pm1-state", st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "flag %u (%s) -> bStatus %u",
                 flag, flag ? "PM1 available" : "PM1 forced off", p.bStatus);
    }

    /* V/P-state capability + mapping. */
    NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_VPSTATE_INFO_PARAMS vi;
    memset(&vi, 0, sizeof vi);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_VPSTATE_INFO, &vi);
    nvx_emit(c, "V2.pm", "internal-vpstate-info", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "PS2.0 supported %u, highest vPstate idx %u",
             vi.bVpsPs20Supported, vi.vPstateIdxHighest);

    if (st == 0) {
        const NvU32 top = vi.vPstateIdxHighest > 15 ? 15 : vi.vPstateIdxHighest;
        for (NvU32 ps = 0; ps <= top; ++ps) {
            NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_VPSTATE_MAPPING_PARAMS m;
            memset(&m, 0, sizeof m);
            m.pStateIdx = ps;
            NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PERF_PFM_REQ_HNDLR_GET_VPSTATE_MAPPING, &m);
            nvx_emit(c, "V2.pm", "internal-vpstate-mapping", st2,
                     st2 == 0 ? NVX_V_OK : NVX_V_REJECTED,
                     "pState %u -> vPstate %u", ps, m.vPstateIdxMapping);
        }
    }

    /* Boost-sync info and auxiliary power state. */
    NV2080_CTRL_INTERNAL_PERF_GPU_BOOST_SYNC_GET_INFO_PARAMS bi;
    memset(&bi, 0, sizeof bi);
    NvU32 st3 = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PERF_GPU_BOOST_SYNC_GET_INFO, &bi);
    nvx_emit(c, "V2.pm", "internal-boost-sync-info", st3,
             st3 == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "hysteresis %llu us, hysteresis enable %u, SLI sync %u",
             (unsigned long long)bi.hysteresisus,
             bi.bHystersisEnable, bi.bSliGpuBoostSyncEnable);

    NV2080_CTRL_INTERNAL_PERF_GET_AUX_POWER_STATE_PARAMS ap;
    memset(&ap, 0, sizeof ap);
    NvU32 st4 = SUBCTL(c, NV2080_CTRL_CMD_INTERNAL_PERF_GET_AUX_POWER_STATE, &ap);
    nvx_emit(c, "V2.pm", "internal-aux-power-state", st4,
             st4 == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "aux powerState %u", ap.powerState);
}

int nvx_stage2_pm_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.pm", "pm-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_pm_reads(c);
    return 0;
}

/* Public power/clock read-surface census (--perf-read).
 *
 * The default battery already exercises the PERF/CLK/THERMAL reads the
 * telemetry path needs. This item closes the census for the *control* read
 * question: "is there any public SDK read channel for offsets, voltages, fan
 * limits or clock caps?". Every GET/INFO/STATUS/READ command in the public
 * perf/pmgr/clk/thermal families is either probed here or recorded as an
 * explicit SKIP with the reason it cannot be sent — so the answer is a
 * complete list, not "we tried a few". */
static void item_perf_reads(nvx_ctx *c)
{
    /* Subdevice module info: pure-output struct (static module id + whether
     * NVSwitch is supported). */
    NV2080_CTRL_PMGR_MODULE_INFO_PARAMS mi;
    memset(&mi, 0, sizeof mi);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PMGR_GET_MODULE_INFO, &mi);
    nvx_emit(c, "V2.perf", "pmgr-get-module-info", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "moduleId 0x%08x nvswitchSupport %u" : "-",
             st == 0 ? mi.moduleId : 0, st == 0 ? mi.nvswitchSupport : 0);

    /* Video-engine perfmon sample: engineType in, busy% + sampling period out.
     * Reaches the NVENC/NVDEC/NVJPG/NVOFA engines only. */
    for (NvU32 eng = (NvU32)NV2080_CTRL_CMD_PERF_VID_ENG_NVENC;
         eng <= (NvU32)NV2080_CTRL_CMD_PERF_VID_ENG_NVOFA; ++eng) {
        NV2080_CTRL_PERF_GET_VID_ENG_PERFMON_SAMPLE_PARAMS vs;
        memset(&vs, 0, sizeof vs);
        vs.engineType = (NV2080_CTRL_CMD_PERF_VID_ENG)eng;
        NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_VID_ENG_PERFMON_SAMPLE, &vs);
        nvx_emit(c, "V2.perf", "perf-vid-eng-perfmon-sample", st2,
                 st2 == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "engine %u -> busy %u%% period %u us",
                 eng, st2 == 0 ? vs.clkPercentBusy : 0,
                 st2 == 0 ? vs.samplingPeriodUs : 0);
    }

    /* Deliberately not sent; the reason is part of the census. */
    nvx_emit(c, "V2.perf", "perf-level-info-v1", 0, NVX_V_SKIP,
             "not sent: NV2080_CTRL_PERF_GET_LEVEL_INFO carries an NvP64 kernel "
             "list, so it cannot be driven through this transport; the inline V2 "
             "twin is probed as V2.15");
    nvx_emit(c, "V2.perf", "perf-gpu-is-idle", 0, NVX_V_SKIP,
             "not sent: despite the name the SDK documents it as a P-state "
             "aggression notification (state-changing), so it is a write");
    nvx_emit(c, "V2.perf", "perf-tegra-perfmon-sample", 0, NVX_V_SKIP,
             "not sent: Tegra-only command, not applicable to this discrete GPU");
}

int nvx_stage2_perf_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.perf", "perf-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_perf_reads(c);
    return 0;
}

/* ------------------------------------------------------------------ */
/* Private clock-domain / V-F read-surface census (--clkvf-read)       */
/* ------------------------------------------------------------------ */
/* The Windows NVOC backend reads the editable core/memory clock offsets and
 * the whole V/F curve through the private ClockClient family: MEASURE_FREQ,
 * ClkDomains GET_CONTROL, the V/F-points GET_INFO/GET_STATUS/GET_CONTROL trio
 * and the PerfVfe equation/variable GETs (nvapi-rs/src/clock.rs,
 * nvapi-rs/src/gpu.rs). Those are the read half of the same wires the known
 * SET ids ride, and they are missing from the OGKM 610.57.04 SDK headers, so
 * the ids live in nvx_defs.h with their RE provenance.
 *
 * On macOS the same RM subdevice controls are reachable, but the kext
 * validates paramsSize: a known id answers NV_ERR_INVALID_ARGUMENT (0x1f) for
 * every size except the one it wants, while an id it does not implement
 * answers something else (NOT_SUPPORTED / INVALID_METHOD / ...). That
 * difference is the oracle this census uses -- and it is exactly the trap
 * that made the first Tier-B probe record three working commands as "the
 * driver refuses them". Two negative controls (ids in unused slots of the
 * same interfaces) are scanned with the identical ladder so the 0x1f verdict
 * is anchored to a measured baseline instead of an assumption.
 *
 * Read-only by construction: every real candidate is a GET, and the candidate
 * table is checked against the documented SET ids before anything is sent.
 */
static const NvU32 nvx_known_set_ids[] = {
    NVX_RM_CMD_VOLT_LIMITS_SET,    /* 0x20801ACF, MODS Volt3x.SetVoltagesMv   */
    NVX_RM_CMD_GPC_VF_OFFSET_SET,  /* 0x2080D031, nvmlDeviceSetGpcClkVfOffset */
    0x2080D01Cu, /* ClockClient ClkDomains SET_CONTROL (nvapi-rs gpu.rs:2203) */
    0x2080D024u, /* ClockClient VfPoints SET_CONTROL (nvapi-rs gpu.rs:3135)  */
    0x2080D0A3u, /* single-slot V/F point write (p100 audit / blackwell T3-1) */
    0x2080E0B0u, /* PerfVfeVar SET_CONTROL (oc-gap-layouts-r610-74.md B3)     */
    0x2080E0B7u, /* PerfVfeEqu SET_CONTROL (oc-gap-layouts-r610-74.md B3)     */
    0x2080F031u, /* nvmlDeviceSetGpcClkVfOffset SET, 610-era twin of D031     */
    0x2080F214u, /* VoltVoltRails SET_CONTROL (voltrails r610)                */
};

static int nvx_id_is_known_set(NvU32 cmd)
{
    for (unsigned i = 0; i < sizeof nvx_known_set_ids / sizeof nvx_known_set_ids[0]; ++i)
        if (cmd == nvx_known_set_ids[i]) return 1;
    return 0;
}

/* paramSize ladder for one control id. Small alignments first, then the sizes
 * the private families are documented to use: the Blackwell V/F-points
 * magics 0x1182C/0x12420 (nvapi-rs nvid.rs:757), the ClockClient control
 * magics 0x10964/0x261A4 (nvapi-rs clock.rs:360) with the 0x13000 working
 * buffer, and the Windows NVAPI escape buffer sizes 0x98240/0x100440. A
 * stamp-validating command needs its size *and* its header; a size-only scan
 * cannot see the latter, which is why a "0x1f everywhere" verdict is reported
 * as "known command, size/header not found" and never as "not implemented".
 */
static const unsigned nvx_clk_sizes[] = {
    4, 8, 12, 16, 24, 32, 36, 40, 44, 48, 56, 64, 76, 88, 96, 100, 128,
    160, 192, 200, 256, 260, 264, 268, 272, 280, 288, 320, 512, 776, 1036,
    1060, 1064, 1092, 1120, 2048, 4096, 4368, 6188, 8448, 9248, 12288, 16384,
    32768, 65536, 67936, 67940, 67944, 77824, 82976, 156064, 156068, 156072,
    262144, 401600, 623168, 737084, 1050688, 1349156, 4343300,
};

static void clk_scan_one(nvx_ctx *c, unsigned char *buf, unsigned char *prev,
                         const char *name, NvU32 cmd, int negative)
{
    const unsigned nsizes = sizeof nvx_clk_sizes / sizeof nvx_clk_sizes[0];
    unsigned accepted[4], nacc = 0;
    unsigned n_1f = 0, n_56 = 0, n_other = 0;
    NvU32 other_st = 0;
    unsigned other_sz = 0;

    for (unsigned s = 0; s < nsizes; ++s) {
        memset(buf, 0, nvx_clk_sizes[s]);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  cmd, buf, nvx_clk_sizes[s]);
        if (st == 0) {
            if (nacc < 4) accepted[nacc] = nvx_clk_sizes[s];
            ++nacc;
        } else if (st == NV_ERR_INVALID_ARGUMENT) {
            ++n_1f;
        } else if (st == NV_ERR_NOT_SUPPORTED) {
            ++n_56;
        } else {
            ++n_other;
            if (other_st == 0) { other_st = st; other_sz = nvx_clk_sizes[s]; }
        }
    }

    char acc_text[96];
    if (nacc) {
        int off = 0;
        for (unsigned a = 0; a < nacc && a < 4; ++a)
            off += snprintf(acc_text + off, sizeof acc_text - (size_t)off,
                            "%s%u", a ? "," : "", accepted[a]);
        if (nacc > 4)
            snprintf(acc_text + off, sizeof acc_text - (size_t)off,
                     ",+%u", nacc - 4);
    } else {
        snprintf(acc_text, sizeof acc_text, "none");
    }

    char other_text[80] = "";
    if (n_other)
        snprintf(other_text, sizeof other_text,
                 ", first other 0x%08x (%s) at size %u",
                 other_st, nvx_status_name(other_st), other_sz);

    nvx_emit(c, "V2.clkvf", name, nacc ? 0 : NV_ERR_INVALID_ARGUMENT,
             nacc ? NVX_V_OK : NVX_V_REJECTED,
             "cmd 0x%08x%s over %u sizes: accepted [%s]; invalid-arg %u, "
             "not-supported %u, other %u%s",
             cmd, negative ? " (negative control)" : "", nsizes,
             acc_text, n_1f, n_56, n_other, other_text);

    /* Acceptance is not data. Re-read the accepted sizes and report how much
     * of the payload is populated, whether it moves, and the first dwords in
     * hex, so a layout we have not decoded is never published as a reading
     * (same rule the Tier-B GETs follow). */
    for (unsigned a = 0; a < nacc && a < 2; ++a) {
        unsigned sz = accepted[a];
        memset(buf, 0, sz);
        NvU32 st1 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                   cmd, buf, sz);
        if (st1 != 0) continue;
        memcpy(prev, buf, sz);
        memset(buf, 0, sz);
        NvU32 st2 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                   cmd, buf, sz);
        unsigned nz = 0, moved = 0;
        for (unsigned off = 0; off + 4 <= sz; off += 4) {
            NvU32 v;
            memcpy(&v, buf + off, 4);
            if (v) ++nz;
            if (memcmp(buf + off, prev + off, 4)) ++moved;
        }
        nvx_emit(c, "V2.clkvf", name, st2, NVX_V_SKIP,
                 "paramSize %u accepted (st1 0x%08x st2 0x%08x): %u non-zero "
                 "words, %u changed on re-read: layout not decoded, probe "
                 "data only", sz, st1, st2, nz, moved);

        /* Raw payload in 16-dword lines: enough to decode a fixed-stride
         * record table by hand without overflowing the result detail field. */
        unsigned show = sz / 4;
        for (unsigned base = 0; base < show; base += 16) {
            unsigned n = show - base < 16 ? show - base : 16;
            char hex[16 * 9 + 1];
            char *hp = hex;
            for (unsigned w = 0; w < n; ++w) {
                NvU32 v;
                memcpy(&v, buf + 4 * (base + w), 4);
                hp += snprintf(hp, 10, "%s%08x", w ? " " : "", v);
            }
            nvx_emit(c, "V2.clkvf", name, 0, NVX_V_SKIP,
                     "payload dwords [%u..%u): %s", base, base + n, hex);
        }
    }
}

static void item_clk_reads(nvx_ctx *c)
{
    static const struct { const char *name; NvU32 cmd; } reads[] = {
        { "clk-measure-freq",          NVX_RM_CMD_CLK_MEASURE_FREQ        },
        { "clk-domains-get-control",   NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL },
        { "vfp-get-info",              NVX_RM_CMD_VFP_GET_INFO            },
        { "vfp-get-status",            NVX_RM_CMD_VFP_GET_STATUS          },
        { "vfp-get-control",           NVX_RM_CMD_VFP_GET_CONTROL         },
        { "vfp-get-info(582.41)",      NVX_RM_CMD_VFP_GET_INFO_582        },
        { "vfp-get-status(582.41)",    NVX_RM_CMD_VFP_GET_STATUS_582      },
        { "vfp-get-aux(582.41)",       NVX_RM_CMD_VFP_GET_AUX_582         },
        { "vfe-equ-get-info",          NVX_RM_CMD_VFE_EQU_GET_INFO        },
        { "vfe-equ-get-control",       NVX_RM_CMD_VFE_EQU_GET_CONTROL     },
        { "vfe-var-get-info",          NVX_RM_CMD_VFE_VAR_GET_INFO        },
        { "vfe-var-get-control",       NVX_RM_CMD_VFE_VAR_GET_CONTROL     },
        { "rails-get-control(legacy)", NVX_RM_CMD_RAILS_GET_CONTROL       },
    };
    /* Negative controls: ids in the same interfaces but past any documented
     * message slot. Scanned with the identical ladder so the "0x1f at every
     * size" verdict for a real candidate can be compared against what an
     * unimplemented id actually answers on this driver. */
    static const struct { const char *name; NvU32 cmd; } controls[] = {
        { "neg-control 0x2080ffff", 0x2080FFFFu },
        { "neg-control 0x2080a0ff", 0x2080A0FFu },
    };
    const unsigned buflen =
        nvx_clk_sizes[sizeof nvx_clk_sizes / sizeof nvx_clk_sizes[0] - 1];
    unsigned char *buf = malloc(buflen), *prev = malloc(buflen);

    if (!buf || !prev) {
        free(buf);
        free(prev);
        nvx_emit(c, "V2.clkvf", "clkvf-read", 0, NVX_V_HARD,
                 "scan buffer alloc failed");
        return;
    }

    for (unsigned r = 0; r < sizeof reads / sizeof reads[0]; ++r) {
        if (nvx_id_is_known_set(reads[r].cmd)) {
            nvx_emit(c, "V2.clkvf", reads[r].name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id; the read census "
                     "never sends it", reads[r].cmd);
            continue;
        }
        clk_scan_one(c, buf, prev, reads[r].name, reads[r].cmd, 0);
    }
    for (unsigned r = 0; r < sizeof controls / sizeof controls[0]; ++r)
        clk_scan_one(c, buf, prev, controls[r].name, controls[r].cmd, 1);

    free(buf);
    free(prev);
}

int nvx_stage2_clk_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkvf", "clkvf-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    item_clk_reads(c);
    return 0;
}

static void item_bios_sku(nvx_ctx *c)
{
    NV2080_CTRL_BIOS_GET_SKU_INFO_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_BIOS_GET_SKU_INFO, &p);
    nvx_emit(c, "V2.13", "bios-sku-info", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "board 0x%x sku \"%.8s\" mod \"%.4s\" project \"%.4s\"" : "-",
             st == 0 ? p.BoardID : 0, st == 0 ? p.chipSKU : "?",
             st == 0 ? p.chipSKUMod : "?", st == 0 ? p.project : "?");
}

static void item_bus(nvx_ctx *c)
{
    NV2080_CTRL_BUS_GET_PCI_INFO_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_BUS_GET_PCI_INFO, &p);
    nvx_emit(c, "V2.14", "bus-get-pci-info", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             st == 0 ? "deviceId 0x%08x subsys 0x%08x rev 0x%x ext 0x%x" : "-",
             st == 0 ? p.pciDeviceId : 0, st == 0 ? p.pciSubSystemId : 0,
             st == 0 ? p.pciRevisionId : 0, st == 0 ? p.pciExtDeviceId : 0);
}

static void item_perf_levels(nvx_ctx *c)
{
    /* Capacities matter here too. The driver answers NV_OK with an empty list
     * when asked for zero domains, and NV_ERR_INVALID_ARGUMENT as soon as one
     * domain is requested: it accepts the control but supplies no clock-domain
     * table. The original battery pre-set perfGetClkInfoListSize to
     * MAX_DOMAINS, which made this look like an unconditional rejection. */
    static NV2080_CTRL_PERF_GET_LEVEL_INFO_V2_PARAMS p;

    memset(&p, 0, sizeof p);
    p.level = NV2080_CTRL_PERF_GET_LEVEL_INFO_LEVEL_STANDARD;
    NvU32 st_empty = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2, &p);
    NvU32 empty_count = p.perfGetClkInfoListSize;

    memset(&p, 0, sizeof p);
    p.level = NV2080_CTRL_PERF_GET_LEVEL_INFO_LEVEL_STANDARD;
    p.perfGetClkInfoListSize = 1;
    NvU32 st_one = SUBCTL(c, NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2, &p);

    nvx_emit(c, "V2.15", "perf-level-info-v2", st_one,
             st_one == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "capacity 0 -> 0x%x (%u domains returned); capacity 1 -> 0x%x "
             "(driver exposes no clock-domain list)",
             st_empty, empty_count, st_one);
}

static void item_i2c_probe(nvx_ctx *c)
{
    /* Dispatch-reachability probe with zeroed params: an I2C transaction on
     * port 0 with no entries should be rejected by argument validation if the
     * FINN I2C family is wired up — any well-formed NV_ERR_* is informative,
     * silence (transport failure) is not expected. READ face only. */
    NV2080_CTRL_I2C_READ_BUFFER_PARAMS p;
    memset(&p, 0, sizeof p);
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_I2C_READ_BUFFER, &p);
    nvx_emit(c, "V2.16", "i2c-read-buffer", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "zeroed-params probe (fan/sensor I2C reachability)");
}

static void item_rusd(nvx_ctx *c)
{
    if (c->dev_fd < 0) {
        nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_SKIP, "no device connection");
        return;
    }
    NvU32 st = 0;
    NV00DE_ALLOC_PARAMETERS ap;
    memset(&ap, 0, sizeof ap);
    ap.polledDataMask = NVX_RUSD_POLL_MASK;
    NvHandle hRusd = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hSubdevice,
                                  0xcafe0003, NVX_CLASS_USER_SHARED_DATA,
                                  &ap, sizeof ap, &st);
    if (!hRusd) {
        nvx_emit(c, "V2.12", "rusd-page", st, NVX_V_REJECTED,
                 "NV00DE alloc rejected");
        return;
    }

    NvU64 len = 0;
    /* RUSD is SYSMEM: RM binds its mapping context to the control fd. */
    void *page = nvx_map_memory_readonly(c->ctl_fd, c->ctl_fd, c->hClient, c->hSubdevice,
                                         hRusd, sizeof(NV00DE_SHARED_DATA), &len, &st);
    if (!page) {
        nvx_emit(c, "V2.12", "rusd-page", st, NVX_V_REJECTED,
                 "NV00DE alloc ok (0x%08x) but read-only mapping failed (RM status or transport/IOKit failure)",
                 hRusd);
        nvx_rm_free(c->ctl_fd, c->hClient, c->hSubdevice, hRusd);
        return;
    }
    if (len < sizeof(NV00DE_SHARED_DATA)) {
        nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_REJECTED,
                 "mapping too short: %llu bytes, need %zu",
                 (unsigned long long)len, sizeof(NV00DE_SHARED_DATA));
        return;
    }
    c->rusd = page;
    c->rusd_len = len;
    nvx_emit(c, "V2.12", "rusd-page", 0, NVX_V_OK,
             "NV00DE 0x%08x mapped at %p len %lluKB — poll mask 0x%x (field validity checked separately)",
             hRusd, page, (unsigned long long)(len >> 10), NVX_RUSD_POLL_MASK);

    /* Full RUSD readout: every polled section with a stable timestamp. */
    {
        NV00DE_SHARED_DATA *sd = (NV00DE_SHARED_DATA *)page;
        RUSD_CLK_PUBLIC_DOMAIN_INFOS clks;
        RUSD_PERF_CURRENT_PSTATE pst;
        RUSD_POWER_LIMITS pl;
        RUSD_INST_POWER_USAGE ipw;
        RUSD_AVG_POWER_USAGE apw;
        RUSD_POWER_POLICY_STATUS pol;
        RUSD_FAN_COOLER_STATUS fan;
        RUSD_TEMPERATURE temps[RUSD_TEMPERATURE_TYPE_MAX];
        char tbuf[96];
        size_t o = 0;
        tbuf[0] = 0;

        for (int i = 0; i < RUSD_TEMPERATURE_TYPE_MAX; i++) {
            if (rusd_copy(&sd->temperatures[i].lastModifiedTimestamp,
                          &sd->temperatures[i], &temps[i], sizeof temps[i]) != 0)
                continue;
            int n = snprintf(tbuf + o, sizeof tbuf - o, "%s%s=%ld.%ldC",
                             o ? " " : "",
                             i == RUSD_TEMPERATURE_TYPE_GPU ? "gpu" :
                             i == RUSD_TEMPERATURE_TYPE_MEMORY ? "mem" :
                             i == RUSD_TEMPERATURE_TYPE_BOARD ? "board" :
                             i == RUSD_TEMPERATURE_TYPE_POWER_SUPPLY ? "psu" : "hbm",
                             (long)temps[i].temperature >> 8,
                             ((long)temps[i].temperature & 0xff) * 100 / 256);
            if (n > 0 && o + n < sizeof tbuf) o += (size_t)n;
        }

        long fanMax = -1, instMW = -1, avgMW = -1, enforcedMW = -1, tgpmW = -1;
        NvU32 pstate = 0xffffffff;
        NvU32 gpcKHz = 0, memKHz = 0, smKHz = 0;
        if (rusd_copy(&sd->fanCoolerStatus.lastModifiedTimestamp,
                      &sd->fanCoolerStatus, &fan, sizeof fan) == 0)
            for (unsigned i = 0; i < RUSD_FAN_COOLER_MAX_COOLERS; i++)
                if ((long)fan.info.rpmCurr[i] > fanMax) fanMax = (long)fan.info.rpmCurr[i];
        if (rusd_copy(&sd->instPowerUsage.lastModifiedTimestamp,
                      &sd->instPowerUsage, &ipw, sizeof ipw) == 0)
            instMW = (long)ipw.info.instGpuPower;
        if (rusd_copy(&sd->avgPowerUsage.lastModifiedTimestamp,
                      &sd->avgPowerUsage, &apw, sizeof apw) == 0)
            avgMW = (long)apw.info.averageGpuPower;
        if (rusd_copy(&sd->powerLimitGpu.lastModifiedTimestamp,
                      &sd->powerLimitGpu, &pl, sizeof pl) == 0)
            enforcedMW = (long)pl.info.enforcedmW;
        if (rusd_copy(&sd->powerPolicyStatus.lastModifiedTimestamp,
                      &sd->powerPolicyStatus, &pol, sizeof pol) == 0)
            tgpmW = (long)pol.info.tgpmW;
        if (rusd_copy(&sd->perfCurrentPstate.lastModifiedTimestamp,
                      &sd->perfCurrentPstate, &pst, sizeof pst) == 0)
            pstate = pst.currentPstate;
        if (rusd_copy(&sd->clkPublicDomainInfos.lastModifiedTimestamp,
                      &sd->clkPublicDomainInfos, &clks, sizeof clks) == 0) {
            gpcKHz = clks.info[RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS].actualClkKHz;
            memKHz = clks.info[RUSD_CLK_PUBLIC_DOMAIN_MEMORY].actualClkKHz;
            smKHz  = clks.info[RUSD_CLK_PUBLIC_DOMAIN_SM].actualClkKHz;
        }

        int valid = tbuf[0] || fanMax >= 0 || instMW >= 0 || avgMW >= 0 ||
                    enforcedMW >= 0 || tgpmW >= 0 || pstate != 0xffffffffu ||
                    gpcKHz || memKHz || smKHz;
        nvx_emit(c, "V2.12b", "rusd-readout", 0, valid ? NVX_V_OK : NVX_V_SKIP,
                 "temp[%s] pstate %u gpc %u mem %u sm %u kHz inst %ld avg %ld mW "
                 "limit %ld tgp %ld mW fan %ld rpm",
                 tbuf, pstate, gpcKHz, memKHz, smKHz,
                 instMW, avgMW, enforcedMW, tgpmW, fanMax);
    }

    /* Keep the page alive for stage 3/4 sampling; connection teardown reclaims it. */
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
    item_bios_sku(c);
    item_bus(c);
    item_perf_levels(c);
    if (NVX_IDLE_ONLY) {
        nvx_emit(c, "V2.16", "i2c-read-buffer", 0, NVX_V_SKIP,
                 "idle-only build: I2C reachability probe disabled");
        nvx_emit(c, "V2.legacy", "legacy-probes", 0, NVX_V_SKIP,
                 "idle-only build: legacy command probes disabled");
    } else {
        item_i2c_probe(c);
        item_legacy_probes(c);
    }
    item_rusd(c);
    return 0;
}

/* ------------------------------------------------------------------ */
/* stage 3 — gated write tests                                         */
/* ------------------------------------------------------------------ */

static nvx_ctx *g_boost_ctx;

/* RATED_TDP restore state (kept separate from the boost latch so one write
 * path can never trigger the other's restore). Armed only after a successful
 * pre-record, so an interrupted run always returns the client to its prior
 * action. */
static nvx_ctx *g_rtdp_ctx;
static int g_rtdp_armed;
static NvU32 g_rtdp_client;
static NvU32 g_rtdp_orig_action;
static NvU32 g_rtdp_orig_vpstate;

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

static void rated_tdp_restore(nvx_ctx *c)
{
    NV2080_CTRL_PERF_RATED_TDP_SET_CONTROL_PARAMS s;
    memset(&s, 0, sizeof s);
    s.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)g_rtdp_client;
    s.input = (NV2080_CTRL_PERF_RATED_TDP_ACTION)g_rtdp_orig_action;
    s.vPstateType = (NV2080_CTRL_PERF_RATED_TDP_VPSTATE_TYPE)g_rtdp_orig_vpstate;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_SET_CONTROL, &s);

    NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS g;
    memset(&g, 0, sizeof g);
    g.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)g_rtdp_client;
    NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &g);
    printf("[..] restoring RATED_TDP client %s(%u) -> %s(%u): SET 0x%x, GET 0x%x%s\n",
           rated_tdp_client_name(g_rtdp_client), g_rtdp_client,
           rated_tdp_action_name(g_rtdp_orig_action), g_rtdp_orig_action,
           st, st2, (st2 == 0 && g.input == g_rtdp_orig_action) ? " (verified)" : "");
}

void nvx_restore_run(void)
{
    if (g_boost_ctx && g_boost_ctx->hSubdevice) {
        printf("[..] restoring: clearing PERF_BOOST latched state\n");
        boost_clear(g_boost_ctx);
        g_boost_ctx = NULL;
    }
    if (g_rtdp_armed && g_rtdp_ctx && g_rtdp_ctx->hSubdevice) {
        rated_tdp_restore(g_rtdp_ctx);
        g_rtdp_armed = 0;
        g_rtdp_ctx = NULL;
    }
}

int nvx_stage3(nvx_ctx *c)
{
    if (NVX_IDLE_ONLY) {
        nvx_emit(c, "V3", "writes", 0, NVX_V_GATED,
                 "idle-only build: all performance writes disabled");
        return -1;
    }
    if (!c->hSubdevice) {
        nvx_emit(c, "V3", "writes", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    /* --- V3.1 PERF_BOOST cycle (the NVK-proven write) ---
     * One-step boost first, then boost-to-max; every phase is cleared. */
    {
        nvx_emit(c, "V3.1", "perf-boost-cycle", 0, NVX_V_OK, "sampling baseline...");
        nvx_sample_once(c, "pre");

        NV2080_CTRL_PERF_BOOST_PARAMS bp;
        memset(&bp, 0, sizeof bp);
        bp.flags = NV2080_CTRL_PERF_BOOST_FLAGS_CMD_BOOST_1LEVEL |
                   (NV2080_CTRL_PERF_BOOST_FLAGS_CUDA_YES << 4);
        bp.duration = NV2080_CTRL_PERF_BOOST_DURATION_INFINITE;
        NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_BOOST, &bp);
        nvx_emit(c, "V3.1a", "perf-boost-1level", st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 st == 0 ? "BOOST_1LEVEL accepted" : "-");
        if (st == 0) { sleep(2); nvx_sample_once(c, "boost1"); boost_clear(c); sleep(1); }

        memset(&bp, 0, sizeof bp);
        bp.flags = NV2080_CTRL_PERF_BOOST_FLAGS_CMD_BOOST_TO_MAX |
                   (NV2080_CTRL_PERF_BOOST_FLAGS_CUDA_YES << 4);
        bp.duration = NV2080_CTRL_PERF_BOOST_DURATION_INFINITE;
        st = SUBCTL(c, NV2080_CTRL_CMD_PERF_BOOST, &bp);
        nvx_emit(c, "V3.1b", "perf-boost-to-max", st,
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

/* Gated RATED_TDP SET: the lowest-risk performance write candidate. Reached
 * only via `--write --rated-tdp-set C:A`. Sequence is mandatory and ordered:
 * pre-record -> arm restore -> SET -> readback. If the pre-record GET fails we
 * refuse to write, because we could not guarantee a restore. */
int nvx_stage3_rated_tdp_set(nvx_ctx *c, NvU32 client, NvU32 action)
{
    if (NVX_IDLE_ONLY) {
        nvx_emit(c, "V3.4", "rated-tdp-set", 0, NVX_V_GATED,
                 "idle-only build: performance writes disabled");
        return -1;
    }
    if (!c->hSubdevice) {
        nvx_emit(c, "V3.4", "rated-tdp-set", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    if (client >= NV2080_CTRL_PERF_RATED_TDP_CLIENT_NUM_CLIENTS) {
        nvx_emit(c, "V3.4", "rated-tdp-set", 0, NVX_V_SKIP,
                 "client %u out of range [0,%u)", client,
                 NV2080_CTRL_PERF_RATED_TDP_CLIENT_NUM_CLIENTS);
        return -1;
    }
    if (client == NV2080_CTRL_PERF_RATED_TDP_CLIENT_RM) {
        nvx_emit(c, "V3.4", "rated-tdp-set", 0, NVX_V_SKIP,
                 "client RM(0) cannot be set (per ctrl2080perf.h:425); it is RM-internal");
        return -1;
    }

    /* 1. pre-record the client's current request; refuse to write without it. */
    NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS g;
    memset(&g, 0, sizeof g);
    g.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)client;
    NvU32 st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &g);
    if (st != 0) {
        nvx_emit(c, "V3.4a", "rated-tdp-pre-record", st, NVX_V_REJECTED,
                 "GET before SET failed; refusing to write (restore not guaranteed)");
        return -1;
    }
    nvx_emit(c, "V3.4a", "rated-tdp-pre-record", st, NVX_V_OK,
             "client %s(%u) original action %s(%u) vPstate %u",
             rated_tdp_client_name(client), client,
             rated_tdp_action_name((NvU32)g.input), (NvU32)g.input,
             (NvU32)g.vPstateType);

    /* 2. arm the dedicated restore path before mutating anything. */
    g_rtdp_ctx = c;
    g_rtdp_client = client;
    g_rtdp_orig_action = (NvU32)g.input;
    g_rtdp_orig_vpstate = (NvU32)g.vPstateType;
    g_rtdp_armed = 1;

    /* 3. SET the requested action. */
    NV2080_CTRL_PERF_RATED_TDP_SET_CONTROL_PARAMS s;
    memset(&s, 0, sizeof s);
    s.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)client;
    s.input = (NV2080_CTRL_PERF_RATED_TDP_ACTION)action;
    s.vPstateType = (NV2080_CTRL_PERF_RATED_TDP_VPSTATE_TYPE)g_rtdp_orig_vpstate;
    st = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_SET_CONTROL, &s);
    nvx_emit(c, "V3.4b", "rated-tdp-set", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED,
             "client %s(%u) <- %s(%u) (rc 0x%x %s)",
             rated_tdp_client_name(client), client,
             rated_tdp_action_name(action), action, st, nvx_status_name(st));

    /* 4. readback and compare. */
    NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS r;
    memset(&r, 0, sizeof r);
    r.client = (NV2080_CTRL_PERF_RATED_TDP_CLIENT)client;
    NvU32 st2 = SUBCTL(c, NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL, &r);
    int ok = (st2 == 0 && (NvU32)r.input == action);
    nvx_emit(c, "V3.4c", "rated-tdp-readback", st2,
             ok ? NVX_V_OK : NVX_V_REJECTED,
             "after SET: action %s(%u) (GET 0x%x) vPstate %u; restore-on-exit armed -> %s(%u)",
             rated_tdp_action_name((NvU32)r.input), (NvU32)r.input, st2,
             (NvU32)r.vPstateType,
             rated_tdp_action_name(g_rtdp_orig_action), g_rtdp_orig_action);

    /* 5. restore now (proving the round-trip) and disarm; atexit/SIGINT would
     * have done the same had we died in between. */
    rated_tdp_restore(c);
    g_rtdp_armed = 0;
    g_rtdp_ctx = NULL;
    return st == 0 ? 0 : 1;
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

/* ------------------------------------------------------------------ */
/* stage 5 — channel smoke: make the GPU execute a pushbuffer          */
/*                                                                     */
/* Port of NullMoth's own push_test (kexts/NVRM/push.inc): sysmem/VRAM */
/* buffers -> FERMI_VASPACE -> NV01_CONTEXT_DMA notifier -> GPFIFO     */
/* channel -> BIND/SCHEDULE -> semaphore release via USERD GP put +    */
/* TURING_USERMODE_A doorbell. PASS = the GPU executed work under      */
/* macOS through our private client.                                   */
/* ------------------------------------------------------------------ */

struct nvx_buf {
    NvHandle hPhys, hVirt;
    NvU64 gpuva;
    volatile uint8_t *cpu;
    NvU64 size;
};

static NvHandle g_smoke_next = 0xcafe0100;

static int smoke_alloc_buf(nvx_ctx *c, NvHandle hVaSpace, int vidmem,
                           NvU64 size, const char *what, struct nvx_buf *b,
                           NvU32 *st)
{
    memset(b, 0, sizeof *b);
    b->size = size;
    NV_MEMORY_ALLOCATION_PARAMS mp;
    memset(&mp, 0, sizeof mp);
    mp.owner = c->hClient;
    mp.type = NVOS32_TYPE_IMAGE;
    mp.size = size;
    mp.attr = DRF_DEF(OS32, _ATTR, _PAGE_SIZE, _4KB) |
              (vidmem ? (DRF_DEF(OS32, _ATTR, _LOCATION, _VIDMEM) |
                         DRF_DEF(OS32, _ATTR, _COHERENCY, _UNCACHED))
                      : (DRF_DEF(OS32, _ATTR, _LOCATION, _PCI) |
                         DRF_DEF(OS32, _ATTR, _COHERENCY, _CACHED)));
    if (vidmem) mp.flags |= NVOS32_ALLOC_FLAGS_PERSISTENT_VIDMEM;
    b->hPhys = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice, g_smoke_next++,
                            vidmem ? NVX_CLASS_MEMORY_LOCAL_USER
                                   : NVX_CLASS_MEMORY_SYSTEM,
                            &mp, sizeof mp, st);
    if (!b->hPhys) return -1;

    b->cpu = nvx_map_memory(c->ctl_fd, c->dev_fd, c->hClient, c->hSubdevice,
                            b->hPhys, size, NULL);
    if (!b->cpu) return -1;

    NV_MEMORY_ALLOCATION_PARAMS vp;
    memset(&vp, 0, sizeof vp);
    vp.owner = c->hClient;
    vp.type = NVOS32_TYPE_IMAGE;
    vp.flags = NVOS32_ALLOC_FLAGS_VIRTUAL;
    vp.size = size;
    vp.hVASpace = hVaSpace;
    b->hVirt = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice, g_smoke_next++,
                            NVX_CLASS_MEMORY_VIRTUAL_NV50, &vp, sizeof vp, st);
    if (!b->hVirt) return -1;
    b->gpuva = vp.offset;   /* RM fills in the allocated VA */

    NVOS46_PARAMETERS d;
    memset(&d, 0, sizeof d);
    d.hClient = c->hClient;
    d.hDevice = c->hDevice;
    d.hDma = b->hVirt;
    d.hMemory = b->hPhys;
    d.offset = 0;
    d.length = size;
    d.flags = DRF_DEF(OS46, _FLAGS, _PAGE_KIND, _VIRTUAL) |
              (vidmem ? DRF_DEF(OS46, _FLAGS, _CACHE_SNOOP, _DISABLE)
                      : DRF_DEF(OS46, _FLAGS, _CACHE_SNOOP, _ENABLE));
    d.dmaOffset = 0;
    if (nvx_esc(c->ctl_fd, NV_ESC_RM_MAP_MEMORY_DMA, &d, sizeof d) != 0 || d.status != 0)
        return -1;
    return 0;
}

static void smoke_free_buf(nvx_ctx *c, struct nvx_buf *b)
{
    if (b->hVirt) nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, b->hVirt);
    if (b->hPhys) nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, b->hPhys);
    memset(b, 0, sizeof *b);
}

int nvx_stage5(nvx_ctx *c)
{
    if (NVX_IDLE_ONLY) {
        nvx_emit(c, "V5", "channel-smoke", 0, NVX_V_GATED,
                 "idle-only build: GPU channel submission disabled");
        return -1;
    }
    if (!c->hSubdevice || c->dev_fd < 0) {
        nvx_emit(c, "V5", "channel-smoke", 0, NVX_V_SKIP,
                 "needs RM chain + device connection");
        return -1;
    }

    /* pick the newest GPFIFO channel class from the classlist (0x??6f) */
    static NV0080_CTRL_GPU_GET_CLASSLIST_V2_PARAMS cls;
    memset(&cls, 0, sizeof cls);
    NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hDevice,
                              NV0080_CTRL_CMD_GPU_GET_CLASSLIST_V2, &cls, sizeof cls);
    NvU32 chanClass = 0;
    if (st == 0)
        for (NvU32 i = 0; i < cls.numClasses && i < NV0080_CTRL_GPU_CLASSLIST_MAX_SIZE; i++)
            if ((cls.classList[i] & 0xff) == 0x6f && cls.classList[i] > chanClass)
                chanClass = cls.classList[i];
    if (!chanClass) {
        nvx_emit(c, "V5.0", "pick-channel-class", st, NVX_V_REJECTED,
                 "no ??6f GPFIFO class in classlist");
        return -1;
    }
    nvx_emit(c, "V5.0", "pick-channel-class", 0, NVX_V_OK,
             "channel class 0x%x", chanClass);

    /* usermode doorbell page (subdevice-parented, 4 KB) */
    NvHandle hUm = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hSubdevice,
                                g_smoke_next++, NVX_CLASS_USERMODE_TURING_A,
                                NULL, 0, &st);
    if (!hUm) {
        nvx_emit(c, "V5.1", "alloc-usermode", st, NVX_V_REJECTED,
                 "TURING_USERMODE_A rejected");
        return -1;
    }
    volatile uint8_t *umap = nvx_map_memory(c->ctl_fd, c->dev_fd, c->hClient,
                                            c->hSubdevice, hUm, 0x1000, NULL);
    if (!umap) {
        nvx_emit(c, "V5.1", "alloc-usermode", 0, NVX_V_REJECTED,
                 "TURING_USERMODE_A 0x%08x alloc ok, map failed", hUm);
        return -1;
    }
    nvx_emit(c, "V5.1", "alloc-usermode", 0, NVX_V_OK,
             "doorbell page mapped at %p", (const void *)umap);

    /* address space */
    NV_VASPACE_ALLOCATION_PARAMETERS vap;
    memset(&vap, 0, sizeof vap);
    vap.flags = NV_VASPACE_ALLOCATION_FLAGS_RETRY_PTE_ALLOC_IN_SYS;
    NvHandle hVa = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice,
                                g_smoke_next++, NVX_CLASS_VASPACE_FERMI_A,
                                &vap, sizeof vap, &st);
    if (!hVa) {
        nvx_emit(c, "V5.2", "alloc-vaspace", st, NVX_V_REJECTED,
                 "FERMI_VASPACE_A rejected");
        return -1;
    }
    nvx_emit(c, "V5.2", "alloc-vaspace", 0, NVX_V_OK, "handle 0x%08x", hVa);

    /* buffers (push.inc sizes) */
    struct nvx_buf notifier, userd, gpfifo, cmdbuf, sem;
    int fail = 0;
    fail |= smoke_alloc_buf(c, hVa, 0, 0x1000,  "notifier", &notifier, &st) != 0;
    fail |= smoke_alloc_buf(c, hVa, 1, 0x80000, "userd",    &userd,    &st) != 0;
    fail |= smoke_alloc_buf(c, hVa, 0, 0x40000, "gpfifo",   &gpfifo,   &st) != 0;
    fail |= smoke_alloc_buf(c, hVa, 0, 0x10000, "cmdbuf",   &cmdbuf,   &st) != 0;
    fail |= smoke_alloc_buf(c, hVa, 0, 0x1000,  "sem",      &sem,      &st) != 0;
    if (fail) {
        nvx_emit(c, "V5.3", "alloc-buffers", st, NVX_V_REJECTED,
                 "one or more buffers failed (see kext log)");
        return -1;
    }
    nvx_emit(c, "V5.3", "alloc-buffers", 0, NVX_V_OK,
             "notifier/userd(vidmem)/gpfifo/cmdbuf/sem mapped, sem VA 0x%llx",
             (unsigned long long)sem.gpuva);
    memset((void *)sem.cpu, 0, 0x1000);
    memset((void *)notifier.cpu, 0, 0x1000);

    /* notifier context DMA */
    NV_CONTEXT_DMA_ALLOCATION_PARAMS cd;
    memset(&cd, 0, sizeof cd);
    cd.flags = DRF_DEF(OS03, _FLAGS, _MAPPING, _KERNEL) |
               DRF_DEF(OS03, _FLAGS, _HASH_TABLE, _DISABLE);
    cd.hMemory = notifier.hPhys;
    cd.offset = 0;
    cd.limit = notifier.size - 1;
    NvHandle hCtxDma = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice,
                                    g_smoke_next++, NVX_CLASS_CONTEXT_DMA,
                                    &cd, sizeof cd, &st);
    if (!hCtxDma) {
        nvx_emit(c, "V5.4", "alloc-ctxdma", st, NVX_V_REJECTED,
                 "NV01_CONTEXT_DMA rejected");
        return -1;
    }

    /* channel */
    NV_CHANNEL_ALLOC_PARAMS cp;
    memset(&cp, 0, sizeof cp);
    cp.hObjectError = hCtxDma;
    cp.gpFifoOffset = gpfifo.gpuva;
    cp.gpFifoEntries = 0x8000;
    cp.flags = 0;
    cp.hVASpace = hVa;
    cp.hUserdMemory[0] = userd.hPhys;
    cp.userdOffset[0] = 0;
    cp.engineType = NVX_ENGINE_TYPE_GRAPHICS;
    NvHandle hChan = nvx_rm_alloc(c->ctl_fd, c->hClient, c->hDevice,
                                  g_smoke_next++, chanClass, &cp, sizeof cp, &st);
    if (!hChan) {
        nvx_emit(c, "V5.5", "alloc-channel", st, NVX_V_REJECTED,
                 "channel class 0x%x rejected", chanClass);
        return -1;
    }
    nvx_emit(c, "V5.5", "alloc-channel", 0, NVX_V_OK,
             "channel 0x%08x (class 0x%x)", hChan, chanClass);

    NVA06F_CTRL_BIND_PARAMS bp;
    memset(&bp, 0, sizeof bp);
    bp.engineType = NVX_ENGINE_TYPE_GRAPHICS;
    st = nvx_rm_control(c->ctl_fd, c->hClient, hChan, NVA06F_CTRL_CMD_BIND,
                        &bp, sizeof bp);
    nvx_emit(c, "V5.6", "channel-bind", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED, "GRAPHICS engine");

    NVA06F_CTRL_GPFIFO_SCHEDULE_PARAMS sp;
    memset(&sp, 0, sizeof sp);
    sp.bEnable = NV_TRUE;
    st = nvx_rm_control(c->ctl_fd, c->hClient, hChan, NVA06F_CTRL_CMD_GPFIFO_SCHEDULE,
                        &sp, sizeof sp);
    nvx_emit(c, "V5.7", "gpfifo-schedule", st,
             st == 0 ? NVX_V_OK : NVX_V_REJECTED, "-");
    if (st != 0) return -1;

    NVC36F_CTRL_GPFIFO_SET_WORK_SUBMIT_TOKEN_NOTIF_INDEX_PARAMS ni;
    memset(&ni, 0, sizeof ni);
    ni.index = NV_CHANNELGPFIFO_NOTIFICATION_TYPE_WORK_SUBMIT_TOKEN;
    (void)nvx_rm_control(c->ctl_fd, c->hClient, hChan,
                         NVC36F_CTRL_CMD_GPFIFO_SET_WORK_SUBMIT_TOKEN_NOTIF_INDEX,
                         &ni, sizeof ni);
    NVC36F_CTRL_CMD_GPFIFO_GET_WORK_SUBMIT_TOKEN_PARAMS tk;
    memset(&tk, 0, sizeof tk);
    st = nvx_rm_control(c->ctl_fd, c->hClient, hChan,
                        NVC36F_CTRL_CMD_GPFIFO_GET_WORK_SUBMIT_TOKEN, &tk, sizeof tk);
    if (st != 0) {
        nvx_emit(c, "V5.8", "work-submit-token", st, NVX_V_REJECTED, "-");
        return -1;
    }
    nvx_emit(c, "V5.8", "work-submit-token", 0, NVX_V_OK,
             "token 0x%08x", tk.workSubmitToken);

    /* pushbuffer: SEMAPHORE method 0x5c releases value 1 at sem.gpuva */
    volatile uint32_t *pb = (volatile uint32_t *)cmdbuf.cpu;
    pb[0] = 0x20000000u | (5u << 16) | (0u << 13) | (0x5cu >> 2);
    pb[1] = (uint32_t)sem.gpuva & ~3u;
    pb[2] = (uint32_t)(sem.gpuva >> 32) & 0xffu;
    pb[3] = 1u;
    pb[4] = 0u;
    pb[5] = 1u | (1u << 20) | (1u << 24);
    const uint32_t ndw = 6;
    volatile uint32_t *gp = (volatile uint32_t *)gpfifo.cpu;
    gp[0] = (uint32_t)cmdbuf.gpuva & ~3u;
    gp[1] = ((uint32_t)(cmdbuf.gpuva >> 32) & 0xffu) | (ndw << 10);
    __sync_synchronize();
    volatile uint32_t *ud = (volatile uint32_t *)userd.cpu;
    ud[NVX_USERD_GPPUT / 4] = 1;
    __sync_synchronize();
    *(volatile uint32_t *)(umap + NVC361_NOTIFY_CHANNEL_PENDING) = tk.workSubmitToken;
    printf("  doorbell rung — waiting for the GPU semaphore...\n");

    volatile uint64_t *semv = (volatile uint64_t *)sem.cpu;
    int ok = 0;
    for (int i = 0; i < 5000; i++) {
        if (*semv == 1) { ok = 1; break; }
        usleep(1000);
    }
    nvx_emit(c, "V5.9", "gpu-semaphore", 0,
             ok ? NVX_V_OK : NVX_V_REJECTED,
             ok ? "GPU executed the pushbuffer under macOS (semaphore==1, <5s) "
                  "— engine submission path fully alive"
                : "timeout: USERD GPGet=%u GPPut=%u",
             ok ? 0 : ud[NVX_USERD_GPGET / 4], ok ? 0 : ud[NVX_USERD_GPPUT / 4]);

    /* teardown of channel objects (buffers/mappings die with the connection) */
    nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, hChan);
    nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, hCtxDma);
    nvx_rm_free(c->ctl_fd, c->hClient, c->hDevice, hVa);
    nvx_rm_free(c->ctl_fd, c->hClient, c->hSubdevice, hUm);
    smoke_free_buf(c, &notifier);
    smoke_free_buf(c, &userd);
    smoke_free_buf(c, &gpfifo);
    smoke_free_buf(c, &cmdbuf);
    smoke_free_buf(c, &sem);
    return ok ? 0 : -1;
}
