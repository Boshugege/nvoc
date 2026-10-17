/* Thin read-only ABI over NVOC's RM transport. NVIDIA layouts come from OGKM,
 * not handwritten Rust copies. No probe CLI or performance SET calls.
 *
 * Two entry points exist:
 *   - nvoc_macos_rm_snapshot(): one-shot open/read/close, used by get-info and
 *     get-status.
 *   - nvoc_macos_rm_session_*(): a persistent session that opens the RM object
 *     chain and maps the RUSD page once, then serves repeated reads. Used by
 *     the continuous CLI monitor so sampling does not rebuild the chain.
 * Every entry point must be serialized by the Rust caller: nvx_xnu owns a
 * process-global fd table.
 *
 * ABI v3 reads every RUSD section the 610.57.04 driver populates on this
 * platform through one section table (macos_rm_abi.h) plus the two live RM
 * controls that have no RUSD equivalent (current P-state, FB heap usage). Raw
 * driver units are transported unchanged; plausibility, decoding and naming
 * are the Rust layer's job, so both languages agree on one set of rules. */
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
#include "macos_rm_abi.h"

/* The layouts, the field indices and the entry-point signatures live in
 * macos_rm_abi.h: including it here makes a definition that drifts from the
 * declared prototype a compile error, and pins the sizes/offsets Rust asserts. */
/* SEC_ prefix: cl00de.h already owns type names such as RUSD_PROC_UTIL. */
enum {
    SEC_CLOCK = NVOC_RM_RUSD_CLOCK,
    SEC_TEMP_GPU = NVOC_RM_RUSD_TEMP_GPU,
    SEC_TEMP_MEMORY = NVOC_RM_RUSD_TEMP_MEMORY,
    SEC_TEMP_BOARD = NVOC_RM_RUSD_TEMP_BOARD,
    SEC_TEMP_POWER_SUPPLY = NVOC_RM_RUSD_TEMP_POWER_SUPPLY,
    SEC_POWER = NVOC_RM_RUSD_POWER,
    SEC_POWER_LIMIT = NVOC_RM_RUSD_POWER_LIMIT,
    SEC_AVG_POWER = NVOC_RM_RUSD_AVG_POWER,
    SEC_POWER_POLICY = NVOC_RM_RUSD_POWER_POLICY,
    SEC_THROTTLE = NVOC_RM_RUSD_THROTTLE,
    SEC_FAN = NVOC_RM_RUSD_FAN,
    SEC_UTIL = NVOC_RM_RUSD_UTIL,
    SEC_PCI = NVOC_RM_RUSD_PCI,
    SEC_PROC_UTIL = NVOC_RM_RUSD_PROC_UTIL,
    SEC_FIELDS = NVOC_RM_RUSD_FIELDS
};

/* The temperature sensors carried by the sample, in sample index order. */
static const unsigned temp_section[NVOC_RM_TEMP_SENSORS] = {
    SEC_TEMP_GPU, SEC_TEMP_MEMORY, SEC_TEMP_BOARD, SEC_TEMP_POWER_SUPPLY
};
static const unsigned temp_type[NVOC_RM_TEMP_SENSORS] = {
    RUSD_TEMPERATURE_TYPE_GPU, RUSD_TEMPERATURE_TYPE_MEMORY,
    RUSD_TEMPERATURE_TYPE_BOARD, RUSD_TEMPERATURE_TYPE_POWER_SUPPLY
};

/* A persistent read-only session: one connection chain plus one mapped RUSD
 * page, reused for every sample. `abandon_chain` defers RM object reclamation
 * to connection teardown when an explicit unmap fails. */
struct nvoc_rm_session {
    int ctl, dev;
    NvHandle client, device, subdevice;
    NvHandle shared;
    NV00DE_SHARED_DATA *data;
    NvU64 length;
    int abandon_chain;
};

static uint32_t failure(char *error, size_t cap, const char *operation, uint32_t status)
{
    if (error && cap)
        snprintf(error, cap, "%s: 0x%08x (%s)", operation, status, nvx_status_name(status));
    return status ? status : UINT32_MAX;
}

