/* Thin read-only ABI over NVOC's RM transport. NVIDIA layouts come from OGKM,
 * not handwritten Rust copies. No probe CLI or performance SET calls.
 *
 * Two entry points exist:
 *   - nvoc_macos_rm_snapshot(): one-shot open/read/close, used by get-info and
 *     get-status. Unchanged behaviour.
 *   - nvoc_macos_rm_session_*(): a persistent session that opens the RM object
 *     chain and maps the RUSD page once, then serves repeated reads. Used by
 *     the continuous CLI monitor so sampling does not rebuild the chain.
 * Every entry point must be serialized by the Rust caller: nvx_xnu owns a
 * process-global fd table. */
#include "nvx_xnu.h"
#include "nvx_rm.h"
#include "nvx_defs.h"
#include "nv-ioctl.h"
#include "nv_escape.h"
#include "class/cl0080.h"
#include "class/cl2080.h"
#include "ctrl/ctrl0000/ctrl0000gpu.h"
#include "ctrl/ctrl0080/ctrl0080gpu.h"
#include "ctrl/ctrl2080/ctrl2080gpu.h"
#include "ctrl/ctrl2080/ctrl2080fb.h"
#include "ctrl/ctrl2080/ctrl2080bus.h"
#include "ctrl/ctrl2080/ctrl2080bios.h"
#include "ctrl/ctrl2080/ctrl2080perf.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "rusd_read.h"

enum { RUSD_CLOCK, RUSD_TEMP, RUSD_POWER, RUSD_LIMIT, RUSD_FAN, RUSD_UTIL, RUSD_FIELDS };
typedef struct {
    uint32_t status, valid, graphics_khz, memory_khz;
    int32_t temperature_fx256;
    uint32_t power_mw, limit_mw, fan_rpm, utilization_percent, reserved;
    uint64_t timestamps[RUSD_FIELDS];
} nvoc_rm_telemetry;

typedef struct {
    uint32_t abi_version, rm_gpu_id, domain, bus, slot, function;
    uint32_t vendor_id, device_id, subsystem_id, revision, board_id, class_count;
    uint64_t total_bytes, usable_bytes, bar1_bytes, l2_bytes;
    uint32_t pstate, memory_status, pstate_status, pci_status, bios_status, name_status, class_status;
    char driver[64], name[64], sku[16], project[16];
    nvoc_rm_telemetry telemetry;
} nvoc_rm_snapshot;

_Static_assert(sizeof(nvoc_rm_snapshot) == 360, "Rust/C snapshot ABI mismatch");

/* One coherent read of the mapped page. field_status[i] == 0 means the value
 * was copied under a stable, data-valid RUSD sequence; nonzero means that
 * section could not be read coherently this time and the value is unset. */
typedef struct {
    uint32_t abi_version;
    uint32_t field_status[RUSD_FIELDS];
    uint32_t graphics_khz, memory_khz;
    int32_t temperature_fx256;
    uint32_t power_mw, limit_mw, fan_rpm, utilization_percent;
    uint32_t pstate, pstate_status;
    uint64_t timestamps[RUSD_FIELDS];
} nvoc_rm_sample;

_Static_assert(sizeof(nvoc_rm_sample) == 112, "Rust/C sample ABI mismatch");

/* A persistent read-only session: one connection chain plus one mapped RUSD
 * page, reused for every sample. `abandon_chain` defers RM object reclamation
 * to connection teardown when an explicit unmap fails. */
typedef struct nvoc_rm_session {
    int ctl, dev;
    NvHandle client, device, subdevice;
    NvHandle shared;
    NV00DE_SHARED_DATA *data;
    NvU64 length;
    int abandon_chain;
} nvoc_rm_session;

static uint32_t failure(char *error, size_t cap, const char *operation, uint32_t status)
{
    snprintf(error, cap, "%s: 0x%08x (%s)", operation, status, nvx_status_name(status));
    return status ? status : UINT32_MAX;
}

