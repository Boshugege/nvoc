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
    nvx_emit(c, "V0.1", "open-ctl", 0, NVX_V_OK, "ctl connection opened (fd %d, service %s)",
             c->ctl_fd, nvx_service_name());

    c->dev_fd = nvx_open(0);
    if (c->dev_fd < 0) {
        nvx_emit(c, "V0.2", "open-dev", 0, NVX_V_SKIP, "no device connection; RUSD map tests limited");
    } else {
        nvx_emit(c, "V0.2", "open-dev", 0, NVX_V_OK,
                 "device connection opened (fd %d, service %s)", c->dev_fd,
                 nvx_service_name());
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

static int nvx_id_is_known_set(NvU32 cmd);   /* defined with the SET-id list below */
static int nvx_id_is_known_wedge(NvU32 cmd); /* ids that latch the GSP wedge */
static int nvx_status_is_wedge_stop(NvU32 st); /* 0x65/0x62 -> abort battery */

/* ------------------------------------------------------------------ */
/* Registered-control read pass (--reg-read)                           */
/* ------------------------------------------------------------------ */
/* The live NVOC export table of the loaded NVRM 610.57.04 build can be read
 * straight out of the Mach-O (probe-macos-rm/tools/extract-nvrm-method-table.py;
 * archived as outputs/method-table-nvrm-610.57.04.tsv).  Every case below is
 * a *public* control id sent with exactly the paramSize that table registers,
 * so this is a metadata-driven GET pass and not a size scan.  It serves two
 * purposes:
 *   (a) attest the extracted table end to end -- reaching the handler proves the
 *       recovered paramSize is the one the build registers, because
 *       rmapiutilGetControlInfo() rejects any other size with 0x1f *before*
 *       dispatch (control.c:448-458).  A 0x1f that still comes back at the
 *       registered size is therefore handler-side input rejection (an empty
 *       info list, a zero sample count), not a bad size -- the discrimination
 *       the earlier private-id passes could not make;
 *   (b) record which of the reachable reads can back stages 3-7 (fan /
 *       thermal / power / clock / topology).
 * Read-only by construction: every id is a Get/Info/Status/Samples command and
 * the whole list is checked against nvx_known_set_ids before being sent.
 */
typedef struct {
    const char *name;
    NvU32 cmd;
    unsigned size;
    const char *src;
} reg_read_case;

static const reg_read_case nvx_reg_reads[] = {
    /* anchors: already-characterised reads, kept so the run is self-checking */
    { "gpu-get-info-v2", 0x20800102u, 580, "live table (exact size)" },
    { "pmgr-get-module-info", 0x20802609u, 8, "known NV_OK anchor" },
    { "clk-pmumon-domains", 0x20801037u, 6024, "known 0x56 anchor" },
    { "internal-pm1-state", 0x20800aceu, 2, "known 0x56 anchor" },
    { "internal-vpstate-info", 0x20800ad4u, 8, "known 0x56 anchor" },
    { "internal-boost-sync-info", 0x20800a80u, 16, "known 0x56 anchor" },
    { "internal-edpp-limit-info", 0x20800afdu, 24, "known 0x56 anchor" },
    /* topology / capability reads: the missing half of the 776 B domain table */
    { "internal-gpu-chip-info", 0x20800a36u, 88, "topology" },
    { "internal-device-info-table", 0x20800a40u, 24580, "capability table" },
    { "internal-memsys-static-config", 0x20800a1cu, 40, "memory topology" },
    { "internal-gpu-hfrp-info", 0x20800a7au, 40, "PM table" },
    { "kgr-get-info", 0x20801201u, 32, "graphics topology" },
    { "kgr-get-gfx-gpc-tpc-info", 0x20801239u, 24, "GPC/TPC topology" },
    { "kgr-get-gpc-mask", 0x2080122au, 24, "GPC topology" },
    { "fb-get-info-v2", 0x20801303u, 1028, "FB/memory topology" },
    { "fb-get-static-bar1-info", 0x20801354u, 24, "BAR1 topology" },
    { "timer-gpu-cpu-correlation", 0x20800406u, 264, "clock correlation" },
    /* perf read surface at its registered sizes */
    { "perf-level-info-v2", 0x2080200bu, 780, "perf levels" },
    { "perf-get-powerstate", 0x2080205au, 4, "power state" },
    { "perf-get-current-pstate", 0x20802068u, 4, "P-state" },
    { "perf-get-vid-eng-perfmon", 0x20802087u, 12, "engine busy meters" },
    { "perf-get-gpumon-util-samples", 0x20802096u, 55888, "util samples" },
};

int nvx_stage2_reg_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.reg", "reg-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    unsigned buflen = 0;
    for (unsigned i = 0; i < sizeof nvx_reg_reads / sizeof nvx_reg_reads[0]; ++i)
        if (nvx_reg_reads[i].size > buflen) buflen = nvx_reg_reads[i].size;
    unsigned char *buf = malloc(buflen);
    if (!buf) {
        nvx_emit(c, "V2.reg", "reg-read", 0, NVX_V_HARD, "buffer alloc failed");
        return -1;
    }

    for (unsigned i = 0; i < sizeof nvx_reg_reads / sizeof nvx_reg_reads[0]; ++i) {
        const reg_read_case *k = &nvx_reg_reads[i];
        if (nvx_id_is_known_set(k->cmd)) {
            nvx_emit(c, "V2.reg", k->name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", k->cmd);
            continue;
        }
        memset(buf, 0, k->size);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  k->cmd, buf, k->size);
        unsigned nz = 0, first_nz = 0, last_nz = 0;
        for (unsigned o = 0; o < k->size; ++o)
            if (buf[o]) { ++nz; if (!first_nz) first_nz = o; last_nz = o; }
        nvx_emit(c, "V2.reg", k->name, st,
                 st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "cmd 0x%08x size %u (%s) -> non-zero %u B, first @%u last @%u; "
                 "head %02x %02x %02x %02x %02x %02x %02x %02x",
                 k->cmd, k->size, k->src, nz, first_nz, last_nz,
                 buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
    }
    free(buf);
    return 0;
}

/* ------------------------------------------------------------------ */
/* Perf-level clock-range read (--clklevel-read)                       */
/* ------------------------------------------------------------------ */
/* NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2 (0x2080200b) is the one clock-shaped
 * command the loaded 610.57.04 build *registers* at a fixed size, and that
 * size is exactly the published parameter block:
 *
 *   NV2080_CTRL_PERF_GET_LEVEL_INFO_V2_PARAMS {
 *       NvU32 level;                     // 0 standard, 1 conservative,
 *       NvU32 flags;                     // 2 aggressive; TYPE 0:0,
 *       NV2080_CTRL_PERF_GET_CLK_INFO perfGetClkInfoList[32];  // MODE 2:1
 *       NvU32 perfGetClkInfoListSize;
 *   };                                   // 4 + 4 + 32*24 + 4 = 780
 *   NV2080_CTRL_PERF_GET_CLK_INFO {
 *       NvU32 flags, domain, currentFreq, defaultFreq, minFreq, maxFreq;
 *   };                                   // 24 B
 *
 * (ctrl2080perf.h:678-696, 651-658.)  The record array has the same
 * 8 B-header + 32x24 B shape as the unattributed 0x20809061 @776 table, but
 * here the identity is *proven*: the id is in the driver's own NVOC export
 * table, so the reply is a first-class RM answer and every record field can be
 * named from the SDK header instead of guessed.  The pass asks for the three
 * published levels, one capacity-preset variant, and the TYPE_OVERCLOCK flag
 * bit, then decodes per-domain current/default/min/max -- the "current value
 * + range" evidence stages 4/6/7 were still missing.
 * Read-only: the table registers no SET twin for this id (GET_LEVEL_INFO_V2
 * only), and the pass writes nothing but the two input dwords.
 */
typedef struct {
    const char *name;
    NvU32 level;
    NvU32 flags;
    NvU32 list_size_in;    /* perfGetClkInfoListSize as sent */
} clk_level_case;

static const clk_level_case nvx_clk_levels[] = {
    { "lvl=standard     flags=0",            0, 0x0,  0 },
    { "lvl=conservative flags=0",            1, 0x0,  0 },
    { "lvl=aggressive   flags=0",            2, 0x0,  0 },
    { "lvl=standard     listSize=32",        0, 0x0, 32 },
    { "lvl=standard     flags=overclock",    0, 0x1,  0 },
    { "lvl=aggressive   flags=oc|maxperf",   2, 0x1 | 0x4, 0 },
};

int nvx_stage2_clk_level_read(nvx_ctx *c)
{
    const NvU32 cmd = 0x2080200bu;
    enum { SZ = 780 };

    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clklevel", "perf-get-level-info-v2", 0, NVX_V_SKIP,
                 "RM chain not open");
        return -1;
    }
    if (nvx_id_is_known_set(cmd)) {
        nvx_emit(c, "V2.clklevel", "perf-get-level-info-v2", 0, NVX_V_HARD,
                 "REFUSED: 0x%08x is a documented SET id", cmd);
        return -1;
    }

    unsigned char buf[SZ];
    unsigned ncases = sizeof nvx_clk_levels / sizeof nvx_clk_levels[0];
    for (unsigned k = 0; k < ncases; ++k) {
        const clk_level_case *q = &nvx_clk_levels[k];
        memset(buf, 0, sizeof buf);
        memcpy(buf + 0, &q->level, 4);
        memcpy(buf + 4, &q->flags, 4);
        memcpy(buf + 776, &q->list_size_in, 4);

        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  cmd, buf, sizeof buf);

        NvU32 out_size = 0, out_flags = 0, filled = 0;
        memcpy(&out_flags, buf + 4, 4);
        memcpy(&out_size, buf + 776, 4);
        for (unsigned i = 0; i < 32; ++i) {
            NvU32 d, cur, def, mn, mx, fl;
            const unsigned char *r = buf + 8 + 24 * i;
            memcpy(&fl, r + 0, 4);
            memcpy(&d, r + 4, 4);
            memcpy(&cur, r + 8, 4);
            memcpy(&def, r + 12, 4);
            memcpy(&mn, r + 16, 4);
            memcpy(&mx, r + 20, 4);
            if (!fl && !d && !cur && !def && !mn && !mx) continue;
            ++filled;
        }
        nvx_emit(c, "V2.clklevel", q->name, st,
                 st == 0 ? (filled ? NVX_V_OK : NVX_V_REJECTED) : NVX_V_REJECTED,
                 "cmd 0x%08x size %u in(level=%u flags=0x%x listSize=%u) -> "
                 "out flags 0x%08x listSize %u; %u of 32 clk-info records "
                 "non-zero", cmd, (unsigned)SZ, q->level, q->flags,
                 q->list_size_in, out_flags, out_size, filled);
        if (st != 0) continue;

        for (unsigned i = 0; i < 32; ++i) {
            NvU32 fl, d, cur, def, mn, mx;
            const unsigned char *r = buf + 8 + 24 * i;
            memcpy(&fl, r + 0, 4);
            memcpy(&d, r + 4, 4);
            memcpy(&cur, r + 8, 4);
            memcpy(&def, r + 12, 4);
            memcpy(&mn, r + 16, 4);
            memcpy(&mx, r + 20, 4);
            if (!fl && !d && !cur && !def && !mn && !mx) continue;
            nvx_emit(c, "V2.clklevel", q->name, st, NVX_V_OK,
                     "  clk[%2u] flags=0x%08x domain=0x%08x current=%u kHz "
                     "(%.3f MHz) default=%u min=%u max=%u kHz", i, fl, d, cur,
                     (double)cur / 1000.0, def, mn, mx);
        }
    }
    return 0;
}

