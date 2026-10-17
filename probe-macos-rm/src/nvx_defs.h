/*
 * probe-macos-rm — NVOC macOS RM-surface validation suite
 *
 * Local constants that are NOT shipped in the public open-gpu-kernel-modules
 * (OGKM) SDK headers. Every value here is provenance-annotated; do not add a
 * constant without a source you verified.
 *
 * Everything else (NVOS parameter structs, NV_ESC_* numbers, class alloc
 * params, ctrl param structs, RUSD layout) comes straight from the OGKM
 * 610.57.04 headers at build time — see build.sh for the include set.
 */
#ifndef NVX_DEFS_H
#define NVX_DEFS_H

#include "nvtypes.h"

/*
 * RM object class IDs (verified against OGKM 610.57.04:
 *   src/nvidia/generated/g_allclasses.h:289  NV01_ROOT_CLIENT = 0x41
 *   src/common/sdk/nvidia/inc/class/cl0080.h:36  NV01_DEVICE_0 = 0x80
 *   src/common/sdk/nvidia/inc/class/cl2080.h:36  NV20_SUBDEVICE_0 = 0x2080
 *   NV00DE (RM_USER_SHARED_DATA) = 0xDE, same generated header.
 * cl0080/cl2080 are included directly; the generated-only ones are here.
 */
#define NVX_CLASS_ROOT_CLIENT  0x41u
#define NVX_CLASS_USER_SHARED_DATA 0x00DEu

/* Channel-smoke class IDs (verified against OGKM 610.57.04
 * src/nvidia/generated/g_allclasses.h): */
#define NVX_CLASS_CONTEXT_DMA        0x02u   /* g_allclasses.h:321 */
#define NVX_CLASS_MEMORY_SYSTEM      0x3Eu   /* g_allclasses.h:325 */
#define NVX_CLASS_MEMORY_LOCAL_USER  0x40u   /* g_allclasses.h:345 */
#define NVX_CLASS_MEMORY_VIRTUAL_NV50 0x50A0u /* g_allclasses.h:879 */
#define NVX_CLASS_VASPACE_FERMI_A    0x90F1u /* g_allclasses.h:895 */
#define NVX_CLASS_USERMODE_TURING_A  0xC461u /* g_allclasses.h:587 */

/* USERD GP FIFO get/put registers, byte offsets inside the USERD page.
 * Source: OGKM class/clc46f.h (Nvc46fControl/TuringAControlGPFifo:
 * GPGet 0x88, GPPut 0x8C). */
#define NVX_USERD_GPGET 0x88u
#define NVX_USERD_GPPUT 0x8Cu

/* Channel engine type. Source: OGKM class/cl2080_notification.h:284. */
#define NVX_ENGINE_TYPE_GRAPHICS 1u

/*
 * Tier-B legacy RM subdevice controls: stable GSP-RM wire IDs that no longer
 * ship as OGKM SDK headers (GSP firmware implements them; NVML/NVAPI use the
 * same IDs on Linux). Sources are NVOC's own reverse-engineering campaign.
 * GET faces are probed read-only in stage 2; SET faces are documented in
 * VALIDATION-MATRIX.md only and deliberately not sent by v1 of this tool.
 */

/* Per-rail voltage rail inventory/status. Source:
 * docs/reverse-engineering/nvapi/voltrails-family-full-layout-r610.md
 *   GetInfo  0x2080B201, record stride 76 B, type byte @+0x44, class byte @+0x77
 *   GetStatus 0x2080B202, record stride 100 B, type byte @+88
 * (Cross-verified as the NVML rail-voltage server path in
 *  blackwell-pro6000-580-173-02-audit.md T3-10.) */
#define NVX_RM_CMD_RAILS_GET_INFO    0x2080B201u
#define NVX_RM_CMD_RAILS_GET_STATUS  0x2080B202u
#define NVX_RAILS_INFO_STRIDE        76u
#define NVX_RAILS_STATUS_STRIDE      100u

/* Accepted `paramsSize` for the Tier-B GETs, measured on 610.57.04/NullMoth
 * (2026-10-17). A smaller buffer is answered NV_ERR_INVALID_ARGUMENT (0x1f),
 * which is why the first version of this probe -- which passed one record's
 * worth of stride -- recorded a flat "driver refuses" for all three.
 *
 * The two rail sizes are consistent with the documented record strides for a
 * 32-record table plus a fixed header (12 + 32*76 = 2444, 32 + 32*100 = 3232);
 * the VF size (524) matches neither 10 nor 32 records of the documented 68 B
 * stride, so that layout is still unresolved.  The payloads themselves are
 * NOT decoded: they are stable across processes and unchanged under GPU load,
 * so they are not usable as readings. */
#define NVX_RAILS_INFO_PARAM_SIZE    2444u
#define NVX_RAILS_STATUS_PARAM_SIZE  3232u
#define NVX_VFPOINTS_PARAM_SIZE      524u

/* Private V/F point curve GET. Source:
 * blackwell-pro6000-580-173-02-audit.md T3-1:
 *   GET 0x208090A2, v1 record 10x68 B / v2 record 32x68 B.
 * SET counterpart 0x2080D0A3 (single-slot VF write) — documented, not sent. */
#define NVX_RM_CMD_VFPOINTS_GET      0x208090A2u
#define NVX_VFPOINTS_V1_RECORDS      10u
#define NVX_VFPOINTS_RECORD_STRIDE   68u

/* Voltage-limit SET (MODS Volt3x.SetVoltagesMv -> SendVoltLimitsToRM).
 * Source: reverse/mods_analysis/REPORT.md + A4_mods5050.md; 7 logical rails
 * x 316 B/record, uV precision. DELIBERATELY NOT SENT by v1: over-volt
 * pre-check bypass (registry key RmVFPointCheckIgnore) has no confirmed
 * macOS kext equivalent yet. Kept here so the matrix doc and the code agree
 * on the ID. */
#define NVX_RM_CMD_VOLT_LIMITS_SET   0x20801ACFu

/* NVML-visible VF clock offset SET (nvmlDeviceSetGpcClkVfOffset).
 * Source: blackwell-pro6000-580-173-02-audit.md T2-10: RM 0x2080D031 on 580
 * (610 doc records 0x2080F031; params 0x608 bytes either way) — version fork
 * unresolved, so v1 does not send it. Documented in the matrix. */
#define NVX_RM_CMD_GPC_VF_OFFSET_SET 0x2080D031u

/* RUSD poll mask for the NV00DE alloc (cl00de.h NV00DE_RUSD_POLL_*):
 * CLOCK|PERF|POWER|THERMAL|FAN */
#define NVX_RUSD_POLL_MASK           0x5Bu

#endif /* NVX_DEFS_H */