/* Build the full private client -> device -> subdevice chain and run the
 * one-time identity/information queries. Leaves the session partially open on
 * failure so the caller can always call session_release(). */
static uint32_t chain_open(nvoc_rm_session *s, nvoc_rm_snapshot *out, char *error, size_t cap)
{
    s->ctl = -1; s->dev = -1;
    s->client = s->device = s->subdevice = 0;
    s->shared = 0; s->data = NULL; s->length = 0; s->abandon_chain = 0;
    out->abi_version = 2;
    uint32_t st = UINT32_MAX;
    const char *operation = "open NullMoth IOKit control service (driver loaded?)";
    s->ctl = nvx_open(1);
    if (s->ctl < 0) goto done;
    operation = "CARD_INFO";
    nv_ioctl_card_info_t card = {0};
    if (nvx_esc(s->ctl, NV_ESC_CARD_INFO, &card, sizeof card) || !card.valid) goto done;
    out->rm_gpu_id = card.gpu_id;
    out->domain = card.pci_info.domain; out->bus = card.pci_info.bus;
    out->slot = card.pci_info.slot; out->function = card.pci_info.function;
    out->vendor_id = card.pci_info.vendor_id; out->device_id = card.pci_info.device_id;
    operation = "RM version handshake";
    nv_ioctl_rm_api_version_t version = {0};
    version.cmd = NV_RM_API_VERSION_CMD_QUERY;
    snprintf(version.versionString, sizeof version.versionString, "610.57.04");
    if (nvx_esc(s->ctl, NV_ESC_CHECK_VERSION_STR, &version, sizeof version) ||
        version.reply != NV_RM_API_VERSION_REPLY_RECOGNIZED ||
        strncmp(version.versionString, "610.57.04", sizeof version.versionString)) goto done;
    snprintf(out->driver, sizeof out->driver, "%s", version.versionString);
    operation = "open NullMoth IOKit device service";
    s->dev = nvx_open(0);
    if (s->dev < 0) goto done;
    operation = "REGISTER_FD";
    nv_ioctl_register_fd_t rf = {0}; rf.ctl_fd = s->ctl;
    if (nvx_esc(s->dev, NV_ESC_REGISTER_FD, &rf, sizeof rf)) goto done;
    operation = "allocate root client";
    s->client = nvx_rm_alloc(s->ctl, 0, 0, 0xcafe0000, NVX_CLASS_ROOT_CLIENT, NULL, 0, &st);
    if (!s->client) goto done;
    operation = "attach GPU";
    NV0000_CTRL_GPU_ATTACH_IDS_PARAMS attach = {0};
    attach.gpuIds[0] = card.gpu_id; attach.gpuIds[1] = NV0000_CTRL_GPU_INVALID_ID;
    st = nvx_rm_control(s->ctl, s->client, s->client, NV0000_CTRL_CMD_GPU_ATTACH_IDS, &attach, sizeof attach);
    if (st) goto done;
    operation = "GPU device instance";
    NV0000_CTRL_GPU_GET_ID_INFO_V2_PARAMS id = {0}; id.gpuId = card.gpu_id;
    st = nvx_rm_control(s->ctl, s->client, s->client, NV0000_CTRL_CMD_GPU_GET_ID_INFO_V2, &id, sizeof id);
    if (st) goto done;
    operation = "allocate device";
    NV0080_ALLOC_PARAMETERS dp = {0}; dp.deviceId = id.deviceInstance; dp.hClientShare = s->client;
    s->device = nvx_rm_alloc(s->ctl, s->client, s->client, 0xcafe0001, NV01_DEVICE_0, &dp, sizeof dp, &st);
    if (!s->device) goto done;
    operation = "allocate subdevice";
    NV2080_ALLOC_PARAMETERS sp = {0};
    s->subdevice = nvx_rm_alloc(s->ctl, s->client, s->device, 0xcafe0002, NV20_SUBDEVICE_0, &sp, sizeof sp, &st);
    if (!s->subdevice) goto done;
#define QUERY(cmd, p) nvx_rm_control(s->ctl, s->client, s->subdevice, cmd, &(p), sizeof(p))
    NV2080_CTRL_GPU_GET_NAME_STRING_PARAMS name = {0};
    out->name_status = QUERY(NV2080_CTRL_CMD_GPU_GET_NAME_STRING, name);
    if (!out->name_status) {
        memcpy(out->name, name.gpuNameString.ascii, sizeof out->name - 1);
    }
    NV2080_CTRL_BUS_GET_PCI_INFO_PARAMS pci = {0};
    out->pci_status = QUERY(NV2080_CTRL_CMD_BUS_GET_PCI_INFO, pci);
    if (!out->pci_status) { out->subsystem_id = pci.pciSubSystemId; out->revision = pci.pciRevisionId; }
    NV2080_CTRL_BIOS_GET_SKU_INFO_PARAMS bios = {0};
    out->bios_status = QUERY(NV2080_CTRL_CMD_BIOS_GET_SKU_INFO, bios);
    if (!out->bios_status) {
        out->board_id = bios.BoardID;
        memcpy(out->sku, bios.chipSKU, sizeof bios.chipSKU);
        memcpy(out->project, bios.project, sizeof bios.project);
    }
    const NvU32 wanted[] = { NV2080_CTRL_FB_INFO_INDEX_TOTAL_RAM_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_USABLE_RAM_SIZE, NV2080_CTRL_FB_INFO_INDEX_BAR1_SIZE,
        NV2080_CTRL_FB_INFO_INDEX_L2CACHE_SIZE };
    NV2080_CTRL_FB_GET_INFO_V2_PARAMS fb = {0}; fb.fbInfoListSize = 4;
    for (unsigned i = 0; i < 4; i++) fb.fbInfoList[i].index = wanted[i];
    out->memory_status = QUERY(NV2080_CTRL_CMD_FB_GET_INFO_V2, fb);
    if (!out->memory_status) {
        out->total_bytes = (uint64_t)fb.fbInfoList[0].data * 1024;
        out->usable_bytes = (uint64_t)fb.fbInfoList[1].data * 1024;
        out->bar1_bytes = (uint64_t)fb.fbInfoList[2].data * 1024;
        out->l2_bytes = fb.fbInfoList[3].data;
    }
    NV2080_CTRL_PERF_GET_CURRENT_PSTATE_PARAMS ps = {0};
    out->pstate_status = QUERY(NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE, ps);
    if (!out->pstate_status) out->pstate = ps.currPstate;
    NV0080_CTRL_GPU_GET_CLASSLIST_V2_PARAMS classes = {0};
    out->class_status = nvx_rm_control(s->ctl, s->client, s->device,
        NV0080_CTRL_CMD_GPU_GET_CLASSLIST_V2, &classes, sizeof classes);
    if (!out->class_status) out->class_count = classes.numClasses;
#undef QUERY
    st = 0;
done:
    return st ? failure(error, cap, operation, st) : 0;
}