/* ------------------------------------------------------------------ */
/* Clock-domain control retry, RM-layer only (--clkdom-read)           */
/* ------------------------------------------------------------------ */
/* Why this exists
 * ---------------
 * The earlier --clkdoc-read pass seeded the *documented* (version|size)
 * magic into dword 0 of every clock-domain parameter block.  Measuring
 * 0x2080B201 showed that those magics live in the NVAPI layer: the RM
 * parameter block itself carries no magic, and its dword 0 is an *output*
 * mask (docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md).
 * So the previous run poisoned dword 0 with a value the RM never expects.
 *
 * This pass is the retry with the magic removed, over a short, derived list
 * of sizes for the two clock-domain controls -- not a size scan:
 *
 *   0x20809019 ClockClkDomainsGetInfo     documented caller size 2520
 *                                        container-base candidate 2520-0x38
 *   0x2080901b ClockClkDomainsGetControl  documented caller size 2404 (control:
 *                                          isolates the magic, everything else
 *                                          identical to 2348)
 *                                        container-base candidate 2404-0x38
 *                                        RM head 12 + 32 * documented 72 B rec
 *                                        RM head 12 + 32 * 76 B (the stride
 *                                          measured for 0x2080B201)
 *
 * Only the documented controllable-domain mask is seeded, at the recorded +8.
 * All of these are GET; 0x2080d01c (the SET twin) is in nvx_known_set_ids and
 * is refused below.  Accepted cases are hex-dumped so a head/record layout can
 * be located without guessing offsets.
 */
typedef struct {
    const char *name;
    NvU32 cmd;
    unsigned size;
    NvU32 mask_off8;
} clkdom_case;

static const clkdom_case nvx_clkdom[] = {
    { "info 2520 documented",            0x20809019u, 2520, 0 },
    { "info 2464 = 2520-0x38",           0x20809019u, 2464, 0 },
    { "ctrl 2404 documented (control)",  0x2080901bu, 2404, 0xff },
    { "ctrl 2348 = 2404-0x38",           0x2080901bu, 2348, 0xff },
    { "ctrl 2316 = 12+32*72",            0x2080901bu, 2316, 0xff },
    { "ctrl 2316 mask=all",              0x2080901bu, 2316, 0xffffffffu },
    { "ctrl 2444 = 12+32*76",            0x2080901bu, 2444, 0xff },
};

int nvx_stage2_clkdom_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkdom", "clkdom-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    unsigned buflen = 0;
    for (unsigned i = 0; i < sizeof nvx_clkdom / sizeof nvx_clkdom[0]; ++i)
        if (nvx_clkdom[i].size > buflen) buflen = nvx_clkdom[i].size;
    unsigned char *buf = malloc(buflen);
    if (!buf) {
        nvx_emit(c, "V2.clkdom", "clkdom-read", 0, NVX_V_HARD, "buffer alloc failed");
        return -1;
    }

    for (unsigned i = 0; i < sizeof nvx_clkdom / sizeof nvx_clkdom[0]; ++i) {
        const clkdom_case *k = &nvx_clkdom[i];
        if (nvx_id_is_known_set(k->cmd)) {
            nvx_emit(c, "V2.clkdom", k->name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", k->cmd);
            continue;
        }
        memset(buf, 0, k->size);
        /* dword 0 stays ZERO on purpose: the (version|size) magics are
         * NVAPI-layer and must not be written into the RM parameter block. */
        memcpy(buf + 8, &k->mask_off8, 4);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  k->cmd, buf, k->size);

        unsigned nz = 0, first = 0, last = 0;
        for (unsigned o = 0; o < k->size; ++o)
            if (buf[o]) { ++nz; if (!first) first = o; last = o; }
        nvx_emit(c, "V2.clkdom", k->name, st,
                 st == 0 ? (nz ? NVX_V_OK : NVX_V_REJECTED) : NVX_V_REJECTED,
                 "cmd 0x%08x size %u dword0=0 mask@+8=0x%08x -> %u non-zero B, "
                 "first @%u last @%u", k->cmd, k->size, k->mask_off8, nz, first, last);
        if (st != 0) continue;

        unsigned nd = k->size / 4, shown = nd < 24 ? nd : 24;
        char hex[24 * 9 + 1];
        char *hp = hex;
        for (unsigned d = 0; d < shown; ++d) {
            NvU32 w; memcpy(&w, buf + 4 * d, 4);
            hp += snprintf(hp, 10, " %08x", w);
        }
        nvx_emit(c, "V2.clkdom", k->name, st, NVX_V_OK, "  dwords[0..%u):%s", shown, hex);
        /* non-zero dword positions, so a head/record base can be located */
        char off[24 * 12 + 1];
        char *op = off;
        unsigned listed = 0;
        for (unsigned d = 0; d < nd && listed < 24; ++d) {
            NvU32 w; memcpy(&w, buf + 4 * d, 4);
            if (!w) continue;
            op += snprintf(op, 13, " +%u=0x%08x", 4 * d, w);
            ++listed;
        }
        nvx_emit(c, "V2.clkdom", k->name, st, NVX_V_OK,
                 "  non-zero dwords (%u listed):%s", listed, off);
    }
    free(buf);
    return 0;
}


/* ------------------------------------------------------------------ */
/* GSP-side clock-domain offset read (--clkctl-read)                   */
/* ------------------------------------------------------------------ */
/* RESPONDER IDENTIFIED (2026-10-17)
 * ---------------------------------
 * The private clock/voltage ids (0x20809061, 0x2080B201, ...) answer NV_OK
 * even though the loaded NVRM's own NVOC export table has no row for them and
 * the literal id appears nowhere in the host binaries.  The path is:
 *
 *   NVRMUserClient::esc(NV_ESC_RM_CONTROL)            (kexts/NVRM/NVRM.cpp)
 *     -> rm_ioctl -> escape.c:793 case NV_ESC_RM_CONTROL
 *     -> Nv04ControlWithSecInfo -> _nv04ControlWithSecInfo
 *     -> RmDeprecatedGetControlHandler(pArgs)         (entry_points.c:498)
 *        - rmDeprecatedControlTable rows = host NVOC ids only
 *        - else IsGssLegacyCall(cmd) = !!(cmd & RM_GSS_LEGACY_MASK)
 *          with RM_GSS_LEGACY_MASK = 0x00008000
 *          (rmapi_deprecated.h:41) and IS_GSP_CLIENT(pGpu)
 *     -> RmGssLegacyRpcCmd()                          (rmapi_gss_legacy_control.c)
 *        "We no longer support these in RM, but ... just forward all of them
 *         to GSP and let it deal with what is or isn't valid."
 *        The only local checks are ptr/size null-consistency, the privileged
 *        bit (0xC000) against RS_PRIV_LEVEL_USER_ROOT, then
 *        pRmApi->Control(pGpu's physical RM API, cmd, params, paramsSize).
 *
 * So for this GPU (GSP client) the *responder is the GSP firmware*, and the
 * paramSize that matters is the GSP's own table entry, not the NVAPI caller
 * struct.  That explains every earlier observation at once:
 *   - no host table row and no id literal: the host only inspects bit 0x8000;
 *   - a *size-dependent* 0x1f: the GSP validates its own param size;
 *   - 0x2080FFFF -> 0x1b (INSUFFICIENT_PERMISSIONS): 0xFFFF has both legacy
 *     bits (0xC000 = privileged) and this process is not root;
 *   - 0x2080A0FF -> 0x56: forwarded, but the GSP has no such method;
 *   - --service nvrm and nvaccel byte-identical: both reach the same GSP.
 * Reproduce the check with: probe-macos-rm/tools/extract-gsp-method-table.py
 * on /Users/Shared/nvfw/nvidia/<ver>/gsp_tu10x.bin (ELF64/RISC-V, .fwimage).
 *
 * SIZES (from that table; do not confuse with the NVAPI-layer struct sizes):
 *   0x20809019 domains-get-info     12336     0x2080901b domains-get-control 2108
 *   0x20809006 measure-freq             8     0x2080901c                     808
 *   0x20809061 vfp(582) get-info      776     0x20809062 vfp(582) get-status 648
 *   0x20809021 vfp get-info         33288     0x20809022 vfp get-status     623112
 *   0x20809023 vfp get-control      66060     0x20809025 freq-ctrl info      7188
 *   0x20809026 freq-ctrl status      5332    0x2080b201 rails get-info      2444
 *   0x2080b202 rails get-status      3232    0x2080b213 rails get-control   1036
 * nvapi-rs clock.rs:1424 records the NVAPI *caller* struct as 0x964 = 2404
 * (mask@+8, records @+100 stride 72).  The RM block for the same control is
 * 2108, which is exactly 60 + 32*64, i.e. the same field offsets projected on
 * a 64-byte record (type @+0, offset_kHz @+44, range_min @+48, range_max @+52,
 * applied @+56, mask @+8 kept).  This pass TESTS that projection instead of
 * asserting it: it prints the driver-written dword map, then the decode.
 *
 * Read-only: every case is a documented GET; the SET twins
 * (0x2080d01c / 0x2080d024 / 0x2080f214) are in nvx_known_set_ids and are
 * refused before anything is sent.  0x208090a2 (524 B, GET or SET unknown) is
 * deliberately NOT sent here.
 */
typedef struct {
    const char *name;
    NvU32 cmd;
    unsigned size;
    NvU32 mask;          /* seeded at +8 when mask_off != ~0u */
    unsigned mask_off;
    int prefill;         /* 0 = zeroed, 1 = small dword counter          */
    int repeat_1s;       /* read twice, one second apart                 */
} clkctl_case;

