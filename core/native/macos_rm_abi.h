/* Shared C ABI for the macOS RM backend.
 *
 * This header is the C-side source of truth for the struct layouts and the
 * entry-point signatures that core/src/macos_rm.rs declares in its
 * `unsafe extern "C"` block. macos_rm.c includes it, so a definition that
 * drifts from the declared prototype fails to compile; the layout assertions
 * below pin the same numbers the Rust side asserts with `offset_of!`/
 * `size_of`, and nvoc_macos_rm_abi_layout() exposes them at runtime so an
 * ignored hardware test can compare the two languages for real instead of
 * trusting a comment.
 *
 * Contract for every entry point (Rust always follows it):
 *   - `error` may be NULL and `cap` may be 0; failure() then skips the message.
 *   - a non-NULL `error` with `cap > 0` always receives a NUL-terminated
 *     reason on a non-zero return.
 *   - a non-zero return means "nothing usable was produced"; outputs are only
 *     read by the caller on return 0.
 *   - `*_size` arguments must equal sizeof() of the corresponding struct; a
 *     mismatch is rejected instead of writing a partially-filled struct.
 *   - the caller serializes all entry points (nvx_xnu owns a process-global
 *     fd table).
 *
 * ABI v3 carries the full read-only RUSD page plus the two live RM controls
 * (P-state, FB heap usage) through one section table, so the one-shot and the
 * continuous paths interpret driver telemetry through identical field rules.
 * ABI v4 adds the private clock-offset channel entry point below. No struct
 * layout changed with it, so every assertion above still holds; the version
 * moves because a caller cannot tell from the layout whether the channel is
 * reachable at all.
 */
#ifndef NVOC_MACOS_RM_ABI_H
#define NVOC_MACOS_RM_ABI_H

#include <stddef.h>
#include <stdint.h>

#define NVOC_RM_ABI_VERSION 5u

/* Per-section status codes carried in nvoc_rm_sample.field_status. */
/* The section was copied under a stable, data-valid sequence. */
#define NVOC_RM_FIELD_OK 0u
/* The section's RUSD sequence or timestamp changed while it was copied: no
 * coherent value this round. Retrying later is reasonable. */
#define NVOC_RM_FIELD_UNSTABLE 1u
/* The section carries no valid timestamp: the driver does not publish it on
 * this platform right now (cl00de.h RUSD_TIMESTAMP_INVALID). Retrying will
 * not help until the driver starts polling that section. */
#define NVOC_RM_FIELD_UNAVAILABLE 2u
/* One-shot only: the section was readable but did not publish a new value
 * inside the initial freshness budget, so the value may predate the query. */
#define NVOC_RM_FIELD_NOT_FRESH 3u

/* Polled RUSD sections, in the order the Rust side indexes them. The list is
 * exactly the NV00DE poll mask nvoc_macos_rm requests (see NVOC_RM_POLL_MASK)
 * and is limited to sections the 610.57.04 driver actually populates: memEcc
 * and memRowRemap stay RUSD_TIMESTAMP_INVALID even when POLL_MEMORY is
 * requested, so they are deliberately not carried. */
#define NVOC_RM_RUSD_FIELDS 14
#define NVOC_RM_RUSD_CLOCK 0
#define NVOC_RM_RUSD_TEMP_GPU 1
#define NVOC_RM_RUSD_TEMP_MEMORY 2
#define NVOC_RM_RUSD_TEMP_BOARD 3
#define NVOC_RM_RUSD_TEMP_POWER_SUPPLY 4
#define NVOC_RM_RUSD_POWER 5
#define NVOC_RM_RUSD_POWER_LIMIT 6
#define NVOC_RM_RUSD_AVG_POWER 7
#define NVOC_RM_RUSD_POWER_POLICY 8
#define NVOC_RM_RUSD_THROTTLE 9
#define NVOC_RM_RUSD_FAN 10
#define NVOC_RM_RUSD_UTIL 11
#define NVOC_RM_RUSD_PCI 12
#define NVOC_RM_RUSD_PROC_UTIL 13

/* Fixed-size value blocks inside nvoc_rm_sample. */
#define NVOC_RM_CLK_DOMAINS 4      /* graphics, memory, video, sm */
#define NVOC_RM_TEMP_SENSORS 4     /* gpu, memory, board, power supply */
#define NVOC_RM_FAN_MAX_COOLERS 16
#define NVOC_RM_ENGINES 6          /* graphics, framebuffer, nvenc, nvdec, nvjpg, nvofa */