/* Release whatever part of the chain exists. Honours abandon_chain, which
 * defers RM reclamation to connection teardown after a failed unmap. */
static uint32_t session_release(nvoc_rm_session *s, char *error, size_t cap)
{
    uint32_t result = 0;
    if (s->data) {
        if (nvx_unmap_memory_fd(s->ctl, s->data) != 0) {
            s->abandon_chain = 1;
            result = failure(error, cap, "RUSD unmap", UINT32_MAX);
        }
        s->data = NULL;
    }
    if (s->shared) nvx_rm_free(s->ctl, s->client, s->subdevice, s->shared);
    s->shared = 0;
    if (!s->abandon_chain) nvx_rm_chain_close(s->ctl, s->client, s->device, s->subdevice);
    if (s->dev >= 0) nvx_close(s->dev);
    if (s->ctl >= 0) nvx_close(s->ctl);
    s->dev = -1; s->ctl = -1;
    return result;
}

/* Allocate and read-only map the shared telemetry page. System memory requires
 * the control fd both for the RM mapping context and the IOKit mapping. */
static int telemetry_map(nvoc_rm_session *s, uint32_t *status)
{
    NV00DE_ALLOC_PARAMETERS ap = {0};
    ap.polledDataMask = NV00DE_RUSD_POLL_CLOCK | NV00DE_RUSD_POLL_PERF |
        NV00DE_RUSD_POLL_POWER | NV00DE_RUSD_POLL_THERMAL | NV00DE_RUSD_POLL_FAN;
    NvU32 st = 0;
    s->shared = nvx_rm_alloc(s->ctl, s->client, s->subdevice, 0xcafe0003,
        RM_USER_SHARED_DATA, &ap, sizeof ap, &st);
    if (!s->shared) { *status = st ? st : UINT32_MAX; return 1; }
    NvU64 length = 0;
    s->data = nvx_map_memory_readonly(s->ctl, s->ctl, s->client, s->subdevice,
        s->shared, sizeof(*s->data), &length, &st);
    if (!s->data) {
        *status = st ? st : UINT32_MAX;
        nvx_rm_free(s->ctl, s->client, s->subdevice, s->shared);
        s->shared = 0;
        return 1;
    }
    if (length < sizeof(*s->data)) {
        /* Too short to trust: report it and let the caller unmap/free. */
        s->length = length;
        *status = UINT32_MAX;
        return 1;
    }
    s->length = length;
    return 0;
}