static const clkctl_case nvx_clkctl[] = {
    /* The offset/range read: GSP-verified size 2108, not the NVAPI 2404. */
    { "domains-get-control 2108 mask=0xFF",  0x2080901bu, 2108, 0x000000ffu, 8, 0, 0 },
    { "domains-get-control 2108 mask=all",   0x2080901bu, 2108, 0xffffffffu, 8, 0, 0 },
    { "domains-get-control 2108 mask=0",     0x2080901bu, 2108, 0x00000000u, 8, 0, 0 },
    { "domains-get-control 2108 counter",    0x2080901bu, 2108, 0x000000ffu, 8, 1, 0 },
    /* The 2108 B answer puts a 0xFF domain mask at dword 0 and overwrites the
     * +8 seed, so the RM block's own input mask is at +0 -- test that. */
    { "domains-get-control 2108 in@0=0xFF",  0x2080901bu, 2108, 0x000000ffu, 0, 0, 0 },
    { "domains-get-control 2108 in@0=all",   0x2080901bu, 2108, 0xffffffffu, 0, 0, 0 },
    { "domains-get-control 2108 in@0=0x3",   0x2080901bu, 2108, 0x00000003u, 0, 0, 0 },
    { "domains-get-control 2108 in@0=FF cnt",0x2080901bu, 2108, 0x000000ffu, 0, 1, 0 },
    /* The a5-background sweep (see --clkctl-offsweep) showed the driver only
     * fills records when the input mask sits at +4; +0 and +8 are rejected
     * (0x25 NV_ERR_INVALID_DATA) unless they are zero.  These are the
     * zero-background repeats of that one derived offset. */
    { "domains-get-control 2108 in@4=0xFF",  0x2080901bu, 2108, 0x000000ffu, 4, 0, 0 },
    { "domains-get-control 2108 in@4=all",   0x2080901bu, 2108, 0xffffffffu, 4, 0, 0 },
    { "domains-get-control 2108 in@4=3",     0x2080901bu, 2108, 0x00000003u, 4, 0, 0 },
    { "domains-get-control 2108 in@4=FF cnt",0x2080901bu, 2108, 0x000000ffu, 4, 1, 0 },
    { "domains-get-info 12336",              0x20809019u, 12336, 0, ~0u, 0, 0 },
    { "measure-freq 8 (x2, 1 s)",            0x20809006u, 8, 0, ~0u, 0, 1 },
    { "vfp582 get-status 648",               0x20809062u, 648, 0, ~0u, 0, 0 },
    { "freq-ctrl-info 7188",                 0x20809025u, 7188, 0, ~0u, 0, 0 },
    /* 0x20809026@5332 is the id that latched the 2026-10-17 GSP wedge with an
     * all-zero block.  The case is kept here as documentation of why the
     * battery refuses it (nvx_known_wedge_ids) -- it must never be re-enabled
     * as a probe case. */
    { "freq-ctrl-status 5332 (refused)",      0x20809026u, 5332, 0, ~0u, 0, 0 },
    /* 0x2080901a is 2052 = 4 + 32*64 in the GSP table: the same 64 B record
     * shape as the 0x2080901b control, with a 4-byte instead of 60-byte head.
     * It is the natural "does a value-bearing variant exist" candidate, and it
     * was never read with a *zeroed* block (the only earlier attempt used a
     * 0xA5 background and came back 0x1f).  Zero background only. */
    { "gc-1a 2052 (4+32*64)",                0x2080901au, 2052, 0, ~0u, 0, 0 },
    { "gc-1a 2052 counter",                  0x2080901au, 2052, 0, ~0u, 1, 0 },
    /* 0x2080901d@32680 already answered NV_OK with a 0xA5 background (352/8170
     * dwords written); read it with a zeroed block so the actual values show. */
    { "gc-1d 32680 zeroed",                  0x2080901du, 32680, 0, ~0u, 0, 0 },
    /* Anchors: the two ids whose sizes were already confirmed empirically. */
    { "anchor vfp582 get-info 776",          0x20809061u, 776, 0, ~0u, 0, 0 },
    { "anchor rails get-info 2444",          0x2080b201u, 2444, 0, ~0u, 0, 0 },
    { "anchor rails get-control 1036",       0x2080b213u, 1036, 0, ~0u, 0, 0 },
    /* Negative anchors: same escape, same size, different legacy bits. */
    { "neg 0x2080a0ff 776 (non-priv, unknown)", 0x2080a0ffu, 776, 0, ~0u, 0, 0 },
    { "neg 0x2080ffff 776 (privileged bit)",    0x2080ffffu, 776, 0, ~0u, 0, 0 },
};

/* Classify every dword of the answer against the pre-send image:
 *   U = untouched (new == seed), Z = cleared (seed != 0, new == 0),
 *   W = written (new != seed and new != 0).
 * With a zero pre-seed the block starts all-U, so a W run is exactly what the
 * handler put there; with the small-counter pre-seed the Z runs show the region
 * the handler clears.  Output is chunked because nvx_emit's detail field is
 * 288 bytes -- an un-chunked line would silently truncate the layout.
 */
static void clkctl_dump(nvx_ctx *c, const clkctl_case *k, const unsigned char *buf,
                        const unsigned char *seed)
{
    unsigned nd = k->size / 4;
    unsigned written = 0, cleared = 0, untouched = 0;
    char cls[4096];              /* one class char per dword (max 3084) */
    unsigned nd_listed = 0;
    char woff[200];
    unsigned wo = 0;

    if (nd > sizeof cls - 1) nd = sizeof cls - 1;
    for (unsigned d = 0; d < nd; ++d) {
        NvU32 w, sv;
        memcpy(&w, buf + 4 * d, 4);
        memcpy(&sv, seed + 4 * d, 4);
        char cc = (w == sv) ? 'U' : (w == 0 ? 'Z' : 'W');
        if (cc == 'U') ++untouched;
        else if (cc == 'Z') ++cleared;
        else ++written;
        cls[d] = cc;
        if (cc == 'W' && wo < sizeof woff - 8 && nd_listed < 32)
            wo += (unsigned)snprintf(woff + wo, sizeof woff - wo, " +%u", 4 * d);
        if (cc == 'W') ++nd_listed;
    }
    cls[nd] = 0;
    woff[wo] = 0;
    nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_OK,
             "  dwords %u: U=%u W=%u Z=%u; W offsets:%s%s", nd, untouched,
             written, cleared, written ? woff : " none",
             nd_listed > 32 ? " ..." : "");
    if (!written) return;

    /* The written entries themselves, five per line. */
    {
        unsigned line = 0;
        char t[288];
        unsigned tn = 0;
        for (unsigned d = 0; d < nd; ++d) {
            if (cls[d] != 'W') continue;
            NvU32 w, sv;
            memcpy(&w, buf + 4 * d, 4);
            memcpy(&sv, seed + 4 * d, 4);
            tn += (unsigned)snprintf(t + tn, 52, "%s+%u:s%08x>w%08x", tn ? " " : "",
                                     4 * d, sv, w);
            if (++line == 5) {
                t[tn] = 0;
                nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_OK, "  W %s", t);
                line = 0; tn = 0;
            }
        }
        if (line) {
            t[tn] = 0;
            nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_OK, "  W %s", t);
        }
    }
    /* Class map, 120 dwords per line, index-prefixed. */
    for (unsigned o = 0; o < nd; o += 120) {
        unsigned n = nd - o < 120 ? nd - o : 120;
        char t[140];
        memcpy(t, cls + o, n);
        t[n] = 0;
        nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_OK, "  cls[%u..%u) %s", o, o + n, t);
    }
    /* Verbatim head, eight dwords per line, first 64 dwords. */
    {
        unsigned show = nd < 64 ? nd : 64;
        for (unsigned o = 0; o < show; o += 8) {
            char t[80];
            unsigned tn = 0;
            unsigned n = show - o < 8 ? show - o : 8;
            for (unsigned i = 0; i < n; ++i) {
                NvU32 w; memcpy(&w, buf + 4 * (o + i), 4);
                tn += (unsigned)snprintf(t + tn, 11, " %08x", w);
            }
            t[tn] = 0;
            nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_OK, "  +%03u:%s", 4 * o, t);
        }
    }
}

/* Build the parameter block for one case (zeroed or small-counter pre-seed),
 * seed the documented mask field, keep a copy of the pre-send image, and send
 * exactly one read-only control. */
static NvU32 clkctl_send(nvx_ctx *c, const clkctl_case *k,
                        unsigned char *buf, unsigned char *seed)
{
    memset(buf, 0, k->size);
    if (k->prefill) {
        for (unsigned d = 0; d < k->size / 4; ++d) {
            NvU32 v = d + 1;
            memcpy(buf + 4 * d, &v, 4);
        }
    }
    if (k->mask_off != ~0u) memcpy(buf + k->mask_off, &k->mask, 4);
    if (seed) memcpy(seed, buf, k->size);
    return nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                          k->cmd, buf, k->size);
}

int nvx_stage2_clkctl_read(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkctl", "clkctl-read", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    unsigned buflen = 0;
    for (unsigned i = 0; i < sizeof nvx_clkctl / sizeof nvx_clkctl[0]; ++i)
        if (nvx_clkctl[i].size > buflen) buflen = nvx_clkctl[i].size;
    unsigned char *buf = malloc(buflen), *seed = malloc(buflen);
    if (!buf || !seed) {
        nvx_emit(c, "V2.clkctl", "clkctl-read", 0, NVX_V_HARD, "buffer alloc failed");
        free(buf); free(seed);
        return -1;
    }

    for (unsigned i = 0; i < sizeof nvx_clkctl / sizeof nvx_clkctl[0]; ++i) {
        const clkctl_case *k = &nvx_clkctl[i];

        if (nvx_id_is_known_set(k->cmd)) {
            nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", k->cmd);
            continue;
        }
        if (nvx_id_is_known_wedge(k->cmd)) {
            nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x latched the GSP wedge on 2026-10-17; "
                     "id and payload cannot be separated, so it is never sent "
                     "by a battery (single deliberate action only)", k->cmd);
            continue;
        }

        NvU32 st = clkctl_send(c, k, buf, seed);

        if (nvx_status_is_wedge_stop(st)) {
            nvx_emit(c, "V2.clkctl", k->name, st, NVX_V_HARD,
                     "STOP: cmd 0x%08x size %u -> 0x%08x (%s); aborting the "
                     "remaining reads of this battery (a timeout can latch the "
                     "GSP legacy path for the whole session)",
                     k->cmd, k->size, st, nvx_status_name(st));
            break;
        }

        if (k->repeat_1s) {
            NvU32 a0 = 0, a1 = 0;
            if (k->size >= 8) { memcpy(&a0, buf, 4); memcpy(&a1, buf + 4, 4); }
            sleep(1);
            NvU32 st2 = clkctl_send(c, k, buf, seed);
            NvU32 b0 = 0, b1 = 0;
            if (k->size >= 8) { memcpy(&b0, buf, 4); memcpy(&b1, buf + 4, 4); }
            nvx_emit(c, "V2.clkctl", k->name, st, st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                     "cmd 0x%08x size %u -> 0x%08x: dwords [%08x %08x] then 1 s "
                     "later [%08x %08x] (second read 0x%08x)",
                     k->cmd, k->size, st, a0, a1, b0, b1, st2);
            continue;
        }

        if (st != 0) {
            nvx_emit(c, "V2.clkctl", k->name, st, NVX_V_REJECTED,
                     "cmd 0x%08x size %u mask@+%d=0x%08x -> 0x%08x (%s)",
                     k->cmd, k->size, k->mask_off == ~0u ? -1 : (int)k->mask_off,
                     k->mask, st, nvx_status_name(st));
            continue;
        }

        unsigned nz = 0, first = 0, last = 0;
        for (unsigned o = 0; o < k->size; ++o)
            if (buf[o]) { ++nz; if (!first) first = o; last = o; }
        nvx_emit(c, "V2.clkctl", k->name, st, nz ? NVX_V_OK : NVX_V_REJECTED,
                 "cmd 0x%08x size %u mask@+%d=0x%08x prefill=%d -> NV_OK, %u "
                 "non-zero bytes, first @%u last @%u",
                 k->cmd, k->size, k->mask_off == ~0u ? -1 : (int)k->mask_off,
                 k->mask, k->prefill, nz, first, last);
        if (!nz) continue;

        clkctl_dump(c, k, buf, seed);
    }

    free(buf); free(seed);
    return 0;
}