/* Poll mask passed to the NV00DE alloc: every section nvoc_rm_sample carries.
 * POLL_MEMORY is intentionally absent (see the section list above). */
#define NVOC_RM_POLL_MASK 0xFBu

/* One coherent read of the mapped page. field_status[i] == NVOC_RM_FIELD_OK
 * means the value was copied under a stable, data-valid RUSD sequence;
 * nonzero means that section produced nothing usable this round and its value
 * fields must be ignored. timestamps[i] is the section's lastModifiedTimestamp
 * as observed (polled sections carry a host realtime nanosecond stamp) and is
 * 0 when unknown.
 *
 * `pstate`/`pstate_status` and `memory_used_kb`/`memory_used_status` come from
 * live RM controls re-read on every sample, not from the mapped page; their
 * values are new whenever the status is 0. */
typedef struct {
    uint32_t abi_version;
    uint32_t field_status[NVOC_RM_RUSD_FIELDS];
    uint64_t timestamps[NVOC_RM_RUSD_FIELDS];
    /* CLOCK: target (MHz) and actual (kHz) for graphics, memory, video, sm.
     * A domain the driver does not implement reports 0/0. */
    uint32_t clk_target_mhz[NVOC_RM_CLK_DOMAINS];
    uint32_t clk_actual_khz[NVOC_RM_CLK_DOMAINS];
    /* TEMP_*: fixed-point 8.8 Celsius, same encoding as NvTemp. */
    int32_t temperature_fx256[NVOC_RM_TEMP_SENSORS];
    uint32_t power_mw, power_limit_requested_mw, power_limit_mw;
    uint32_t average_power_mw, power_policy_tgp_mw, throttle_reason_mask;
    /* FAN: one RUSD section; rpmCurr[] copied verbatim. The driver does not
     * report how many coolers exist, so callers treat 0 entries as "no
     * reading" and validate plausible ranges themselves. */
    uint32_t fan_rpm[NVOC_RM_FAN_MAX_COOLERS];
    /* UTIL: perfDevUtil percentages. */
    uint32_t utilization_percent, memory_utilization_percent;
    /* PROC_UTIL: engine busy percent x100 (cl00de.h units), newest sample. */
    uint32_t engine_utilization_pct100[NVOC_RM_ENGINES];
    /* PCI: RUSD_BUS_DATA_PCIE_GEN_INFO copied verbatim (cl00de.h says it
     * matches NV2080_CTRL_BUS_INFO_INDEX_PCIE_GEN_INFO). Decoded by Rust. */
    uint32_t pci_gen_info;
    uint32_t pstate, pstate_status;
    /* FB_GET_INFO_V2: used video memory (heap size - heap free), kilobytes. */
    uint32_t memory_used_kb, memory_used_status;
} nvoc_rm_sample;

/* One-shot telemetry: the mapped-page status plus the same section table the
 * continuous path uses. `status` is the RUSD allocate/map failure (0 = ok) and
 * `sample.field_status` distinguishes per-section failures, including
 * NVOC_RM_FIELD_NOT_FRESH for a section that was readable but stayed on its
 * mapping-time watermark for the whole wait budget. */
typedef struct {
    uint32_t status;
    nvoc_rm_sample sample;
} nvoc_rm_telemetry;

typedef struct {
    uint32_t abi_version, rm_gpu_id, domain, bus, slot, function;
    uint32_t vendor_id, device_id, subsystem_id, revision, board_id, class_count;
    uint64_t total_bytes, usable_bytes, bar1_bytes, l2_bytes;
    uint32_t pstate, memory_status, pstate_status, pci_status, bios_status, name_status, class_status;
    char driver[64], name[64], sku[16], project[16];
    nvoc_rm_telemetry telemetry;
} nvoc_rm_snapshot;

/* Opaque: defined in macos_rm.c. Rust only ever holds a pointer. */
typedef struct nvoc_rm_session nvoc_rm_session;

_Static_assert(sizeof(nvoc_rm_sample) == 368, "nvoc_rm_sample ABI drift");
_Static_assert(sizeof(nvoc_rm_telemetry) == 376, "nvoc_rm_telemetry ABI drift");
_Static_assert(sizeof(nvoc_rm_snapshot) == 648, "nvoc_rm_snapshot ABI drift");