static void telemetry_unmap_and_free(nvoc_rm_session *s, nvoc_rm_telemetry *out)
{
    if (s->data) {
        if (nvx_unmap_memory_fd(s->ctl, s->data) != 0) {
            /* Drop the telemetry and keep the RM backing alive until the
             * connection closes; the mapping must not be reused. */
            s->abandon_chain = 1;
            s->data = NULL;
            if (out) { out->valid = 0; out->status = UINT32_MAX; }
            return;
        }
        s->data = NULL;
    }
    if (s->shared) nvx_rm_free(s->ctl, s->client, s->subdevice, s->shared);
    s->shared = 0;
}

/* One coherent, non-advancing read. Used by the continuous monitor, which
 * decides field freshness/updates in Rust from the returned timestamps. */
static uint32_t sample_read(nvoc_rm_session *s, nvoc_rm_sample *out)
{
    RUSD_CLK_PUBLIC_DOMAIN_INFOS clocks;
    RUSD_TEMPERATURE temp;
    RUSD_INST_POWER_USAGE power;
    RUSD_POWER_LIMITS limit;
    RUSD_FAN_COOLER_STATUS fan;
    RUSD_PERF_DEVICE_UTILIZATION util;
    NvU64 ts;
    memset(out, 0, sizeof(*out));
    out->abi_version = 2;
#define COPY(field, dest, index) \
    nvoc_rusd_copy(&(field).lastModifiedTimestamp, &(field), &(dest), sizeof(dest), &ts)
    if (COPY(s->data->clkPublicDomainInfos, clocks, RUSD_CLOCK)) {
        out->graphics_khz = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS].actualClkKHz;
        out->memory_khz = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_MEMORY].actualClkKHz;
        out->timestamps[RUSD_CLOCK] = ts;
    } else {
        out->field_status[RUSD_CLOCK] = 1;
    }
    if (COPY(s->data->temperatures[RUSD_TEMPERATURE_TYPE_GPU], temp, RUSD_TEMP)) {
        out->temperature_fx256 = temp.temperature;
        out->timestamps[RUSD_TEMP] = ts;
    } else {
        out->field_status[RUSD_TEMP] = 1;
    }
    if (COPY(s->data->instPowerUsage, power, RUSD_POWER)) {
        out->power_mw = power.info.instGpuPower;
        out->timestamps[RUSD_POWER] = ts;
    } else {
        out->field_status[RUSD_POWER] = 1;
    }
    if (COPY(s->data->powerLimitGpu, limit, RUSD_LIMIT)) {
        out->limit_mw = limit.info.enforcedmW;
        out->timestamps[RUSD_LIMIT] = ts;
    } else {
        out->field_status[RUSD_LIMIT] = 1;
    }
    if (COPY(s->data->fanCoolerStatus, fan, RUSD_FAN)) {
        out->fan_rpm = 0;
        for (unsigned i = 0; i < RUSD_FAN_COOLER_MAX_COOLERS; i++)
            if (fan.info.rpmCurr[i] > out->fan_rpm) out->fan_rpm = fan.info.rpmCurr[i];
        out->timestamps[RUSD_FAN] = ts;
    } else {
        out->field_status[RUSD_FAN] = 1;
    }
    if (COPY(s->data->perfDevUtil, util, RUSD_UTIL)) {
        out->utilization_percent = util.info.gpuPercentBusy;
        out->timestamps[RUSD_UTIL] = ts;
    } else {
        out->field_status[RUSD_UTIL] = 1;
    }