/* Whole-buffer hex dump of the read-only domain blocks (--clkctl-hex).
 * The dword-class map from --clkctl-read says *where* the GSP wrote; this
 * prints the actual bytes so the record stride and field offsets can be
 * decoded offline instead of guessed.  16 bytes per line, so the line stays
 * far inside nvx_emit's 288-byte detail field; only GET ids are sent.
 *
 * THIS IS THE AUTHORITATIVE RECORD for the block contents.  The 2026-10-17
 * 0xA5 sweep of 0x2080901b reported "106/527 dwords changed" but the captured
 * log kept only 40 of them (and 60 of the 352 dwords 0x2080901d reported), so
 * the delta printer alone is not a lossless record and a block whose tail was
 * dropped cannot be reconstructed offline.  A GET must not have to be repeated
 * just because the log format lost data -- after a GSP wedge it cannot be
 * repeated at all.  Every read-only clkctl id therefore gets a full dump here,
 * written to one file by tools/clock-offset-read-run.sh.
 *
 * Order matters: the ids are read in increasing confidence and the previously
 * least-known one (0x2080901d, 32680 B) goes last, so a timeout on it still
 * leaves the rest of the dump complete.  GET ids only; the SET twins
 * (0x2080d01c / 0x2080d024 / 0x2080f214) are refused below. */
int nvx_stage2_clkctl_hex(nvx_ctx *c)
{
    /* mask/mask_off: some blocks take a domain mask as *input*, and a zeroed
     * input selects nothing, so those cases must seed the mask to return a
     * populated block.  0x2080901b's input mask is at +4: with a 0xA5
     * background, seeding 0xFF at +4 is the only seed that answers NV_OK
     * (every other header dword -> 0x25), which is also why the zero-filled
     * --clkctl-read cases seeded at +8 returned NV_OK with an empty block.
     * The zero-background confirmation of +4 is in the same battery. */
    /* PREDICTIONS, so this battery tests the model instead of illustrating it.
     * The 0xA5 sweep reported 106/527 changed dwords for mask@+4 = 0xFF with
     * its last change at +564.  Both numbers are explained by exactly one
     * model: the mask selects *domain entries* (not domain bits), each filled
     * entry is a 64 B record at +60 + 64*i whose handler writes 13 dwords
     * (rec+0 type, rec+8..rec+44, rec+52, rec+56) and leaves rec+4, rec+48,
     * rec+60 alone -- so mask 0xFF = 8 entries = 2 + 8*13 = 106 changes, and
     * the last one is entry 7's rec+56 = 60 + 7*64 + 56 = 564.  The 9019
     * directory lists 10 entries (domain bits 0,1,4,2,3,20,19,6,21,5) with
     * record types 08,09,08,09,09,09,03,08,09,09, which is why mask 0xFF must
     * fill 8 records even though only 7 of its bits name a domain.
     *
     * Predicted non-zero bytes in a zeroed block (the type bytes are the only
     * non-zero output of this handler; everything else it writes is zero):
     *   mask 0x1   -> one record : +60 = 08 08                     (13+2 changed dwords)
     *   mask 0xFF  -> eight      : +60 08, +124 09, +188 08, +252 09,
     *                              +316 09, +380 09, +444 03, +508 08
     *   mask 0x3FF -> all ten    : the eight above plus +572 09, +636 09
     * A 0x25 answer would instead mean the value is out of range for this
     * field, which is a benign rejection (seen many times on this id).
     */
    static const struct {
        const char *name;
        NvU32 cmd;
        unsigned size;
        unsigned mask_off;   /* ~0u = no mask field */
        NvU32 mask;
    } cases[] = {
        { "domains-get-control 0x2080901B @2108 in@4=0xFF", 0x2080901bu, 2108u,
          4u, 0x000000ffu },
        { "domains-get-control 0x2080901B @2108 in@4=0x3FF", 0x2080901bu, 2108u,
          4u, 0x000003ffu },
        { "domains-get-control 0x2080901B @2108 in@4=0x1", 0x2080901bu, 2108u,
          4u, 0x00000001u },
        { "domains-get-control 0x2080901B @2108 in@4=0",    0x2080901bu, 2108u,
          4u, 0x00000000u },
        { "freq-ctrl-info   0x20809025 @7188",  0x20809025u, 7188u,  ~0u, 0u },
        { "domains-get-info 0x20809019 @12336", 0x20809019u, 12336u, ~0u, 0u },
        { "gc-1d             0x2080901D @32680", 0x2080901du, 32680u, ~0u, 0u },
    };

    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkctl", "clkctl-hex", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; ++i) {
        const char *name = cases[i].name;
        unsigned size = cases[i].size;

        if (nvx_id_is_known_set(cases[i].cmd)) {
            nvx_emit(c, "V2.clkctl", name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", cases[i].cmd);
            continue;
        }
        if (nvx_id_is_known_wedge(cases[i].cmd)) {
            nvx_emit(c, "V2.clkctl", name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a known GSP-wedge id", cases[i].cmd);
            continue;
        }

        unsigned char *buf = calloc(1, size);
        if (!buf) {
            nvx_emit(c, "V2.clkctl", name, 0, NVX_V_HARD, "buffer alloc failed");
            continue;
        }
        if (cases[i].mask_off != ~0u)
            memcpy(buf + cases[i].mask_off, &cases[i].mask, 4);

        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  cases[i].cmd, buf, size);
        if (st != 0) {
            nvx_emit(c, "V2.clkctl", name, st, NVX_V_REJECTED,
                     "cmd 0x%08x size %u -> 0x%08x (%s)", cases[i].cmd, size,
                     st, nvx_status_name(st));
            free(buf);
            if (nvx_status_is_wedge_stop(st)) {
                nvx_emit(c, "V2.clkctl", name, st, NVX_V_HARD,
                         "STOP: aborting the remaining hex dumps");
                break;
            }
            continue;
        }

        nvx_emit(c, "V2.clkctl", name, st, NVX_V_OK,
                 "cmd 0x%08x size %u -> NV_OK, %u-byte hex follows",
                 cases[i].cmd, size, size);
        for (unsigned o = 0; o < size; o += 16) {
            unsigned n = size - o < 16 ? size - o : 16;
            char t[128];
            unsigned tn = 0;
            tn += (unsigned)snprintf(t + tn, sizeof t - tn, "  %05u:", o);
            for (unsigned b = 0; b < n; ++b)
                tn += (unsigned)snprintf(t + tn, sizeof t - tn, " %02x", buf[o + b]);
            t[tn] = 0;
            nvx_emit(c, "V2.clkctl", name, st, NVX_V_OK, "%s", t);
        }
        free(buf);
    }
    return 0;
}

/* Bounded value-container probe (--clkctl-offsweep).
 *
 * Two questions left open by --clkctl-read: (a) does the 0x2080901b offset
 * container carry values at all on this GSP, and (b) is its input mask field
 * somewhere other than +0/+8?  A zero pre-seed cannot answer either, because a
 * driver write of zero is classified "untouched".  This probe therefore fills
 * the block with 0xA5A5A5A5 (every dword non-zero and identifiable), then runs
 * the 60-byte header one dword at a time with a 0xFF seed, printing exactly
 * which dwords each run changed and to what.  Every case is a documented GET
 * from the TU116 GSP's own export table; sizes come from that table (plus the
 * two accepted anchors at 776/1036), never from a scan.  SET ids refused. */
typedef struct {
    const char *name;
    NvU32 cmd;
    unsigned size;
    unsigned seed_off;
    NvU32 seed;
} offsweep_case;

static void offsweep_send(nvx_ctx *c, const offsweep_case *k)
{
    unsigned char *buf = malloc(k->size), *seed = malloc(k->size);
    if (!buf || !seed) {
        nvx_emit(c, "V2.clkctl", k->name, 0, NVX_V_HARD, "buffer alloc failed");
        free(buf); free(seed);
        return;
    }
    for (unsigned d = 0; d < k->size / 4; ++d) {
        NvU32 v = (k->seed_off == ~0u) ? 0u : 0xA5A5A5A5u;
        memcpy(buf + 4 * d, &v, 4);
    }
    if (k->seed_off != ~0u) memcpy(buf + k->seed_off, &k->seed, 4);
    memcpy(seed, buf, k->size);

    NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                              k->cmd, buf, k->size);
    if (st != 0) {
        nvx_emit(c, "V2.clkctl", k->name, st, NVX_V_REJECTED,
                 "cmd 0x%08x size %u seed@+%d -> 0x%08x (%s)", k->cmd, k->size,
                 k->seed_off == ~0u ? -1 : (int)k->seed_off, st,
                 nvx_status_name(st));
        free(buf); free(seed);
        return;
    }

    unsigned nd = k->size / 4, nch = 0, first = 0, last = 0, listed = 0;
    for (unsigned d = 0; d < nd; ++d) {
        NvU32 w, sv;
        memcpy(&w, buf + 4 * d, 4);
        memcpy(&sv, seed + 4 * d, 4);
        if (w != sv) { if (!nch) first = 4 * d; last = 4 * d; ++nch; }
    }
    nvx_emit(c, "V2.clkctl", k->name, st, nch ? NVX_V_OK : NVX_V_REJECTED,
             "cmd 0x%08x size %u seed@+%d=0x%08x -> NV_OK, %u/%u dwords changed"
             " (first @%u last @%u)", k->cmd, k->size,
             k->seed_off == ~0u ? -1 : (int)k->seed_off, k->seed, nch, nd,
             first, last);
    if (!nch) { free(buf); free(seed); return; }

    /* Changed dwords themselves, four per line: +off: seed>new. */
    {
        char t[280];
        unsigned tn = 0, per = 0;
        for (unsigned d = 0; d < nd; ++d) {
            NvU32 w, sv;
            memcpy(&w, buf + 4 * d, 4);
            memcpy(&sv, seed + 4 * d, 4);
            if (w == sv) continue;
            if (listed >= 600) break;
            tn += (unsigned)snprintf(t + tn, sizeof t - tn, "%s+%u:%08x>%08x",
                                     tn ? " " : "", 4 * d, sv, w);
            if (++per == 4) {
                t[tn] = 0;
                nvx_emit(c, "V2.clkctl", k->name, st, NVX_V_OK, "  %s", t);
                tn = 0; per = 0;
            }
            ++listed;
        }
        if (per) {
            t[tn] = 0;
            nvx_emit(c, "V2.clkctl", k->name, st, NVX_V_OK, "  %s", t);
        }
    }
    free(buf); free(seed);
}

