/* Shared C ABI for the read-only macOS RM backend.
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
 */
#ifndef NVOC_MACOS_RM_ABI_H
#define NVOC_MACOS_RM_ABI_H

#include <stddef.h>
#include <stdint.h>

#define NVOC_RM_ABI_VERSION 2u

/* Per-field status codes carried in nvoc_rm_sample.field_status. */
#define NVOC_RM_FIELD_OK 0u
/* The section's RUSD sequence or timestamp changed while it was copied: no
 * coherent value this round. Retrying later is reasonable. */
#define NVOC_RM_FIELD_UNSTABLE 1u
/* The section carries no valid timestamp: the driver does not publish it on
 * this platform right now (cl00de.h RUSD_TIMESTAMP_INVALID). Retrying will
 * not help until the driver starts polling that section. */
#define NVOC_RM_FIELD_UNAVAILABLE 2u

/* Polled RUSD sections, in the order the Rust side indexes them. */
#define NVOC_RM_RUSD_FIELDS 6
#define NVOC_RM_RUSD_CLOCK 0
#define NVOC_RM_RUSD_TEMP 1
#define NVOC_RM_RUSD_POWER 2
#define NVOC_RM_RUSD_LIMIT 3
#define NVOC_RM_RUSD_FAN 4
#define NVOC_RM_RUSD_UTIL 5

typedef struct {
    uint32_t status, valid, graphics_khz, memory_khz;
    int32_t temperature_fx256;
    uint32_t power_mw, limit_mw, fan_rpm, utilization_percent, reserved;
    uint64_t timestamps[NVOC_RM_RUSD_FIELDS];
} nvoc_rm_telemetry;

typedef struct {
    uint32_t abi_version, rm_gpu_id, domain, bus, slot, function;
    uint32_t vendor_id, device_id, subsystem_id, revision, board_id, class_count;
    uint64_t total_bytes, usable_bytes, bar1_bytes, l2_bytes;
    uint32_t pstate, memory_status, pstate_status, pci_status, bios_status, name_status, class_status;
    char driver[64], name[64], sku[16], project[16];
    nvoc_rm_telemetry telemetry;
} nvoc_rm_snapshot;

/* One coherent read of the mapped page. field_status[i] == NVOC_RM_FIELD_OK
 * means the value was copied under a stable, data-valid RUSD sequence;
 * nonzero means that section produced nothing usable this round and the value
 * field must be ignored. timestamps[i] is the section's lastModifiedTimestamp
 * as observed (polled sections carry a host realtime nanosecond stamp, others
 * a cl00de.h sequence number) and is 0 when unknown. */
typedef struct {
    uint32_t abi_version;
    uint32_t field_status[NVOC_RM_RUSD_FIELDS];
    uint32_t graphics_khz, memory_khz;
    int32_t temperature_fx256;
    uint32_t power_mw, limit_mw, fan_rpm, utilization_percent;
    uint32_t pstate, pstate_status;
    uint64_t timestamps[NVOC_RM_RUSD_FIELDS];
} nvoc_rm_sample;

/* Opaque: defined in macos_rm.c. Rust only ever holds a pointer. */
typedef struct nvoc_rm_session nvoc_rm_session;

_Static_assert(sizeof(nvoc_rm_telemetry) == 88, "nvoc_rm_telemetry ABI drift");
_Static_assert(sizeof(nvoc_rm_snapshot) == 360, "nvoc_rm_snapshot ABI drift");
_Static_assert(sizeof(nvoc_rm_sample) == 112, "nvoc_rm_sample ABI drift");

_Static_assert(offsetof(nvoc_rm_snapshot, telemetry) == 272, "snapshot.telemetry offset drift");
_Static_assert(offsetof(nvoc_rm_snapshot, driver) == 108, "snapshot.driver offset drift");
_Static_assert(offsetof(nvoc_rm_telemetry, timestamps) == 40, "telemetry.timestamps offset drift");
_Static_assert(offsetof(nvoc_rm_sample, graphics_khz) == 28, "sample.graphics_khz offset drift");
_Static_assert(offsetof(nvoc_rm_sample, pstate) == 56, "sample.pstate offset drift");
_Static_assert(offsetof(nvoc_rm_sample, timestamps) == 64, "sample.timestamps offset drift");

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

/* Release the mapping, the RM objects and both fds. Returns non-zero (and a
 * reason) when the mapping could not be unmapped; the resource is then
 * deliberately left to connection teardown instead of being freed under a
 * live mapping. Always frees the session itself. */
uint32_t nvoc_macos_rm_session_close(nvoc_rm_session *session, char *error, size_t cap);

/* Layout fingerprint for the cross-language ABI test (Rust mirrors it in
 * core/src/macos_rm.rs::abi_layout). Slots, in order:
 *   0  sizeof(nvoc_rm_snapshot)          1  sizeof(nvoc_rm_sample)
 *   2  sizeof(nvoc_rm_telemetry)         3  offsetof(snapshot, telemetry)
 *   4  offsetof(snapshot, driver)        5  offsetof(telemetry, timestamps)
 *   6  offsetof(telemetry, temperature_fx256)
 *   7  offsetof(sample, graphics_khz)    8  offsetof(sample, pstate)
 *   9  offsetof(sample, timestamps)     10  NVOC_RM_ABI_VERSION
 *  11  NVOC_RM_RUSD_FIELDS              12  NVOC_RM_FIELD_OK
 *  13  NVOC_RM_FIELD_UNSTABLE           14  NVOC_RM_FIELD_UNAVAILABLE
 *  15-20  NVOC_RM_RUSD_CLOCK/TEMP/POWER/LIMIT/FAN/UTIL
 * Fills up to `count` slots and returns the number this build has. */
#define NVOC_RM_LAYOUT_SLOTS 21
size_t nvoc_macos_rm_abi_layout(uint64_t *out, size_t count);

#endif