#undef COPY
    /* P-state is a live RM control, not a RUSD section: re-read it each sample
     * so a long run never reports the value captured when the chain opened. */
    NV2080_CTRL_PERF_GET_CURRENT_PSTATE_PARAMS ps = {0};
    out->pstate_status = nvx_rm_control(s->ctl, s->client, s->subdevice,
        NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE, &ps, sizeof ps);
    if (!out->pstate_status) out->pstate = ps.currPstate;
    return 0;
}

/* Repeatedly read until every polled field advances past the baselines taken
 * just after mapping, or the 1.5 s budget expires. Used once per session (and
 * by the single-shot path) so the first exposed sample is known-fresh. */
static void sample_wait_fresh(nvoc_rm_session *s, nvoc_rm_telemetry *out)
{
    const NvU64 baseline[RUSD_FIELDS] = {
        s->data->clkPublicDomainInfos.lastModifiedTimestamp,
        s->data->temperatures[RUSD_TEMPERATURE_TYPE_GPU].lastModifiedTimestamp,
        s->data->instPowerUsage.lastModifiedTimestamp,
        s->data->powerLimitGpu.lastModifiedTimestamp,
        s->data->fanCoolerStatus.lastModifiedTimestamp,
        s->data->perfDevUtil.lastModifiedTimestamp,
    };
    for (unsigned attempt = 0; attempt < 16; attempt++) {
        RUSD_CLK_PUBLIC_DOMAIN_INFOS clocks;
        RUSD_TEMPERATURE temp;
        RUSD_INST_POWER_USAGE power;
        RUSD_POWER_LIMITS limit;
        RUSD_FAN_COOLER_STATUS fan;
        RUSD_PERF_DEVICE_UTILIZATION util;
/* Reset validity on each attempt; never keep a value whose current sequence
 * became invalid or whose data could no longer be read. */
#define READ(field, dest, index) nvoc_rusd_read(&(field).lastModifiedTimestamp, &(field), \
    &(dest), sizeof(dest), baseline[index], &out->timestamps[index])
        out->valid = 0;
        if (READ(s->data->clkPublicDomainInfos, clocks, RUSD_CLOCK)) {
            out->graphics_khz = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS].actualClkKHz;
            out->memory_khz = clocks.info[RUSD_CLK_PUBLIC_DOMAIN_MEMORY].actualClkKHz;
            out->valid |= 1u << RUSD_CLOCK;
        }
        if (READ(s->data->temperatures[RUSD_TEMPERATURE_TYPE_GPU], temp, RUSD_TEMP)) {
            out->temperature_fx256 = temp.temperature;
            out->valid |= 1u << RUSD_TEMP;
        }
        if (READ(s->data->instPowerUsage, power, RUSD_POWER)) {
            out->power_mw = power.info.instGpuPower;
            out->valid |= 1u << RUSD_POWER;
        }
        if (READ(s->data->powerLimitGpu, limit, RUSD_LIMIT)) {
            out->limit_mw = limit.info.enforcedmW;
            out->valid |= 1u << RUSD_LIMIT;
        }
        if (READ(s->data->fanCoolerStatus, fan, RUSD_FAN)) {
            out->fan_rpm = 0;
            for (unsigned i = 0; i < RUSD_FAN_COOLER_MAX_COOLERS; i++)
                if (fan.info.rpmCurr[i] > out->fan_rpm) out->fan_rpm = fan.info.rpmCurr[i];
            out->valid |= 1u << RUSD_FAN;
        }
        if (READ(s->data->perfDevUtil, util, RUSD_UTIL)) {
            out->utilization_percent = util.info.gpuPercentBusy;
            out->valid |= 1u << RUSD_UTIL;
        }