int nvx_stage2_clkctl_offsweep(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkctl", "clkctl-offsweep", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }

    /* (a) locate the 0x2080901b input-mask field: one run per header dword. */
    for (unsigned off = 0; off <= 56; off += 4) {
        char nm[96];
        snprintf(nm, sizeof nm, "gc 2108 A5 seed+%u=0xFF", off);
        offsweep_case k = { nm, 0x2080901bu, 2108u, off, 0x000000ffu };
        if (nvx_id_is_known_set(k.cmd)) {
            nvx_emit(c, "V2.clkctl", k.name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", k.cmd);
            continue;
        }
        offsweep_send(c, &k);
    }
    /* all-ones mask at +0 as a separate datapoint (documented as rejected by
     * the NVAPI layer; the GSP may still accept it). */
    {
        offsweep_case k = { "gc 2108 A5 seed+0=all", 0x2080901bu, 2108u, 0u, 0xffffffffu };
        offsweep_send(c, &k);
    }

    /* (b) SAFETY: no further A5-filled ids.  The 2026-10-17 run of this sweep
     * fed 0xA5A5A5A5 into 0x20809026@5332 (and 0x2080901c@808 came back
     * 0x65 NV_ERR_TIMEOUT first); the GSP then latched 0x62
     * NV_ERR_RESET_REQUIRED for EVERY legacy-forwarded id (0x20809019/61 no
     * longer answer) and stayed that way for the rest of the session.  Only a
     * driver reload / reboot clears it, both of which are out of bounds here.
     * So the only ids this probe will ever fill with a non-zero background are
     * the ones that provably reject bad content safely (0x2080901b -> 0x25),
     * and the value containers are read with a ZERO background only. */
    {
        static const offsweep_case more[] = {
            { "gc 0x2080901a @2052", 0x2080901au, 2052u, ~0u, 0u },
            { "gc 0x2080901c @808",  0x2080901cu, 808u,  ~0u, 0u },
            { "gc 0x2080901d @32680",0x2080901du, 32680u,~0u, 0u },
            { "gc 0x20809025 @7188", 0x20809025u, 7188u, ~0u, 0u },
            /* 0x20809026 @5332 deliberately absent: it is the id that latched
             * the wedge (see nvx_known_wedge_ids).  It is refused below even
             * if a future edit re-adds it. */
        };
        for (unsigned i = 0; i < sizeof more / sizeof more[0]; ++i) {
            if (nvx_id_is_known_set(more[i].cmd)) {
                nvx_emit(c, "V2.clkctl", more[i].name, 0, NVX_V_HARD,
                         "REFUSED: 0x%08x is a documented SET id", more[i].cmd);
                continue;
            }
            if (nvx_id_is_known_wedge(more[i].cmd)) {
                nvx_emit(c, "V2.clkctl", more[i].name, 0, NVX_V_HARD,
                         "REFUSED: 0x%08x is a known GSP-wedge id", more[i].cmd);
                continue;
            }
            offsweep_send(c, &more[i]);
        }
    }
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

/* Ids that took the GSP's legacy path down for the whole session.
 * 0x20809026 @5332 (ClkFreqController status) did it on 2026-10-17: the call
 * returned 0x65 NV_ERR_TIMEOUT and every later legacy-forwarded id answered
 * 0x62 NV_ERR_RESET_REQUIRED until a reboot (see
 * docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md §6).
 * That run used an 0xA5A5A5A5-filled block, and no experiment separates "this
 * id" from "that content", so the id is refused regardless of payload instead
 * of being retried with a "safer" background.  0x2080901c @808 also timed out,
 * but that one was survivable; it is not listed here only because nothing in
 * the battery sends it.
 *
 * These are refused before the buffer is even built.  A deliberate single
 * attempt is a separate, explicitly-gated action, not a probe case. */
static const NvU32 nvx_known_wedge_ids[] = {
    0x20809026u, /* ClkFreqController GET_STATUS @5332 -- wedged the GSP */
};

static int nvx_id_is_known_wedge(NvU32 cmd)
{
    for (unsigned i = 0; i < sizeof nvx_known_wedge_ids / sizeof nvx_known_wedge_ids[0]; ++i)
        if (cmd == nvx_known_wedge_ids[i]) return 1;
    return 0;
}

/* A 0x65 timeout (or a latched 0x62) means "abort the battery", never "try a
 * different size": see the boundary in the doc §6. */
static int nvx_status_is_wedge_stop(NvU32 st)
{
    return st == 0x00000065u || st == 0x00000062u;
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
    2404, 262144, 401600, 623168, 737084, 1050688, 1349156, 4343300,
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
                     "payload dwords [%u..%u) :%s", base, base + n, hex);
        }
    }

    /* Stamped pass. The private clock/VF families stamp their params with
     * (version << 16 | size) -- 0x10964 is "V1, 2404 B", 0x12420 is "V1,
     * 9248 B", not a byte count. A zeroed buffer therefore cannot satisfy a
     * stamp-checking command no matter what size it is, which is the second
     * half of the 0x1f verdict above. Re-send each documented magic at the
     * size it encodes, with the magic in the first dword. */
    static const NvU32 stamps[] = {
        0x010964u,  /* ClockDomains control V1 (nvapi-rs clock.rs:360)          */
        0x0261A4u,  /* ClockDomains control V2 (same)                          */
        0x01182Cu,  /* V/F points INFO, Blackwell (nvapi-rs nvid.rs:757)        */
        0x012420u,  /* V/F points GET_CONTROL, Blackwell (nvapi-rs nvid.rs:758) */
        0x474604u,  /* V/F points canonical control stamp (p100 audit, 0x4604)  */
        0x014420u,  /* old V/F stamps the Windows translator accepts            */
        0x0620C0u,
        0x0B3F3Cu,
        0x149224u,
        0x01000Cu,  /* generic NV2080 12 B / 36 B stamps                        */
        0x010024u,
    };
    char stamp_text[192] = "";
    unsigned nstamp_ok = 0;
    int stamp_off = 0;
    for (unsigned i = 0; i < sizeof stamps / sizeof stamps[0]; ++i) {
        unsigned sz = stamps[i] & 0xffffu;
        if (sz < 4 || sz > 0xffffu) continue;
        memset(buf, 0, sz);
        memcpy(buf, &stamps[i], 4);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  cmd, buf, sz);
        if (st == 0) ++nstamp_ok;
        if (st == 0 && stamp_off < (int)sizeof stamp_text - 16)
            stamp_off += snprintf(stamp_text + stamp_off,
                                  sizeof stamp_text - (size_t)stamp_off,
                                  "0x%08x@%u ", stamps[i], sz);
    }
    nvx_emit(c, "V2.clkvf", name, nstamp_ok ? 0 : NV_ERR_INVALID_ARGUMENT,
             nstamp_ok ? NVX_V_OK : NVX_V_SKIP,
             "stamped pass over %u (version|size) magics: %s",
             (unsigned)(sizeof stamps / sizeof stamps[0]),
             nstamp_ok ? stamp_text : "none accepted (still 0x1f)");
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

/* ------------------------------------------------------------------ */
/* Documented-marshalling pass (--clkdoc-read)                          */
/* ------------------------------------------------------------------ */
/* The census above answered "which paramSizes does this driver accept" with
 * the driver's own INVALID_ARGUMENT oracle -- but it sent ZEROED buffers, and
 * every private clock/V-F read has TWO documented header requirements the
 * zeroed scan cannot satisfy:
 *
 *   1. a STAMP (dword[0]) the handler compares against a literal -- e.g.
 *      ClockClient domains control V1 = 0x10964, VFP GET_INFO = 0x78604
 *      (that one is a size, not version|size), VFP GET_CONTROL = 0x474604;
 *   2. an input SEED the caller must pre-fill -- the clock-domains control
 *      takes the requested domain mask at +8, the VFP GET_STATUS takes the
 *      point mask at +4..+132 copied out of GET_INFO, the rails control takes
 *      a rail-mask filter.
 *
 * So each case below sends exactly ONE (size, stamp, seed) triple taken from
 * the binding it cites, and the verdict is about that documented form rather
 * than about a scan. The table is checked against the documented SET ids
 * before anything is sent, and every case is a GET.
 */

typedef struct {
    const char *name;
    NvU32  cmd;
    unsigned size;      /* paramSize to send (the documented struct size) */
    NvU32  w0;          /* dword[0] stamp; 0 = leave as sent (zeroed) */
    unsigned seed_off;  /* byte offset of the u32 seed, ~0u = none */
    NvU32  seed_val;    /* seed value written at seed_off */
    unsigned dump;      /* dwords of payload to dump on accept (0 = default) */
    /* `src` records where the triple came from; printed so a rejection is
     * traceable to a specific binding, not to "we tried stuff". */
    const char *src;
} clkdoc_case;

static unsigned doc_census(const unsigned char *buf, unsigned sz,
                           unsigned *first, unsigned *last)
{
    unsigned nz = 0;
    *first = ~0u;
    *last = 0;
    for (unsigned off = 0; off + 4 <= sz; off += 4) {
        NvU32 v;
        memcpy(&v, buf + off, 4);
        if (!v) continue;
        if (*first == ~0u) *first = off;
        *last = off;
        ++nz;
    }
    return nz;
}

static void doc_dump(nvx_ctx *c, const char *name, const unsigned char *buf,
                     unsigned sz, unsigned max_words)
{
    unsigned words = sz / 4;
    if (words > max_words) words = max_words;
    for (unsigned base = 0; base < words; base += 16) {
        unsigned n = words - base < 16 ? words - base : 16;
        char hex[16 * 9 + 1];
        char *hp = hex;
        for (unsigned w = 0; w < n; ++w) {
            NvU32 v;
            memcpy(&v, buf + 4 * (base + w), 4);
            hp += snprintf(hp, 10, "%s%08x", w ? " " : "", v);
        }
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "payload dwords [%u..%u) :%s", base, base + n, hex);
    }
}

/* ClockClient clock-domains control (V1 magic 0x10964, 2404 B): mask@+8,
 * 32 x 72 B bit-sparse domain records @+100. Record: type u8@+0,
 * offset_kHz i32@+44, range_min@+48, range_max@+52, applied@+56. */