/* Mapping-time watermark of one polled section, by section index. 0 means the
 * driver has no valid timestamp for it right now. */
static NvU64 section_timestamp(const NV00DE_SHARED_DATA *d, unsigned index)
{
    switch (index) {
    case SEC_CLOCK: return d->clkPublicDomainInfos.lastModifiedTimestamp;
    case SEC_TEMP_GPU: return d->temperatures[RUSD_TEMPERATURE_TYPE_GPU].lastModifiedTimestamp;
    case SEC_TEMP_MEMORY: return d->temperatures[RUSD_TEMPERATURE_TYPE_MEMORY].lastModifiedTimestamp;
    case SEC_TEMP_BOARD: return d->temperatures[RUSD_TEMPERATURE_TYPE_BOARD].lastModifiedTimestamp;
    case SEC_TEMP_POWER_SUPPLY: return d->temperatures[RUSD_TEMPERATURE_TYPE_POWER_SUPPLY].lastModifiedTimestamp;
    case SEC_POWER: return d->instPowerUsage.lastModifiedTimestamp;
    case SEC_POWER_LIMIT: return d->powerLimitGpu.lastModifiedTimestamp;
    case SEC_AVG_POWER: return d->avgPowerUsage.lastModifiedTimestamp;
    case SEC_POWER_POLICY: return d->powerPolicyStatus.lastModifiedTimestamp;
    case SEC_THROTTLE: return d->clkThrottleInfo.lastModifiedTimestamp;
    case SEC_FAN: return d->fanCoolerStatus.lastModifiedTimestamp;
    case SEC_UTIL: return d->perfDevUtil.lastModifiedTimestamp;
    case SEC_PCI: return d->pciBusData.lastModifiedTimestamp;
    case SEC_PROC_UTIL: return d->procUtil.lastModifiedTimestamp;
    default: return 0;
    }
}

/* Why a polled section produced no value this round. */
static uint32_t section_state(const volatile NvU64 *timestamp)
{
    NvU64 raw = *timestamp;
    if (raw == RUSD_TIMESTAMP_INVALID) return NVOC_RM_FIELD_UNAVAILABLE;
    if (!RUSD_SEQ_DATA_VALID(raw) || raw == RUSD_TIMESTAMP_WRITE_IN_PROGRESS)
        return NVOC_RM_FIELD_UNSTABLE;
    return NVOC_RM_FIELD_OK;
}

/* A copy that failed although the section looked valid raced with the poller. */
static uint32_t section_failure_status(const volatile NvU64 *timestamp)
{
    uint32_t state = section_state(timestamp);
    return state ? state : NVOC_RM_FIELD_UNSTABLE;
}

/* Live RM controls that have no RUSD equivalent: the current P-state (re-read
 * every sample so a long run never reports the value captured at open time)
 * and the video-memory heap usage (FB_GET_INFO_V2 heap size - heap free). Both
 * report their own status; a failure withdraws only that measurement. */
static void read_live_controls(nvoc_rm_session *s, nvoc_rm_sample *out)
{
    NV2080_CTRL_PERF_GET_CURRENT_PSTATE_PARAMS ps = {0};
    out->pstate_status = nvx_rm_control(s->ctl, s->client, s->subdevice,
        NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE, &ps, sizeof ps);
    if (!out->pstate_status) out->pstate = ps.currPstate;

    const NvU32 wanted[2] = {
        NV2080_CTRL_FB_INFO_INDEX_HEAP_SIZE, NV2080_CTRL_FB_INFO_INDEX_HEAP_FREE
    };
    NV2080_CTRL_FB_GET_INFO_V2_PARAMS fb = {0};
    fb.fbInfoListSize = 2;
    for (unsigned i = 0; i < 2; i++) fb.fbInfoList[i].index = wanted[i];
    out->memory_used_status = nvx_rm_control(s->ctl, s->client, s->subdevice,
        NV2080_CTRL_CMD_FB_GET_INFO_V2, &fb, sizeof fb);
    if (!out->memory_used_status) {
        NvU32 heap_kb = fb.fbInfoList[0].data, free_kb = fb.fbInfoList[1].data;
        out->memory_used_kb = heap_kb > free_kb ? heap_kb - free_kb : 0;
    }
}

