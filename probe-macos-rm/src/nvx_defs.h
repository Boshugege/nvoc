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

/* ------------------------------------------------------------------ */
/* Private ClockClient / VfPoints / PerfVfe READ faces                 */
/* ------------------------------------------------------------------ */
/* NVOC's own Windows backend reads the editable core/memory clock offsets
 * and the V/F curve through the private ClockClient family. The ids below are
 * the GET (read) faces of exactly those wires -- taken from our own nvapi-rs
 * bindings and the NVAPI RE campaign docs, NOT from the OGKM SDK (the
 * 610.57.04 open tree keeps the interface names in g_finn_rm_api.h but strips
 * the command defines: docs/reverse-engineering/nvml/binding-strategy-
 * evaluation.md L58, "接口在、命令剥离").
 *
 * Provenance discipline: every id here is annotated with the nvapi-rs /
 * reverse-engineering doc it came from. The SET twins of these families are
 * listed in nvx_known_set_ids (nvx_suite.c) and the census refuses to send
 * one even if a later edit pastes the wrong constant.
 */

/* Physical clock measurement for one clock domain. Source:
 * nvapi-rs/src/clock.rs:524 (NVAPI 0xFB8F61EC) -- domain index in, physical
 * frequency in kHz out. Read-only. */
#define NVX_RM_CMD_CLK_MEASURE_FREQ        0x20809006u

/* Controllable clock-domain block (get-private-freq-domain-info on Windows).
 * Source: nvapi-rs/src/clock.rs:360 / gpu.rs:1797 (NVAPI 0xF58938F5; V1 magic
 * 0x10964, V2 magic 0x261A4). SET twin is 0x2080D01C. */
#define NVX_RM_CMD_CLK_DOMAINS_GET_CONTROL 0x2080901Bu

/* Private ClockClient V/F-points trio (get-private-vftable on Windows).
 * Source: nvapi-rs/src/gpu.rs:2366 + clock.rs:586/681 (NVAPI 0x8895B510 /
 * 0x7FEE9032 / 0xDA025C3E). GET_INFO = present-point masks, GET_STATUS = the
 * 488 B curve records, GET_CONTROL = the applied-offset readback whose SET
 * twin is 0x2080D024. RE'd as 0x2080906x on 582.41
 * (docs/reverse-engineering/nvapi/p100-58241-privatevftable-audit.md §2) and
 * renumbered to 0x2080902x by R610. */
#define NVX_RM_CMD_VFP_GET_INFO    0x20809021u  /* NVAPI 0x8895B510 */
#define NVX_RM_CMD_VFP_GET_STATUS  0x20809022u  /* NVAPI 0x7FEE9032 */
#define NVX_RM_CMD_VFP_GET_CONTROL 0x20809023u  /* NVAPI 0xDA025C3E */

/* 582.41-era numbering of the same family (p100-58241-privatevftable-audit.md
 * §2/§5: GET_INFO 0x20809061 / GET_STATUS 0x20809062 / the 0x20809022+0x20809025
 * GET handlers). Kept so the census can show which numbering this driver
 * answers instead of guessing. */
#define NVX_RM_CMD_VFP_GET_INFO_582    0x20809061u
#define NVX_RM_CMD_VFP_GET_STATUS_582  0x20809062u
#define NVX_RM_CMD_VFP_GET_AUX_582     0x20809025u

/* PerfVfe equation/variable GET faces. Source: nvapi-rs/src/gpu.rs:3922/3954/
 * 4046/4077 and docs/reverse-engineering/nvapi/oc-gap-layouts-r610-74.md §B3
 * (NVAPI 0x8D49471C / 0x4C75C9FE / 0xB9DA41D6 / 0x5D387298). SET twins are
 * 0x2080E0B7 (Equ) and 0x2080E0B0 (Var). */
#define NVX_RM_CMD_VFE_EQU_GET_INFO    0x2080A0B5u  /* NVAPI 0x8D49471C */
#define NVX_RM_CMD_VFE_EQU_GET_CONTROL 0x2080A0B6u  /* NVAPI 0x4C75C9FE */
#define NVX_RM_CMD_VFE_VAR_GET_INFO    0x2080A0B1u  /* NVAPI 0xB9DA41D6 */
#define NVX_RM_CMD_VFE_VAR_GET_CONTROL 0x2080A0B3u  /* NVAPI 0x5D387298 */