_Static_assert(offsetof(nvoc_rm_snapshot, telemetry) == 272, "snapshot.telemetry offset drift");
_Static_assert(offsetof(nvoc_rm_snapshot, driver) == 108, "snapshot.driver offset drift");
_Static_assert(offsetof(nvoc_rm_telemetry, sample) == 8, "telemetry.sample offset drift");
_Static_assert(offsetof(nvoc_rm_sample, timestamps) == 64, "sample.timestamps offset drift");
_Static_assert(offsetof(nvoc_rm_sample, clk_target_mhz) == 176, "sample.clk_target_mhz offset drift");
_Static_assert(offsetof(nvoc_rm_sample, clk_actual_khz) == 192, "sample.clk_actual_khz offset drift");
_Static_assert(offsetof(nvoc_rm_sample, temperature_fx256) == 208, "sample.temperature_fx256 offset drift");
_Static_assert(offsetof(nvoc_rm_sample, power_mw) == 224, "sample.power_mw offset drift");
_Static_assert(offsetof(nvoc_rm_sample, fan_rpm) == 248, "sample.fan_rpm offset drift");
_Static_assert(offsetof(nvoc_rm_sample, utilization_percent) == 312, "sample.utilization_percent offset drift");
_Static_assert(offsetof(nvoc_rm_sample, engine_utilization_pct100) == 320, "sample.engine_utilization_pct100 offset drift");
_Static_assert(offsetof(nvoc_rm_sample, pci_gen_info) == 344, "sample.pci_gen_info offset drift");
_Static_assert(offsetof(nvoc_rm_sample, pstate) == 348, "sample.pstate offset drift");
_Static_assert(offsetof(nvoc_rm_sample, memory_used_kb) == 356, "sample.memory_used_kb offset drift");

/* One-shot read: open a chain, capture identity, optionally wait for fresh
 * telemetry, release everything. `telemetry` is a boolean. */
uint32_t nvoc_macos_rm_snapshot(nvoc_rm_snapshot *out, size_t size, uint32_t telemetry,
                               char *error, size_t cap);

/* Persistent session. On success `*out_session` owns the connection chain and
 * the read-only RUSD mapping until nvoc_macos_rm_session_close().
 *
 * `baseline` (optional, `baseline_count` == NVOC_RM_RUSD_FIELDS) receives each
 * polled section's lastModifiedTimestamp as observed at mapping time, i.e. the
 * "already old" watermark a caller must see advance before it may treat a
 * later value as new. Sections that carry no valid timestamp report 0. */
uint32_t nvoc_macos_rm_session_open(nvoc_rm_session **out_session, nvoc_rm_snapshot *info,
                                   size_t info_size, uint64_t *baseline, size_t baseline_count,
                                   char *error, size_t cap);

/* Read one coherent sample from a live session. Never advances the session:
 * timestamps are reported as observed so the caller can decide freshness. */
uint32_t nvoc_macos_rm_session_sample(nvoc_rm_session *session, nvoc_rm_sample *out,
                                     size_t size, char *error, size_t cap);

/* The private clock-offset channel. Three read-only commands that are not in
 * the open kernel modules' headers; their geometry is the probe's measurement
 * on the device. Each is requested by kind and must be given exactly the size
 * its command declares. The payload is valid only on a zero return: while the
 * GSP is in its reset-required state all three answer 0x62 instead of data, and
 * that refusal must never be rendered as a zero offset. */
#define NVOC_RM_OFFSET_KINDS 4
#define NVOC_RM_OFFSET_DIRECTORY 0 /* 12336 bytes: domain bit -> record type */
#define NVOC_RM_OFFSET_RANGE 1     /* 7188 bytes: per-domain offset range, kHz */
#define NVOC_RM_OFFSET_CONTROL 2   /* 2108 bytes: domain control, current values */
#define NVOC_RM_OFFSET_CLOCK_DOCUMENT \
    3 /* 776 bytes: live per-domain frequency and raw extra fields */
uint32_t nvoc_macos_rm_session_offset(nvoc_rm_session *session, uint32_t kind, void *out,
                                      size_t size, char *error, size_t cap);

/* Private clock-offset transaction transport. The Rust backend serializes the
 * entire transaction and validates identity/mode/range. SET requires root and
 * forwards only record index zero (graphics) or two (memory); it never accepts an arbitrary RM id. */
uint32_t nvoc_macos_rm_session_clock_offset(nvoc_rm_session *session, uint32_t index, uint32_t write,
    void *block, size_t size, char *error, size_t cap);

