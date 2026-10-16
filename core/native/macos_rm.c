/* Thin read-only ABI over NVOC's RM transport. NVIDIA layouts come from OGKM,
 * not handwritten Rust copies. No probe CLI, shared-page mapping or SET calls. */
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
#include <string.h>

typedef struct {
    uint32_t abi_version, rm_gpu_id, domain, bus, slot, function;
    uint32_t vendor_id, device_id, subsystem_id, revision, board_id, class_count;
    uint64_t total_bytes, usable_bytes, bar1_bytes, l2_bytes;
    uint32_t pstate, memory_status, pstate_status, pci_status, bios_status, name_status, class_status;
    char driver[64], name[64], sku[16], project[16];
} nvoc_rm_snapshot;

_Static_assert(sizeof(nvoc_rm_snapshot) == 272, "Rust/C snapshot ABI mismatch");

static uint32_t failure(char *error, size_t cap, const char *operation, uint32_t status)
{
    snprintf(error, cap, "%s: 0x%08x (%s)", operation, status, nvx_status_name(status));
    return status ? status : UINT32_MAX;
}

/* Must be serialized by the Rust caller: nvx_xnu owns a process-global fd table.
 * NullMoth's CARD_INFO currently describes the first matching service only. */
uint32_t nvoc_macos_rm_snapshot(nvoc_rm_snapshot *out, size_t size, char *error, size_t cap)
{
    if (size != sizeof(*out)) return failure(error, cap, "snapshot ABI size", UINT32_MAX);
    memset(out, 0, sizeof(*out));
    out->abi_version = 1;
    int ctl = nvx_open(1), dev = -1;
    NvHandle client = 0, device = 0, subdevice = 0;
    uint32_t result = UINT32_MAX, st = UINT32_MAX;
    const char *operation = "open NullMoth IOKit control service (driver loaded?)";
    if (ctl < 0) goto done;
    operation = "CARD_INFO";
    nv_ioctl_card_info_t card = {0};
    if (nvx_esc(ctl, NV_ESC_CARD_INFO, &card, sizeof card) || !card.valid) goto done;
    out->rm_gpu_id = card.gpu_id;
    out->domain = card.pci_info.domain; out->bus = card.pci_info.bus;
    out->slot = card.pci_info.slot; out->function = card.pci_info.function;
    out->vendor_id = card.pci_info.vendor_id; out->device_id = card.pci_info.device_id;
    operation = "RM version handshake";
    nv_ioctl_rm_api_version_t version = {0};
    version.cmd = NV_RM_API_VERSION_CMD_QUERY;
    snprintf(version.versionString, sizeof version.versionString, "610.57.04");
    if (nvx_esc(ctl, NV_ESC_CHECK_VERSION_STR, &version, sizeof version) ||
        version.reply != NV_RM_API_VERSION_REPLY_RECOGNIZED ||
        strncmp(version.versionString, "610.57.04", sizeof version.versionString)) goto done;
    snprintf(out->driver, sizeof out->driver, "%s", version.versionString);
    operation = "open NullMoth IOKit device service";
    dev = nvx_open(0);
    if (dev < 0) goto done;
    operation = "REGISTER_FD";
    nv_ioctl_register_fd_t rf = {0}; rf.ctl_fd = ctl;
    if (nvx_esc(dev, NV_ESC_REGISTER_FD, &rf, sizeof rf)) goto done;
    operation = "allocate root client";
    client = nvx_rm_alloc(ctl, 0, 0, 0xcafe0000, NVX_CLASS_ROOT_CLIENT, NULL, 0, &st);
    if (!client) goto done;
    operation = "attach GPU";
    NV0000_CTRL_GPU_ATTACH_IDS_PARAMS attach = {0};
    attach.gpuIds[0] = card.gpu_id; attach.gpuIds[1] = NV0000_CTRL_GPU_INVALID_ID;
    st = nvx_rm_control(ctl, client, client, NV0000_CTRL_CMD_GPU_ATTACH_IDS, &attach, sizeof attach);
    if (st) goto done;
    operation = "GPU device instance";
    NV0000_CTRL_GPU_GET_ID_INFO_V2_PARAMS id = {0}; id.gpuId = card.gpu_id;
    st = nvx_rm_control(ctl, client, client, NV0000_CTRL_CMD_GPU_GET_ID_INFO_V2, &id, sizeof id);
    if (st) goto done;
    operation = "allocate device";
    NV0080_ALLOC_PARAMETERS dp = {0}; dp.deviceId = id.deviceInstance; dp.hClientShare = client;
    device = nvx_rm_alloc(ctl, client, client, 0xcafe0001, NV01_DEVICE_0, &dp, sizeof dp, &st);
    if (!device) goto done;
    operation = "allocate subdevice";
    NV2080_ALLOC_PARAMETERS sp = {0};
    subdevice = nvx_rm_alloc(ctl, client, device, 0xcafe0002, NV20_SUBDEVICE_0, &sp, sizeof sp, &st);
    if (!subdevice) goto done;
#define QUERY(cmd, p) nvx_rm_control(ctl, client, subdevice, cmd, &(p), sizeof(p))
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
    out->class_status = nvx_rm_control(ctl, client, device,
        NV0080_CTRL_CMD_GPU_GET_CLASSLIST_V2, &classes, sizeof classes);
    if (!out->class_status) out->class_count = classes.numClasses;
    result = 0;
done:
    if (result) result = failure(error, cap, operation, st);
    /* Free partially-created chains as well as successful sessions. */
    nvx_rm_chain_close(ctl, client, device, subdevice);
    if (dev >= 0) nvx_close(dev);
    if (ctl >= 0) nvx_close(ctl);
    return result;
}