/* VoltVoltRails GetControl -- the readback half of the rail family whose
 * GetInfo/GetStatus are already probed as V2.9/V2.10. Source:
 * docs/reverse-engineering/nvapi/voltrails-family-full-layout-r610.md (32 B
 * records, type byte @record+0). SET twin is 0x2080F214. */
#define NVX_RM_CMD_RAILS_GET_CONTROL 0x2080B213u

/* ------------------------------------------------------------------ */
/* Documented STAMP + SEED values for the same read faces              */
/* ------------------------------------------------------------------ */
/* The census above answered "which paramSizes does this driver accept" with
 * the driver's own INVALID_ARGUMENT oracle. It deliberately sent ZEROED
 * buffers, which cannot satisfy a handler that validates dword[0] (the
 * stamp) and/or an input mask seed -- so "0x1f at every size" says the
 * documented header was missing, not that the command is absent.
 *
 * These are the literal stamp values the Windows handlers compare dword[0]
 * against, per nvapi-rs/sys/src/gpu/clock.rs + power.rs and the R610 RE docs.
 * NOTE the V/F-points family stamp is NOT (version << 16 | size).
 */
#define NVX_STAMP_CLK_DOMAINS_INFO_V1      0x0109D8u /* |2520  clock.rs:1930 */
#define NVX_STAMP_CLK_DOMAINS_INFO_610A    0x0486ACu /* |296620 clock.rs:1933 */
#define NVX_STAMP_CLK_DOMAINS_INFO_610B    0x0506ACu /* |329388 clock.rs:1933 */
#define NVX_STAMP_CLK_DOMAINS_CONTROL_V1   0x010964u /* |2404, mask@+8 */
#define NVX_STAMP_CLK_DOMAINS_CONTROL_V2   0x0261A4u /* |24996 */
#define NVX_STAMP_CLK_MEASURE_V1           0x010020u /* 32B, domain@+4 */
#define NVX_STAMP_CLK_MEASURE_V2           0x020020u /* 32B, u64 counter */
#define NVX_STAMP_VFP_INFO                 0x078604u /* 493060B, NOT ver|size */
#define NVX_STAMP_VFP_INFO_LEGACY          83996u    /* 0x1481C */
#define NVX_STAMP_VFP_INFO_WIDE            0x05A484u /* 369796B */
#define NVX_STAMP_VFP_STATUS               2000388u  /* 0x1E8604 */
#define NVX_STAMP_VFP_CONTROL              0x474604u /* 4343300B */
#define NVX_STAMP_VFP_CONTROL_SNAPSHOT     82976u    /* 0x14420 */
#define NVX_STAMP_CLK_FREQ_CTRL_INFO       0x010C4Cu /* 3148B  clock.rs:5421 */
#define NVX_STAMP_CLK_FREQ_CTRL_STATUS     0x0109CCu /* 2508B  clock.rs:5466 */

/* Private ClockClient clock-domains GET_INFO (RM 0x20809019) -- the article's
 * domain-discovery API (clock.rs:1930). Distinct from GET_CONTROL 0x2080901B
 * and from the 582.41-numered VFP trio. Read-only. */
#define NVX_RM_CMD_CLK_DOMAINS_GET_INFO    0x20809019u
/* ClkFreqController directory + status (clock.rs:5402/5455). RM 0x20809025
 * carries {max_kHz, -18750/+18750} per domain on TU116 -- and this bench's
 * GTX 1650 SUPER IS TU116, so it is a first-class candidate here. */
#define NVX_RM_CMD_CLK_FREQ_CTRL_INFO      0x20809025u
#define NVX_RM_CMD_CLK_FREQ_CTRL_STATUS    0x20809026u
/* VoltVoltRails GetInfo/GetStatus (power.rs:61): same op family as B213. */
#define NVX_RM_CMD_RAILS_GET_INFO          0x2080B201u
#define NVX_RM_CMD_RAILS_GET_STATUS        0x2080B202u

#endif /* NVX_DEFS_H */