static void doc_decode_clk_ctrl(nvx_ctx *c, const char *name,
                                const unsigned char *buf, unsigned sz)
{
    if (sz < 108) return;
    NvU32 mask;
    memcpy(&mask, buf + 8, 4);
    unsigned filled = 0;
    char line[200];
    int off = 0;
    off += snprintf(line + off, sizeof line - (size_t)off,
                    "clock-domains control: seed/echo mask@+8=0x%08x;", mask);
    for (unsigned bit = 0; bit < 32 && off + 8 < (int)sizeof line; ++bit) {
        unsigned ro = 100 + 72 * bit;
        if (ro + 60 > sz) break;
        if (!buf[ro]) continue;
        ++filled;
    }
    if (!filled) {
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "clock-domains control: mask@+8=0x%08x, but 0 of 32 per-domain "
                 "records carry a non-zero type byte => no domain description "
                 "returned for this seed", mask);
        return;
    }
    nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
             "clock-domains control: mask@+8=0x%08x, %u of 32 per-domain "
             "records non-zero (see following lines)", mask, filled);
    for (unsigned bit = 0; bit < 32; ++bit) {
        unsigned ro = 100 + 72 * bit;
        if (ro + 60 > sz) break;
        if (!buf[ro]) continue;
        NvU32 off_k = 0, rmin = 0, rmax = 0, appl = 0;
        memcpy(&off_k, buf + ro + 44, 4);
        memcpy(&rmin, buf + ro + 48, 4);
        memcpy(&rmax, buf + ro + 52, 4);
        memcpy(&appl, buf + ro + 56, 4);
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "  bit %2u (0x%08x) type=0x%02x offset=%d kHz range[%d..%d] "
                 "applied=%d", bit, 1u << bit, buf[ro], (int)off_k,
                 (int)rmin, (int)rmax, (int)appl);
    }
}

/* The 8 B head + 24 B record table RM 0x20809061 ACKs at paramSize 776.
 * Each record carries a domain bit and a kHz-scale value. */
static void doc_decode_domain_table(nvx_ctx *c, const char *name,
                                    const unsigned char *buf, unsigned sz)
{
    if (sz < 32) return;
    NvU32 w0, w1;
    memcpy(&w0, buf, 4);
    memcpy(&w1, buf + 4, 4);
    nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
             "domain table: head [0]=0x%08x [1]=0x%08x; records @+8 stride 24",
             w0, w1);
    unsigned n = (sz - 8) / 24;
    for (unsigned i = 0; i < n; ++i) {
        const unsigned char *r = buf + 8 + 24 * i;
        NvU32 tag, bit, val, t1, t2, t3;
        memcpy(&tag, r + 0, 4);
        memcpy(&bit, r + 4, 4);
        memcpy(&val, r + 8, 4);
        memcpy(&t1, r + 12, 4);
        memcpy(&t2, r + 16, 4);
        memcpy(&t3, r + 20, 4);
        if (!tag && !bit && !val) continue;
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "  rec[%2u] tag=0x%08x bit=0x%08x value=%u kHz (%.3f MHz) "
                 "t1=%d t2=0x%08x t3=0x%08x",
                 i, tag, bit, val, (double)val / 1000.0, (int)t1, t2, t3);
    }
}

/* ClkFreqController directory (0x20809025, mask@+4, records @+0x4C stride
 * 0x60: supported@+0, type@+4, steps@+8, max_kHz@+0x2C, offsets@+0x38/0x3C)
 * and its mask-seeded status sibling (0x20809026). */
static void doc_decode_freq_ctrl(nvx_ctx *c, const char *name,
                                 const unsigned char *buf, unsigned sz)
{
    NvU32 mask;
    memcpy(&mask, buf + 4, 4);
    unsigned filled = 0;
    for (unsigned bit = 0; bit < 32; ++bit) {
        unsigned ro = 0x4C + 0x60 * bit;
        if (ro + 0x40 > sz) break;
        NvU32 sup;
        memcpy(&sup, buf + ro, 4);
        if (sup) ++filled;
    }
    nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
             "clk-freq-controller: mask@+4=0x%08x, %u of 32 records have a "
             "non-zero supported flag", mask, filled);
    for (unsigned bit = 0; bit < 32; ++bit) {
        unsigned ro = 0x4C + 0x60 * bit;
        if (ro + 0x40 > sz) break;
        NvU32 sup, steps, maxkhz, mn, mx;
        memcpy(&sup, buf + ro, 4);
        if (!sup) continue;
        memcpy(&steps, buf + ro + 8, 4);
        memcpy(&maxkhz, buf + ro + 0x2C, 4);
        memcpy(&mn, buf + ro + 0x38, 4);
        memcpy(&mx, buf + ro + 0x3C, 4);
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "  bit %2u type=%u steps=%u max=%u kHz (%.3f MHz) "
                 "offset range [%d..%d] kHz",
                 bit, buf[ro + 4], steps, maxkhz, (double)maxkhz / 1000.0,
                 (int)mn, (int)mx);
    }
}

/* VoltVoltRails GetControl (RM 0x2080B213): 32 B records, type byte @+0 and a
 * six-dword payload @+4 (voltrails-family-full-layout-r610.md). The measured
 * accept is 1036 B = 12 B head + 32 x 32 B. */
static void doc_decode_rails_ctrl(nvx_ctx *c, const char *name,
                                  const unsigned char *buf, unsigned sz)
{
    NvU32 h0 = 0, h1 = 0, h2 = 0;
    memcpy(&h0, buf, 4);
    memcpy(&h1, buf + 4, 4);
    memcpy(&h2, buf + 8, 4);
    nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
             "rails get-control: %u B = 12 B head + 32 x 32 B records; "
             "head dwords 0x%08x 0x%08x 0x%08x", sz, h0, h1, h2);
    for (unsigned i = 0; i < 32; ++i) {
        unsigned ro = 12 + 32 * i;
        NvU32 pay[6];
        if (ro + 32 > sz) break;
        if (!buf[ro]) continue;
        for (unsigned k = 0; k < 6; ++k) memcpy(&pay[k], buf + ro + 4 + 4 * k, 4);
        nvx_emit(c, "V2.clkdoc", name, 0, NVX_V_SKIP,
                 "  rec[%2u] type=0x%02x payload %08x %08x %08x %08x %08x %08x",
                 i, buf[ro], pay[0], pay[1], pay[2], pay[3], pay[4], pay[5]);
    }
}