#undef READ
        if (out->valid == (1u << RUSD_FIELDS) - 1) break;
        if (attempt < 15) {
            struct timespec delay = { .tv_nsec = 100000000 };
            nanosleep(&delay, NULL);
        }
    }
}

/* Must be serialized by the Rust caller: nvx_xnu owns a process-global fd
 * table. NullMoth's CARD_INFO currently describes the first matching service. */
uint32_t nvoc_macos_rm_snapshot(nvoc_rm_snapshot *out, size_t size, uint32_t telemetry,
                               char *error, size_t cap)
{
    if (size != sizeof(*out)) return failure(error, cap, "snapshot ABI size", UINT32_MAX);
    memset(out, 0, sizeof(*out));
    nvoc_rm_session s;
    uint32_t result = chain_open(&s, out, error, cap);
    if (!result && telemetry) {
        uint32_t status = 0;
        memset(&out->telemetry, 0, sizeof out->telemetry);
        if (telemetry_map(&s, &status)) {
            out->telemetry.status = status;
        } else {
            sample_wait_fresh(&s, &out->telemetry);
            telemetry_unmap_and_free(&s, &out->telemetry);
        }
    }
    /* A failed unmap is already reflected in out->telemetry (all fields
     * unavailable); the one-shot query itself still completed. */
    session_release(&s, error, cap);
    return result;
}

/* ---- persistent session ABI (continuous monitoring) ---- */

uint32_t nvoc_macos_rm_session_open(nvoc_rm_session **out_session, nvoc_rm_snapshot *info,
                                   size_t info_size, char *error, size_t cap)
{
    if (!out_session || info_size != sizeof(*info))
        return failure(error, cap, "session info ABI size", UINT32_MAX);
    *out_session = NULL;
    memset(info, 0, sizeof(*info));
    nvoc_rm_session *s = calloc(1, sizeof(*s));
    if (!s) return failure(error, cap, "session allocation", UINT32_MAX);
    uint32_t result = chain_open(s, info, error, cap);
    if (!result) {
        uint32_t status = 0;
        /* No wait here: the first sample() reports advancement relative to the
         * mapping-time baseline, so the Rust caller owns the wait policy. */
        if (telemetry_map(s, &status)) result = failure(error, cap, "RUSD allocate/map", status);
    }
    if (result) {
        session_release(s, error, cap);
        free(s);
        return result;
    }
    *out_session = s;
    return 0;
}

uint32_t nvoc_macos_rm_session_sample(nvoc_rm_session *s, nvoc_rm_sample *out, size_t size,
                                     char *error, size_t cap)
{
    if (!s) return failure(error, cap, "session sample without session", UINT32_MAX);
    if (size != sizeof(*out)) return failure(error, cap, "sample ABI size", UINT32_MAX);
    if (!s->data || s->length < sizeof(*s->data))
        return failure(error, cap, "RUSD page not mapped", UINT32_MAX);
    return sample_read(s, out);
}

uint32_t nvoc_macos_rm_session_close(nvoc_rm_session *s, char *error, size_t cap)
{
    if (!s) return 0;
    uint32_t result = session_release(s, error, cap);
    free(s);
    return result;
}