/* Read every carried RUSD section into the sample, recording either the
 * coherent value or the per-section failure status. Never advances state. */
static void sample_read_sections(nvoc_rm_session *s, nvoc_rm_sample *out)
{
    NvU64 ts;
    /* COPY() takes a stable copy of one section and reports its timestamp; on
     * failure the caller records why the section produced nothing. */
#define COPY(section, dest, index) \
    nvoc_rusd_copy(&(section).lastModifiedTimestamp, &(section), &(dest), sizeof(dest), &ts)

    RUSD_CLK_PUBLIC_DOMAIN_INFOS clocks;
    if (COPY(s->data->clkPublicDomainInfos, clocks, SEC_CLOCK)) {
        for (unsigned i = 0; i < NVOC_RM_CLK_DOMAINS; i++) {
            out->clk_target_mhz[i] = clocks.info[i].targetClkMHz;
            out->clk_actual_khz[i] = clocks.info[i].actualClkKHz;
        }
        out->timestamps[SEC_CLOCK] = ts;
    } else {
        out->field_status[SEC_CLOCK] =
            section_failure_status(&s->data->clkPublicDomainInfos.lastModifiedTimestamp);
    }

    for (unsigned i = 0; i < NVOC_RM_TEMP_SENSORS; i++) {
        RUSD_TEMPERATURE temp;
        if (COPY(s->data->temperatures[temp_type[i]], temp, temp_section[i])) {
            out->temperature_fx256[i] = temp.temperature;
            out->timestamps[temp_section[i]] = ts;
        } else {
            out->field_status[temp_section[i]] =
                section_failure_status(&s->data->temperatures[temp_type[i]].lastModifiedTimestamp);
        }
    }

    RUSD_INST_POWER_USAGE power;
    if (COPY(s->data->instPowerUsage, power, SEC_POWER)) {
        out->power_mw = power.info.instGpuPower;
        out->timestamps[SEC_POWER] = ts;
    } else {
        out->field_status[SEC_POWER] = section_failure_status(&s->data->instPowerUsage.lastModifiedTimestamp);
    }

    RUSD_POWER_LIMITS limit;
    if (COPY(s->data->powerLimitGpu, limit, SEC_POWER_LIMIT)) {
        out->power_limit_requested_mw = limit.info.requestedmW;
        out->power_limit_mw = limit.info.enforcedmW;
        out->timestamps[SEC_POWER_LIMIT] = ts;
    } else {
        out->field_status[SEC_POWER_LIMIT] = section_failure_status(&s->data->powerLimitGpu.lastModifiedTimestamp);
    }

    RUSD_AVG_POWER_USAGE avg;
    if (COPY(s->data->avgPowerUsage, avg, SEC_AVG_POWER)) {
        out->average_power_mw = avg.info.averageGpuPower;
        out->timestamps[SEC_AVG_POWER] = ts;
    } else {
        out->field_status[SEC_AVG_POWER] = section_failure_status(&s->data->avgPowerUsage.lastModifiedTimestamp);
    }

    RUSD_POWER_POLICY_STATUS policy;
    if (COPY(s->data->powerPolicyStatus, policy, SEC_POWER_POLICY)) {
        out->power_policy_tgp_mw = policy.info.tgpmW;
        out->timestamps[SEC_POWER_POLICY] = ts;
    } else {
        out->field_status[SEC_POWER_POLICY] = section_failure_status(&s->data->powerPolicyStatus.lastModifiedTimestamp);
    }

    RUSD_CLK_THROTTLE_INFO throttle;
    if (COPY(s->data->clkThrottleInfo, throttle, SEC_THROTTLE)) {
        out->throttle_reason_mask = throttle.reasonMask;
        out->timestamps[SEC_THROTTLE] = ts;
    } else {
        out->field_status[SEC_THROTTLE] = section_failure_status(&s->data->clkThrottleInfo.lastModifiedTimestamp);
    }

    RUSD_FAN_COOLER_STATUS fan;
    if (COPY(s->data->fanCoolerStatus, fan, SEC_FAN)) {
        for (unsigned i = 0; i < NVOC_RM_FAN_MAX_COOLERS; i++)
            out->fan_rpm[i] = fan.info.rpmCurr[i];
        out->timestamps[SEC_FAN] = ts;
    } else {
        out->field_status[SEC_FAN] = section_failure_status(&s->data->fanCoolerStatus.lastModifiedTimestamp);
    }

    RUSD_PERF_DEVICE_UTILIZATION util;
    if (COPY(s->data->perfDevUtil, util, SEC_UTIL)) {
        out->utilization_percent = util.info.gpuPercentBusy;
        out->memory_utilization_percent = util.info.memoryPercentBusy;
        out->timestamps[SEC_UTIL] = ts;
    } else {
        out->field_status[SEC_UTIL] = section_failure_status(&s->data->perfDevUtil.lastModifiedTimestamp);
    }

    RUSD_PCIE_DATA pci;
    if (COPY(s->data->pciBusData, pci, SEC_PCI)) {
        out->pci_gen_info = pci.info.data[RUSD_BUS_DATA_PCIE_GEN_INFO];
        out->timestamps[SEC_PCI] = ts;
    } else {
        out->field_status[SEC_PCI] = section_failure_status(&s->data->pciBusData.lastModifiedTimestamp);
    }

    RUSD_PROC_UTIL proc;
    if (COPY(s->data->procUtil, proc, SEC_PROC_UTIL)) {
        /* info.samples[] is a small ring; take the newest by its own stamp. */
        const RUSD_CTRL_PERF_GPUMON_PERFMON_UTIL_SAMPLE *newest = NULL;
        for (unsigned i = 0; i < RUSD_PROC_UTIL_SAMPLE_COUNT; i++) {
            const RUSD_CTRL_PERF_GPUMON_PERFMON_UTIL_SAMPLE *cur = &proc.info.samples[i];
            if (!newest || cur->timeStamp >= newest->timeStamp) newest = cur;
        }
        if (newest) {
            out->engine_utilization_pct100[0] = newest->gr.util;
            out->engine_utilization_pct100[1] = newest->fb.util;
            out->engine_utilization_pct100[2] = newest->nvenc.util;
            out->engine_utilization_pct100[3] = newest->nvdec.util;
            out->engine_utilization_pct100[4] = newest->nvjpg.util;
            out->engine_utilization_pct100[5] = newest->nvofa.util;
        }
        out->timestamps[SEC_PROC_UTIL] = ts;
    } else {
        out->field_status[SEC_PROC_UTIL] = section_failure_status(&s->data->procUtil.lastModifiedTimestamp);
    }
#undef COPY
}