/* Locked clocks: the caller builds the whole 648-byte block for both
 * directions, so the native side only chooses the id (0x20809014 query,
 * 0x2080d041 write) and guards root. SET echoes the snapped values back into
 * the same buffer, which is what the readback compares against. */
#define NVOC_RM_LOCK_BLOCK_BYTES 648
uint32_t nvoc_macos_rm_session_locked_clocks(nvoc_rm_session *session, uint32_t write,
    void *block, size_t size, char *error, size_t cap);

/* Clock-domain enumeration: fills 4-byte mask, 32 kHz words, 32 statuses
 * (260 bytes) from 0x20809001 mode 0 plus one 0x20809006 version-1 read per
 * reported bit. Bits the driver refuses keep a non-zero status. */
#define NVOC_RM_DOMAIN_BLOCK_BYTES 260
#define NVOC_RM_DOMAIN_STATUS_OFFSET 132
uint32_t nvoc_macos_rm_session_clock_domains(nvoc_rm_session *session, void *out, size_t size,
                                             char *error, size_t cap);

/* kind: info/status/current/default GET, or exact cooler0 SET (0..4). */
uint32_t nvoc_macos_rm_session_fan(nvoc_rm_session *session, uint32_t kind,
    void *block, size_t size, char *error, size_t cap);
/* Fixed board power GET-info(0,20000B), GET-client(1,12B), SET-client(2,12B).
 * SET admits only board policy1/client0xfe and the validated 70..100W window. */
uint32_t nvoc_macos_rm_session_power(nvoc_rm_session *session, uint32_t kind,
    void *block, size_t size, char *error, size_t cap);

/* Fixed RATED_TDP GET(0)/SET(1), 12B {client, action, vPstate}. Client 1..4
 * (RM=0 is internal) and action 0..4 are checked here as well as in Rust. */
uint32_t nvoc_macos_rm_session_rated_tdp(nvoc_rm_session *session, uint32_t kind,
    void *block, size_t size, char *error, size_t cap);

/* Fixed PERF power-state GET(0)/SET(1), 4B {powerState}. The value is the
 * published AC(0)/BATTERY(1) source, checked here as well as in Rust; SET
 * requires root. The reply echoes the stored dword, so it is the readback. */
uint32_t nvoc_macos_rm_session_power_state(nvoc_rm_session *session, uint32_t kind,
    void *block, size_t size, char *error, size_t cap);

/* Fixed thermal policy GET-info, GET-control(1,1440B), SET(2,1440B).
 * Rust validates the acoustic target and the complete driver-normalized reply. */
uint32_t nvoc_macos_rm_session_thermal_policy(nvoc_rm_session *session, uint32_t kind,
    void *block, size_t size, char *error, size_t cap);

/* The public thermal read face: NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2
 * (0x20800513), an in-tree header-defined opcode batch whose read opcodes
 * (targets/sensors available, a sensor's reading, its reading range, its target
 * index, its provider index, that target's type) are declared to return NV_OK on
 * this kext and were measured to do so. Only reads are issued; nothing here
 * writes, so it needs no privilege and no gate.
 *
 * The batch runs with EXECUTE_FLAGS_IGNORE_FAIL so one unsupported opcode does
 * not hide the rest, and every instruction reports whether it ran and with what
 * status: both travel here unchanged, because a range that was never written
 * must not be read as a range of zero. Naming (which target type a sensor
 * belongs to) and plausibility stay in Rust.
 *
 * Caller must have serialized the transport. The payload is valid only on a
 * zero return: a wedged GSP answers 0x62 here as it does for every other
 * control, and that refusal must never be rendered as a temperature or a
 * bound. */
#define NVOC_RM_THERMAL_SENSORS_MAX 8
#define NVOC_RM_THERMAL_TARGET_NONE 0u
#define NVOC_RM_THERMAL_TARGET_GPU 1u
#define NVOC_RM_THERMAL_TARGET_MEMORY 2u
#define NVOC_RM_THERMAL_TARGET_POWER_SUPPLY 4u
#define NVOC_RM_THERMAL_TARGET_BOARD 8u
#define NVOC_RM_THERMAL_TARGET_UNKNOWN 0xffffffffu

typedef struct nvoc_rm_thermal_sensor {
    uint32_t executed;       /* 1 when that instruction ran */
    uint32_t result;         /* that instruction's own NV_STATUS */
    uint32_t target_index;   /* 0xffffffff when the target read did not run */
    uint32_t provider_index; /* 0xffffffff when the provider read did not run */
    uint32_t target_type;    /* NVOC_RM_THERMAL_TARGET_* looked up by target index */
    uint32_t target_known;   /* 1 when that lookup had a clean target-type read */
    int32_t reading;         /* raw driver unit, as published */
    int32_t range_min;
    int32_t range_max;
    uint32_t range_known; /* 1 when the range instruction ran clean */
} nvoc_rm_thermal_sensor;