static void item_clk_doc(nvx_ctx *c)
{
    static const clkdoc_case cases[] = {
        /* ---- ClockClient clock domains: GET_INFO then GET_CONTROL ---- */
        { "domains-get-info v1 2520", NVX_RM_CMD_CLK_DOMAINS_GET_INFO, 2520,
          NVX_STAMP_CLK_DOMAINS_INFO_V1, ~0u, 0, 32, "clock.rs:1930" },
        { "domains-get-info 610a", NVX_RM_CMD_CLK_DOMAINS_GET_INFO, 296620,
          NVX_STAMP_CLK_DOMAINS_INFO_610A, ~0u, 0, 0, "clock.rs:1933" },
        { "domains-get-info 610b", NVX_RM_CMD_CLK_DOMAINS_GET_INFO, 329388,
          NVX_STAMP_CLK_DOMAINS_INFO_610B, ~0u, 0, 0, "clock.rs:1933" },
        { "domains-get-control v1 noseed", NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL,
          2404, NVX_STAMP_CLK_DOMAINS_CONTROL_V1, ~0u, 0, 64,
          "clock.rs:1432 (mask@+8)" },
        { "domains-get-control v1 mask=ff",
          NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL, 2404,
          NVX_STAMP_CLK_DOMAINS_CONTROL_V1, 8, 0xFFu, 64, "clock.rs:1432" },
        { "domains-get-control v1 mask=all",
          NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL, 2404,
          NVX_STAMP_CLK_DOMAINS_CONTROL_V1, 8, 0xFFFFFFFFu, 64, "clock.rs:1432" },
        { "domains-get-control v2 mask=ff",
          NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL, 24996,
          NVX_STAMP_CLK_DOMAINS_CONTROL_V2, 8, 0xFFu, 32, "clock.rs:1456" },
        /* ---- Direct per-domain frequency measurement ---- */
        { "measure-freq v1 dom0", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V1, 4, 0, 8, "clock.rs:1535 (GPC)" },
        { "measure-freq v1 dom1", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V1, 4, 1, 8, "clock.rs:1535 (XBAR)" },
        { "measure-freq v1 dom2", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V1, 4, 2, 8, "clock.rs:1535 (SYS)" },
        { "measure-freq v1 dom4", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V1, 4, 4, 8, "clock.rs:1535 (MCLK)" },
        { "measure-freq v2 dom0", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V2, 4, 0, 8, "clock.rs:1552" },
        /* ---- V/F points: GET_INFO (directory) then GET_STATUS (curves) ---- */
        { "vfp-get-info 493060", NVX_RM_CMD_VFP_GET_INFO_582, 493060,
          NVX_STAMP_VFP_INFO, ~0u, 0, 24, "clock.rs:2480 (0x78604)" },
        { "vfp-get-info legacy 83996", NVX_RM_CMD_VFP_GET_INFO_582, 83996,
          NVX_STAMP_VFP_INFO_LEGACY, ~0u, 0, 24, "clock.rs:2505" },
        { "vfp-get-info wide 369796", NVX_RM_CMD_VFP_GET_INFO_582, 369796,
          NVX_STAMP_VFP_INFO_WIDE, ~0u, 0, 24, "clock.rs:2510" },
        { "vfp-get-info mid 157692", NVX_RM_CMD_VFP_GET_INFO_582, 157692,
          157692u, ~0u, 0, 24, "clock.rs:2516" },
        { "vfp-get-info mid 249844", NVX_RM_CMD_VFP_GET_INFO_582, 249844,
          249844u, ~0u, 0, 24, "clock.rs:2516" },
        { "vfp-get-info 776 (observed ACK)", NVX_RM_CMD_VFP_GET_INFO_582, 776,
          0, ~0u, 0, 64, "census stage-clkvf-read (accepted size)" },
        /* GET_STATUS needs its +4..+132 header seeded from GET_INFO; the
         * unseeded form is sent first so the two are distinguishable. */
        { "vfp-get-status 2000388 noseed", NVX_RM_CMD_VFP_GET_STATUS_582,
          2000388, NVX_STAMP_VFP_STATUS, ~0u, 0, 24, "clock.rs:2836" },
        /* ---- V/F control readback (applied offsets) ---- */
        { "vfp-get-control 0x474604", NVX_RM_CMD_VFP_GET_CONTROL, 4343300,
          NVX_STAMP_VFP_CONTROL, ~0u, 0, 24, "clock.rs:3471" },
        { "vfp-get-control snapshot", NVX_RM_CMD_VFP_GET_CONTROL, 82976,
          NVX_STAMP_VFP_CONTROL_SNAPSHOT, ~0u, 0, 24, "clock.rs:3478" },
        { "vfp-get-control 401472", NVX_RM_CMD_VFP_GET_CONTROL, 401472,
          401472u, ~0u, 0, 24, "clock.rs:3471" },
        { "vfp-get-control 737404", NVX_RM_CMD_VFP_GET_CONTROL, 737404,
          737404u, ~0u, 0, 24, "clock.rs:3471" },
        { "vfp-get-control 1348740", NVX_RM_CMD_VFP_GET_CONTROL, 1348740,
          1348740u, ~0u, 0, 24, "clock.rs:3471" },
        /* ---- TU116 ClkFreqController (this bench's GPU part) ---- */
        { "freq-ctrl-info 3148", NVX_RM_CMD_CLK_FREQ_CTRL_INFO, 3148,
          NVX_STAMP_CLK_FREQ_CTRL_INFO, ~0u, 0, 64, "clock.rs:5421" },
        { "freq-ctrl-status mask=7", NVX_RM_CMD_CLK_FREQ_CTRL_STATUS, 2508,
          NVX_STAMP_CLK_FREQ_CTRL_STATUS, 4, 0x7u, 32, "clock.rs:5466" },
        { "freq-ctrl-status mask=all", NVX_RM_CMD_CLK_FREQ_CTRL_STATUS, 2508,
          NVX_STAMP_CLK_FREQ_CTRL_STATUS, 4, 0xFFFFFFFFu, 32, "clock.rs:5466" },
        /* ---- Voltage rails (same op family as the two ACKs above) ---- */
        { "rails-get-control w0=0", NVX_RM_CMD_RAILS_GET_CONTROL, 1036,
          0, ~0u, 0, 64, "voltrails r610 (accepted size)" },
        { "rails-get-control byte15=0xff", NVX_RM_CMD_RAILS_GET_CONTROL, 1036,
          0, 12, 0xFF000000u, 64, "voltrails r610 (buf[15]=rail filter)" },
        { "rails-get-control byte15=0x01", NVX_RM_CMD_RAILS_GET_CONTROL, 1036,
          0, 12, 0x01000000u, 64, "voltrails r610 (buf[15]=rail filter)" },
        { "rails-get-info 2444", NVX_RM_CMD_RAILS_GET_INFO, 2444,
          0, ~0u, 0, 64, "voltrails r610 (12 + 32 x 76)" },
        { "rails-get-status 3212", NVX_RM_CMD_RAILS_GET_STATUS, 3212,
          0, ~0u, 0, 64, "voltrails r610 (12 + 32 x 100)" },
        { "rails-get-info esc 500008", NVX_RM_CMD_RAILS_GET_INFO, 500008,
          0, ~0u, 0, 16, "voltrails r610 (Windows esc buffer size)" },
    };
    const unsigned ncase = sizeof cases / sizeof cases[0];
    unsigned buflen = 0;
    for (unsigned i = 0; i < ncase; ++i)
        if (cases[i].size > buflen) buflen = cases[i].size;
    unsigned char *buf = malloc(buflen);
    if (!buf) {
        nvx_emit(c, "V2.clkdoc", "clkdoc-read", 0, NVX_V_HARD,
                 "doc buffer alloc (%u B) failed", buflen);
        return;
    }

    for (unsigned i = 0; i < ncase; ++i) {
        const clkdoc_case *k = &cases[i];
        if (nvx_id_is_known_set(k->cmd)) {
            nvx_emit(c, "V2.clkdoc", k->name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id; the read pass "
                     "never sends it", k->cmd);
            continue;
        }
        memset(buf, 0, k->size);
        if (k->w0) memcpy(buf, &k->w0, 4);
        if (k->seed_off != ~0u && k->seed_off + 4 <= k->size)
            memcpy(buf + k->seed_off, &k->seed_val, 4);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  k->cmd, buf, k->size);
        unsigned first = 0, last = 0;
        unsigned nz = doc_census(buf, k->size, &first, &last);
        nvx_emit(c, "V2.clkdoc", k->name, st, st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "cmd 0x%08x size %u stamp 0x%08x seed[%s]=0x%08x -> 0x%08x "
                 "(%s); %u non-zero words%s  [%s]",
                 k->cmd, k->size, k->w0,
                 k->seed_off == ~0u ? "-" : "off",
                 k->seed_val, st, nvx_status_name(st), nz,
                 nz ? "" : "", k->src);
        if (st != 0) continue;

        if (nz) {
            char fl[64];
            snprintf(fl, sizeof fl, ", first @%u, last @%u", first, last);
            nvx_emit(c, "V2.clkdoc", k->name, 0, NVX_V_SKIP,
                     "accepted: %u non-zero words of %u%s", nz, k->size / 4, fl);
        } else {
            nvx_emit(c, "V2.clkdoc", k->name, 0, NVX_V_SKIP,
                     "accepted but the whole %u B payload is zero (command "
                     "accepts this form and returns no data)", k->size);
        }

        unsigned dump = k->dump ? k->dump : 64;
        if (dump) doc_dump(c, k->name, buf, k->size, dump);

        if (k->cmd == NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL && k->size == 2404)
            doc_decode_clk_ctrl(c, k->name, buf, k->size);
        if (k->cmd == NVX_RM_CMD_VFP_GET_INFO_582 && k->size == 776)
            doc_decode_domain_table(c, k->name, buf, k->size);
        if (k->cmd == NVX_RM_CMD_CLK_FREQ_CTRL_INFO)
            doc_decode_freq_ctrl(c, k->name, buf, k->size);
        if (k->cmd == NVX_RM_CMD_RAILS_GET_CONTROL)
            doc_decode_rails_ctrl(c, k->name, buf, k->size);
    }

    /* Measurement is a two-sample affair by construction: the counter is a
     * read-modify-write cycle counter and the payload carries a timestamp.
     * Report the delta so a wrong decode is visible instead of implied. */
    for (unsigned di = 0; di < 3; ++di) {
        static const unsigned idx[3] = { 0, 2, 4 };
        NvU32 p0[8], p1[8];
        memset(p0, 0, sizeof p0);
        memset(p1, 0, sizeof p1);
        p0[0] = p1[0] = NVX_STAMP_CLK_MEASURE_V1;
        p0[1] = p1[1] = idx[di];
        NvU32 s0 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  NVX_RM_CMD_CLK_MEASURE_FREQ, p0, sizeof p0);
        usleep(120000);
        NvU32 s1 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  NVX_RM_CMD_CLK_MEASURE_FREQ, p1, sizeof p1);
        if (s0 || s1) continue;
        NvU64 t0 = (NvU64)p0[4] | ((NvU64)p0[5] << 32);
        NvU64 t1 = (NvU64)p1[4] | ((NvU64)p1[5] << 32);
        NvU32 c0 = p0[2], c1 = p1[2];
        nvx_emit(c, "V2.clkdoc", "measure-freq delta", 0, NVX_V_SKIP,
                 "domain idx %u: counter %u -> %u (d %d), ts %llu -> %llu ns "
                 "(dt %lld); implied %s", idx[di], c0, c1, (int)(c1 - c0),
                 (unsigned long long)t0, (unsigned long long)t1,
                 (long long)(t1 - t0),
                 t1 > t0 ? "freq = dcount/dt * 1e9 Hz (see below)" : "N/A");
    }

    /* Seed sweep on the three surfaces that ACK: is the ACK a fixed head, or
     * does an input mask make the handler fill its record array? Each variant
     * writes EXACTLY ONE dword of input and everything else stays zero, so a
     * change in the non-zero word count is attributable to that dword. */
    {
        static const struct {
            const char *name; NvU32 cmd; unsigned size; unsigned off; NvU32 val;
            const char *why;
        } seeds[] = {
            { "B201 2444 mask@0=all", NVX_RM_CMD_RAILS_GET_INFO, 2444, 0,
              0xFFFFFFFFu, "is the rail mask an input filter?" },
            { "B201 2444 byte15=ff", NVX_RM_CMD_RAILS_GET_INFO, 2444, 12,
              0xFF000000u, "Windows esc buf[15]=rail filter, same offset?" },
            { "B213 1036 mask@0=ff", NVX_RM_CMD_RAILS_GET_CONTROL, 1036, 0,
              0xFFu, "seed the output mask slot" },
            { "B213 1036 byte15=ff", NVX_RM_CMD_RAILS_GET_CONTROL, 1036, 12,
              0xFF000000u, "esc buf[15]=rail filter on the RM param too?" },
            { "B213 1036 rec0.type=2", NVX_RM_CMD_RAILS_GET_CONTROL, 1036, 12,
              0x00000002u, "dense entry type seed (doc: type@+72)" },
            { "776 mask@0=all", NVX_RM_CMD_VFP_GET_INFO_582, 776, 0,
              0xFFFFFFFFu, "is the 776 table mask-seeded (head[0])?" },
            { "776 mask@4=all", NVX_RM_CMD_VFP_GET_INFO_582, 776, 4,
              0xFFFFFFFFu, "is the 776 table mask-seeded (head[1])?" },
        };
        for (unsigned i = 0; i < sizeof seeds / sizeof seeds[0]; ++i) {
            const unsigned sz = seeds[i].size;
            memset(buf, 0, sz);
            memcpy(buf + seeds[i].off, &seeds[i].val, 4);
            NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                      seeds[i].cmd, buf, sz);
            unsigned first = 0, last = 0;
            unsigned nz = doc_census(buf, sz, &first, &last);
            nvx_emit(c, "V2.clkdoc", seeds[i].name, st,
                     st == 0 ? NVX_V_SKIP : NVX_V_REJECTED,
                     "cmd 0x%08x size %u [+%u]=0x%08x -> 0x%08x (%s); %u "
                     "non-zero words incl. that seed (first @%u last @%u) [%s]",
                     seeds[i].cmd, sz, seeds[i].off, seeds[i].val, st,
                     nvx_status_name(st), nz, first, last, seeds[i].why);
            if (st == 0)
                doc_dump(c, seeds[i].name, buf, sz, 24);
        }
    }

    /* The 776 B table is the one confirmed ACK. Read it twice a second apart:
     * if the values move on an idle desktop they are live clocks; if they hold
     * they are a per-domain capability/max table. Either answer is a reading
     * class we can name, which is what this pass is for. */
    {
        unsigned char a[776], b[776];
        memset(a, 0, sizeof a);
        memset(b, 0, sizeof b);
        NvU32 s0 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  NVX_RM_CMD_VFP_GET_INFO_582, a, sizeof a);
        sleep(1);
        NvU32 s1 = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  NVX_RM_CMD_VFP_GET_INFO_582, b, sizeof b);
        if (s0 == 0 && s1 == 0) {
            unsigned moved = 0;
            for (unsigned off = 0; off + 4 <= sizeof a; off += 4)
                if (memcmp(a + off, b + off, 4)) ++moved;
            nvx_emit(c, "V2.clkdoc", "776 stability", 0, NVX_V_SKIP,
                     "two reads 1 s apart: %u of %u dwords changed under an "
                     "idle desktop", moved, (unsigned)(sizeof a / 4));
            doc_decode_domain_table(c, "776 table (re-read)", b, sizeof b);
        }
    }

    free(buf);
}