/* One coherent, non-advancing read for the continuous monitor, which decides
 * field freshness/updates in Rust from the returned timestamps. */
static uint32_t sample_read(nvoc_rm_session *s, nvoc_rm_sample *out)
{
    memset(out, 0, sizeof(*out));
    out->abi_version = NVOC_RM_ABI_VERSION;
    sample_read_sections(s, out);
    read_live_controls(s, out);
    return 0;
}

/* Build the full private client -> device -> subdevice chain and run the
 * one-time identity/information queries. Leaves the session partially open on
 * failure so the caller can always call session_release(). */
static uint32_t chain_open(nvoc_rm_session *s, nvoc_rm_snapshot *out, char *error, size_t cap)
{
    s->ctl = -1; s->dev = -1;
    s->client = s->device = s->subdevice = 0;
    s->shared = 0; s->data = NULL; s->length = 0; s->abandon_chain = 0;
    out->abi_version = NVOC_RM_ABI_VERSION;
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
    /* When the unmap failed, freeing the shared object would release memory
     * that is still mapped; keeping the mapping alive but never reading it is
     * the safe order. Both the objects and the mapping belong to this
     * connection, which the fds below tear down. */
    if (s->abandon_chain) {
        s->shared = 0;
    } else {
        if (s->shared) nvx_rm_free(s->ctl, s->client, s->subdevice, s->shared);
        s->shared = 0;
        nvx_rm_chain_close(s->ctl, s->client, s->device, s->subdevice);
    }
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
    ap.polledDataMask = NVOC_RM_POLL_MASK;
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
            if (out) {
                out->status = UINT32_MAX;
                for (unsigned i = 0; i < SEC_FIELDS; i++)
                    out->sample.field_status[i] = NVOC_RM_FIELD_UNAVAILABLE;
            }
            return;
        }
        s->data = NULL;
    }
    if (s->shared) nvx_rm_free(s->ctl, s->client, s->subdevice, s->shared);
    s->shared = 0;
}