typedef struct nvoc_rm_thermal {
    uint32_t status; /* pass 1 status: counts are valid only when it is 0 */
    uint32_t target_count;
    uint32_t sensor_count;
    uint32_t reported_sensors;   /* how many per-sensor batches were filled */
    uint32_t target_types_known; /* leading target_types entries that are known */
    /* Target types are read once per target index, before the sensors, because
     * the type instruction takes the index the sensor read has not returned
     * yet: a sensor's own type is looked up here afterwards. */
    uint32_t target_types[NVOC_RM_THERMAL_SENSORS_MAX];
    nvoc_rm_thermal_sensor sensors[NVOC_RM_THERMAL_SENSORS_MAX];
} nvoc_rm_thermal;

/* The per-target batch must fit the control's own instruction list; the .c file
 * asserts the cap against NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION_MAX_COUNT,
 * which only that translation unit includes. */
_Static_assert(sizeof(nvoc_rm_thermal_sensor) == 40, "nvoc_rm_thermal_sensor ABI drift");
_Static_assert(sizeof(nvoc_rm_thermal) == 372, "nvoc_rm_thermal ABI drift");
_Static_assert(offsetof(nvoc_rm_thermal, target_types) == 20, "thermal.target_types offset drift");
_Static_assert(offsetof(nvoc_rm_thermal, sensors) == 52, "thermal.sensors offset drift");
_Static_assert(offsetof(nvoc_rm_thermal_sensor, reading) == 24,
               "thermal_sensor.reading offset drift");

uint32_t nvoc_macos_rm_session_thermal(nvoc_rm_session *session, nvoc_rm_thermal *out, size_t size,
                                       char *error, size_t cap);

/* Release the mapping, the RM objects and both fds. Returns non-zero (and a
 * reason) when the mapping could not be unmapped; the resource is then
 * deliberately left to connection teardown instead of being freed under a
 * live mapping. Always frees the session itself. */
uint32_t nvoc_macos_rm_session_close(nvoc_rm_session *session, char *error, size_t cap);

/* Layout fingerprint for the cross-language ABI test (Rust mirrors it in
 * core/src/macos_rm.rs::abi_layout). Slots, in order:
 *   0  sizeof(nvoc_rm_snapshot)             1  sizeof(nvoc_rm_sample)
 *   2  sizeof(nvoc_rm_telemetry)            3  offsetof(snapshot, telemetry)
 *   4  offsetof(snapshot, driver)           5  offsetof(telemetry, sample)
 *   6  offsetof(sample, timestamps)         7  offsetof(sample, temperature_fx256)
 *   8  offsetof(sample, clk_target_mhz)     9  offsetof(sample, clk_actual_khz)
 *  10  offsetof(sample, power_mw)          11  offsetof(sample, fan_rpm)
 *  12  offsetof(sample, engine_utilization_pct100)
 *  13  offsetof(sample, pci_gen_info)      14  offsetof(sample, pstate)
 *  15  offsetof(sample, memory_used_kb)    16  NVOC_RM_ABI_VERSION
 *  17  NVOC_RM_RUSD_FIELDS                 18  NVOC_RM_FIELD_OK
 *  19  NVOC_RM_FIELD_UNSTABLE              20  NVOC_RM_FIELD_UNAVAILABLE
 *  21  NVOC_RM_FIELD_NOT_FRESH             22  NVOC_RM_CLK_DOMAINS
 *  23  NVOC_RM_TEMP_SENSORS                24  NVOC_RM_FAN_MAX_COOLERS
 *  25  NVOC_RM_ENGINES                     26  NVOC_RM_POLL_MASK
 *  27-40  NVOC_RM_RUSD_CLOCK .. NVOC_RM_RUSD_PROC_UTIL (14 section indices)
 *  41  sizeof(nvoc_rm_thermal)             42  sizeof(nvoc_rm_thermal_sensor)
 *  43  offsetof(thermal, sensors)
 * Fills up to `count` slots and returns the number this build has. */
#define NVOC_RM_LAYOUT_SLOTS 44
size_t nvoc_macos_rm_abi_layout(uint64_t *out, size_t count);

#endif