/* ------------------------------------------------------------------ */
/* Object-carrier sweep (--clkobj-read)                                */
/* ------------------------------------------------------------------ */
/* Every documented case that still answers 0x1f was sent with
 * hObject = hSubdevice, which is where the two working faces live. The
 * remaining failing layer is therefore "command/category/object mismatch":
 * an RM control is dispatched on (class, method, object), and a control that
 * belongs to the DEVICE or to the CLIENT is rejected with INVALID_ARGUMENT
 * when addressed to the subdevice -- the same status a bad paramSize produces.
 * This sweep re-sends the SAME documented form against each carrier so the
 * two causes are separated instead of merged.
 *
 * Read-only: GET ids only, same SET blacklist as the other passes.
 */

static void item_clk_obj(nvx_ctx *c)
{
    static const struct {
        const char *name; NvU32 cmd; unsigned size; NvU32 stamp;
        unsigned seed_off; NvU32 seed_val;
    } cases[] = {
        { "domains-get-info v1", NVX_RM_CMD_CLK_DOMAINS_GET_INFO, 2520,
          NVX_STAMP_CLK_DOMAINS_INFO_V1, ~0u, 0 },
        { "domains-get-control v1", NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL, 2404,
          NVX_STAMP_CLK_DOMAINS_CONTROL_V1, 8, 0xFFu },
        { "measure-freq v1 dom0", NVX_RM_CMD_CLK_MEASURE_FREQ, 32,
          NVX_STAMP_CLK_MEASURE_V1, 4, 0 },
        { "vfp-get-info 493060", NVX_RM_CMD_VFP_GET_INFO_582, 493060,
          NVX_STAMP_VFP_INFO, ~0u, 0 },
        { "vfp-get-status 2000388", NVX_RM_CMD_VFP_GET_STATUS_582, 2000388,
          NVX_STAMP_VFP_STATUS, ~0u, 0 },
        { "vfp-get-control 0x474604", NVX_RM_CMD_VFP_GET_CONTROL, 4343300,
          NVX_STAMP_VFP_CONTROL, ~0u, 0 },
        { "freq-ctrl-info 3148", NVX_RM_CMD_CLK_FREQ_CTRL_INFO, 3148,
          NVX_STAMP_CLK_FREQ_CTRL_INFO, ~0u, 0 },
        { "freq-ctrl-status 2508", NVX_RM_CMD_CLK_FREQ_CTRL_STATUS, 2508,
          NVX_STAMP_CLK_FREQ_CTRL_STATUS, 4, 0x7u },
        { "rails-get-status 3212", NVX_RM_CMD_RAILS_GET_STATUS, 3212, 0, ~0u, 0 },
        /* Two known-good controls as carrier controls: whatever carrier flips
         * these is the wrong carrier, so the sweep is self-anchoring. */
        { "ctl vfp-get-info 776", NVX_RM_CMD_VFP_GET_INFO_582, 776, 0, ~0u, 0 },
        { "ctl rails-get-info 2444", NVX_RM_CMD_RAILS_GET_INFO, 2444, 0, ~0u, 0 },
        /* Size-boundary set for the rail GetInfo: is the accept an exact
         * struct size, a minimum, or a congruence? (2404 fails, 2444 and
         * 500008 pass.) */
        { "rails-get-info 88", NVX_RM_CMD_RAILS_GET_INFO, 88, 0, ~0u, 0 },
        { "rails-get-info 164", NVX_RM_CMD_RAILS_GET_INFO, 164, 0, ~0u, 0 },
        { "rails-get-info 620", NVX_RM_CMD_RAILS_GET_INFO, 620, 0, ~0u, 0 },
        { "rails-get-info 2440", NVX_RM_CMD_RAILS_GET_INFO, 2440, 0, ~0u, 0 },
        { "rails-get-info 2444", NVX_RM_CMD_RAILS_GET_INFO, 2444, 0, ~0u, 0 },
        { "rails-get-info 2448", NVX_RM_CMD_RAILS_GET_INFO, 2448, 0, ~0u, 0 },
        { "rails-get-info 2480", NVX_RM_CMD_RAILS_GET_INFO, 2480, 0, ~0u, 0 },
        { "rails-get-info 2500", NVX_RM_CMD_RAILS_GET_INFO, 2500, 0, ~0u, 0 },
    };
    struct { const char *tag; NvHandle h; } carriers[3];
    carriers[0].tag = "subdevice"; carriers[0].h = c->hSubdevice;
    carriers[1].tag = "device";    carriers[1].h = c->hDevice;
    carriers[2].tag = "client";    carriers[2].h = c->hClient;

    unsigned buflen = 0;
    for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; ++i)
        if (cases[i].size > buflen) buflen = cases[i].size;
    unsigned char *buf = malloc(buflen);
    if (!buf) {
        nvx_emit(c, "V2.clkobj", "clkobj-read", 0, NVX_V_HARD,
                 "alloc %u B failed", buflen);
        return;
    }

    for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; ++i) {
        NvU32 st[3];
        if (nvx_id_is_known_set(cases[i].cmd)) {
            nvx_emit(c, "V2.clkobj", cases[i].name, 0, NVX_V_HARD,
                     "REFUSED: 0x%08x is a documented SET id", cases[i].cmd);
            continue;
        }
        for (unsigned k = 0; k < 3; ++k) {
            memset(buf, 0, cases[i].size);
            if (cases[i].stamp) memcpy(buf, &cases[i].stamp, 4);
            if (cases[i].seed_off != ~0u)
                memcpy(buf + cases[i].seed_off, &cases[i].seed_val, 4);
            st[k] = nvx_rm_control(c->ctl_fd, c->hClient, carriers[k].h,
                                   cases[i].cmd, buf, cases[i].size);
        }
        nvx_emit(c, "V2.clkobj", cases[i].name, st[0],
                 st[0] == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "cmd 0x%08x size %u stamp 0x%08x -> subdevice 0x%08x (%s) | "
                 "device 0x%08x (%s) | client 0x%08x (%s)",
                 cases[i].cmd, cases[i].size, cases[i].stamp,
                 st[0], nvx_status_name(st[0]),
                 st[1], nvx_status_name(st[1]),
                 st[2], nvx_status_name(st[2]));
        if (st[0] == 0) {
            /* buf was zeroed again by the last (client) carrier attempt, so
             * re-read on the subdevice before judging the payload. */
            memset(buf, 0, cases[i].size);
            if (cases[i].stamp) memcpy(buf, &cases[i].stamp, 4);
            if (cases[i].seed_off != ~0u)
                memcpy(buf + cases[i].seed_off, &cases[i].seed_val, 4);
            NvU32 re = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                      cases[i].cmd, buf, cases[i].size);
            unsigned first = 0, last = 0;
            unsigned nz = doc_census(buf, cases[i].size, &first, &last);
            nvx_emit(c, "V2.clkobj", cases[i].name, re, NVX_V_SKIP,
                     "  subdevice re-read 0x%08x: %u non-zero words, first @%u "
                     "last @%u", re, nz, first, last);
        }
    }
    free(buf);
}

/* ------------------------------------------------------------------ */
/* Live clock tracker (--clk-track N)                                  */
/* ------------------------------------------------------------------ */
/* One line per second: the RUSD gpc/mem clocks (when the shared page is
 * mapped) next to the per-domain kHz table that RM 0x20809061 ACKs at
 * paramSize 776.
 *
 * The question this answers is the one payload dumps cannot: a table that
 * MOVES with the RUSD clocks under a real load is a live reading, while a
 * table that holds still while they move is a capability/max list. Without
 * that A/B the 776 B table stays "probe data", not a reading.
 *
 * Read-only: one RM GET per second, plus the existing RUSD page reads. */
int nvx_stage4_clk_track(nvx_ctx *c, int seconds)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V4.clk", "track", 0, NVX_V_SKIP, "RM chain not open");
        return -1;
    }
    unsigned char buf[776];
    for (int t = 0; t <= seconds; ++t) {
        memset(buf, 0, sizeof buf);
        NvU32 st = nvx_rm_control(c->ctl_fd, c->hClient, c->hSubdevice,
                                  NVX_RM_CMD_VFP_GET_INFO_582, buf, sizeof buf);
        /* Print each record's OWN bit field: the entries are ordered by bit,
         * but indexing them positionally silently mislabels every domain. */
        char dom[192];
        int off = 0;
        unsigned shown = 0;
        for (unsigned i = 0; i < (sizeof buf - 8) / 24; ++i) {
            unsigned ro = 8 + 24 * i;
            NvU32 bit = 0, v = 0;
            if (st != 0 || ro + 24 > sizeof buf) break;
            memcpy(&bit, buf + ro + 4, 4);
            memcpy(&v, buf + ro + 8, 4);
            if (!bit && !v) continue;
            ++shown;
            off += snprintf(dom + off, sizeof dom - (size_t)off, "%s0x%x=%u",
                            shown > 1 ? " " : "", bit, v);
            if (off <= 0 || off >= (int)sizeof dom - 1) break;
        }
        if (!shown) snprintf(dom, sizeof dom, "no records");
        NvU32 gpc = 0, dram = 0;
        int have_rusd = 0;
        if (c->rusd) {
            NV00DE_SHARED_DATA *sd = (NV00DE_SHARED_DATA *)c->rusd;
            RUSD_CLK_PUBLIC_DOMAIN_INFOS clocks;
            if (rusd_copy(&sd->clkPublicDomainInfos.lastModifiedTimestamp,
                          &sd->clkPublicDomainInfos, &clocks, sizeof clocks) == 0) {
                gpc = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS].actualClkKHz;
                dram = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_MEMORY].actualClkKHz;
                have_rusd = 1;
            }
        }
        char rusd_txt[64];
        if (have_rusd)
            snprintf(rusd_txt, sizeof rusd_txt, "gpc=%u mem=%u kHz", gpc, dram);
        else
            snprintf(rusd_txt, sizeof rusd_txt, "rusd n/a");
        nvx_emit(c, "V4.clk", "track", st, st == 0 ? NVX_V_OK : NVX_V_REJECTED,
                 "t=+%ds rm776[%s] | %s", t, dom, rusd_txt);
        if (t < seconds) sleep(1);
    }
    return 0;
}

int nvx_stage2_clk_obj(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkobj", "clkobj-read", 0, NVX_V_SKIP,
                 "RM chain not open");
        return -1;
    }
    item_clk_obj(c);
    return 0;
}

int nvx_stage2_clk_doc(nvx_ctx *c)
{
    if (!c->hSubdevice) {
        nvx_emit(c, "V2.clkdoc", "clkdoc-read", 0, NVX_V_SKIP,
                 "RM chain not open");
        return -1;
    }
    item_clk_doc(c);
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