/* Mapping-time watermark for every polled section: a caller must observe a
 * later value before it may treat a reading as new. 0 means the driver has no
 * valid timestamp for that section right now. */
static void capture_baseline(nvoc_rm_session *s, uint64_t *baseline)
{
    for (unsigned i = 0; i < SEC_FIELDS; i++)
        baseline[i] = section_timestamp(s->data, i);
}

/* Repeatedly read until every polled section advances past the watermarks
 * taken just after mapping, or the 1.5 s budget expires. Used once per session
 * (and by the single-shot path) so the first exposed sample is known-fresh;
 * sections that never advanced are marked NVOC_RM_FIELD_NOT_FRESH rather than
 * presented as a fresh reading. */
static void sample_wait_fresh(nvoc_rm_session *s, nvoc_rm_sample *out)
{
    NvU64 baseline[SEC_FIELDS];
    capture_baseline(s, baseline);
    struct timespec delay = { .tv_nsec = 100000000 };
    for (unsigned attempt = 0; attempt < 16; attempt++) {
        /* Re-read from scratch: a section that looked quiet on an earlier
         * attempt must get another chance instead of staying stamped as
         * "not fresh" for the rest of the budget. */
        memset(out, 0, sizeof(*out));
        out->abi_version = NVOC_RM_ABI_VERSION;
        sample_read_sections(s, out);
        read_live_controls(s, out);
        unsigned resolved = 0;
        for (unsigned i = 0; i < SEC_FIELDS; i++) {
            NvU64 observed = out->timestamps[i];
            /* A section the driver does not publish is final: there is
             * nothing to wait for, so it must not hold the budget open. */
            if (out->field_status[i] == NVOC_RM_FIELD_UNAVAILABLE) {
                resolved++;
                continue;
            }
            if (out->field_status[i] != NVOC_RM_FIELD_OK) continue;
            if (observed != baseline[i] || observed >= RUSD_SEQ_START) {
                resolved++;
            } else {
                /* Readable but still on the mapping-time watermark. */
                out->field_status[i] = NVOC_RM_FIELD_NOT_FRESH;
                out->timestamps[i] = 0;
            }
        }
        if (resolved == SEC_FIELDS) break;
        if (attempt < 15) nanosleep(&delay, NULL);
    }
}

/* Must be serialized by the Rust caller: nvx_xnu owns a process-global fd
 * table. NullMoth's CARD_INFO currently describes the first matching service. */
uint32_t nvoc_macos_rm_snapshot(nvoc_rm_snapshot *out, size_t size, uint32_t telemetry,
                               char *error, size_t cap)
{
    if (!out) return failure(error, cap, "snapshot without output buffer", UINT32_MAX);
    if (size != sizeof(*out)) return failure(error, cap, "snapshot ABI size", UINT32_MAX);
    memset(out, 0, sizeof(*out));
    nvoc_rm_session s;
    uint32_t result = chain_open(&s, out, error, cap);
    if (!result && telemetry) {
        uint32_t status = 0;
        memset(&out->telemetry, 0, sizeof out->telemetry);
        if (telemetry_map(&s, &status)) {
            out->telemetry.status = status;
            for (unsigned i = 0; i < SEC_FIELDS; i++)
                out->telemetry.sample.field_status[i] = NVOC_RM_FIELD_UNAVAILABLE;
        } else {
            sample_wait_fresh(&s, &out->telemetry.sample);
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
                                   size_t info_size, uint64_t *baseline, size_t baseline_count,
                                   char *error, size_t cap)
{
    if (!out_session || !info)
        return failure(error, cap, "session open without output buffers", UINT32_MAX);
    if (info_size != sizeof(*info))
        return failure(error, cap, "session info ABI size", UINT32_MAX);
    if (baseline && baseline_count != SEC_FIELDS)
        return failure(error, cap, "session baseline ABI size", UINT32_MAX);
    *out_session = NULL;
    memset(info, 0, sizeof(*info));
    nvoc_rm_session *s = calloc(1, sizeof(*s));
    if (!s) return failure(error, cap, "session allocation", UINT32_MAX);
    uint32_t result = chain_open(s, info, error, cap);
    if (!result) {
        uint32_t status = 0;
        /* No wait here: the first sample() reports advancement relative to the
         * mapping-time baseline, so the Rust caller owns the wait policy. */
        if (telemetry_map(s, &status)) {
            result = failure(error, cap, "RUSD allocate/map", status);
        } else if (baseline) {
            capture_baseline(s, baseline);
        }
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
    if (!s || !out) return failure(error, cap, "session sample without buffers", UINT32_MAX);
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

/* Cross-language ABI fingerprint: the ignored hardware test compares these
 * numbers with size_of/offset_of! on the Rust side, so a layout change cannot
 * pass unnoticed on either side. See NVOC_RM_LAYOUT_SLOTS in
 * core/native/macos_rm_abi.h for the slot meanings. */
size_t nvoc_macos_rm_abi_layout(uint64_t *out, size_t count)
{
    const uint64_t layout[NVOC_RM_LAYOUT_SLOTS] = {
        sizeof(nvoc_rm_snapshot),
        sizeof(nvoc_rm_sample),
        sizeof(nvoc_rm_telemetry),
        offsetof(nvoc_rm_snapshot, telemetry),
        offsetof(nvoc_rm_snapshot, driver),
        offsetof(nvoc_rm_telemetry, sample),
        offsetof(nvoc_rm_sample, timestamps),
        offsetof(nvoc_rm_sample, temperature_fx256),
        offsetof(nvoc_rm_sample, clk_target_mhz),
        offsetof(nvoc_rm_sample, clk_actual_khz),
        offsetof(nvoc_rm_sample, power_mw),
        offsetof(nvoc_rm_sample, fan_rpm),
        offsetof(nvoc_rm_sample, engine_utilization_pct100),
        offsetof(nvoc_rm_sample, pci_gen_info),
        offsetof(nvoc_rm_sample, pstate),
        offsetof(nvoc_rm_sample, memory_used_kb),
        NVOC_RM_ABI_VERSION,
        NVOC_RM_RUSD_FIELDS,
        NVOC_RM_FIELD_OK,
        NVOC_RM_FIELD_UNSTABLE,
        NVOC_RM_FIELD_UNAVAILABLE,
        NVOC_RM_FIELD_NOT_FRESH,
        NVOC_RM_CLK_DOMAINS,
        NVOC_RM_TEMP_SENSORS,
        NVOC_RM_FAN_MAX_COOLERS,
        NVOC_RM_ENGINES,
        NVOC_RM_POLL_MASK,
        NVOC_RM_RUSD_CLOCK,
        NVOC_RM_RUSD_TEMP_GPU,
        NVOC_RM_RUSD_TEMP_MEMORY,
        NVOC_RM_RUSD_TEMP_BOARD,
        NVOC_RM_RUSD_TEMP_POWER_SUPPLY,
        NVOC_RM_RUSD_POWER,
        NVOC_RM_RUSD_POWER_LIMIT,
        NVOC_RM_RUSD_AVG_POWER,
        NVOC_RM_RUSD_POWER_POLICY,
        NVOC_RM_RUSD_THROTTLE,
        NVOC_RM_RUSD_FAN,
        NVOC_RM_RUSD_UTIL,
        NVOC_RM_RUSD_PCI,
        NVOC_RM_RUSD_PROC_UTIL,
    };
    if (out && count) {
        size_t n = count < NVOC_RM_LAYOUT_SLOTS ? count : NVOC_RM_LAYOUT_SLOTS;
        for (size_t i = 0; i < n; i++) out[i] = layout[i];
    }
    return NVOC_RM_LAYOUT_SLOTS;
}
