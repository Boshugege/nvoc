/* Compile-time contract test for the macOS RM C ABI.
 *
 * This translation unit never executes: it exists so the compiler proves, on
 * every build of the check list, that
 *   - the layout assertions in core/native/macos_rm_abi.h hold for this
 *     toolchain (the same numbers core/src/macos_rm.rs asserts with
 *     size_of!/offset_of!),
 *   - every entry point has the signature Rust declares in its
 *     `unsafe extern "C"` block, and
 *   - the field status codes, the section order and the fixed value-block
 *     sizes Rust matches on are the ones the header defines.
 *
 * macos_rm.c includes the same header, so a definition that drifts from the
 * declared prototype is a compile error there rather than a silent ABI
 * mismatch at run time. The ignored hardware test
 * (macos_rm_readonly.rs::native_and_rust_abi_layouts_agree) additionally
 * compares the runtime fingerprint from both languages.
 *
 * Build (part of the macOS check list):
 *   cc -std=c11 -Wall -Wextra -Werror -Icore/native -fsyntax-only \
 *      core/tests/macos_rm_abi.c
 */
#include "macos_rm_abi.h"

/* One typedef per entry point, written the way Rust declares it. If either
 * side changes an argument list, one of these assignments stops compiling. */
typedef uint32_t (*nvoc_snapshot_call)(nvoc_rm_snapshot *, size_t, uint32_t, char *, size_t);
typedef uint32_t (*nvoc_session_open_call)(nvoc_rm_session **, nvoc_rm_snapshot *, size_t,
                                           uint64_t *, size_t, char *, size_t);
typedef uint32_t (*nvoc_session_sample_call)(nvoc_rm_session *, nvoc_rm_sample *, size_t, char *,
                                             size_t);
typedef uint32_t (*nvoc_session_close_call)(nvoc_rm_session *, char *, size_t);
typedef size_t (*nvoc_abi_layout_call)(uint64_t *, size_t);

/* Not static: the point is the assignments, and this is never called. */
void nvoc_abi_type_check(void)
{
    nvoc_snapshot_call snapshot = nvoc_macos_rm_snapshot;
    nvoc_session_open_call session_open = nvoc_macos_rm_session_open;
    nvoc_session_sample_call session_sample = nvoc_macos_rm_session_sample;
    nvoc_session_close_call session_close = nvoc_macos_rm_session_close;
    nvoc_abi_layout_call abi_layout = nvoc_macos_rm_abi_layout;

    (void)snapshot;
    (void)session_open;
    (void)session_sample;
    (void)session_close;
    (void)abi_layout;
}

/* The status codes and the section order Rust indexes with. */
_Static_assert(NVOC_RM_FIELD_OK == 0, "field status: ok");
_Static_assert(NVOC_RM_FIELD_UNSTABLE == 1, "field status: unstable");
_Static_assert(NVOC_RM_FIELD_UNAVAILABLE == 2, "field status: unavailable");
_Static_assert(NVOC_RM_FIELD_NOT_FRESH == 3, "field status: not fresh");
_Static_assert(NVOC_RM_RUSD_CLOCK == 0 && NVOC_RM_RUSD_TEMP_GPU == 1 &&
                   NVOC_RM_RUSD_TEMP_MEMORY == 2 && NVOC_RM_RUSD_TEMP_BOARD == 3 &&
                   NVOC_RM_RUSD_TEMP_POWER_SUPPLY == 4 && NVOC_RM_RUSD_POWER == 5 &&
                   NVOC_RM_RUSD_POWER_LIMIT == 6 && NVOC_RM_RUSD_AVG_POWER == 7 &&
                   NVOC_RM_RUSD_POWER_POLICY == 8 && NVOC_RM_RUSD_THROTTLE == 9 &&
                   NVOC_RM_RUSD_FAN == 10 && NVOC_RM_RUSD_UTIL == 11 &&
                   NVOC_RM_RUSD_PCI == 12 && NVOC_RM_RUSD_PROC_UTIL == 13,
               "RUSD section order");
_Static_assert(NVOC_RM_RUSD_FIELDS == 14, "RUSD section count");
_Static_assert(NVOC_RM_CLK_DOMAINS == 4, "clock domain count");
_Static_assert(NVOC_RM_TEMP_SENSORS == 4, "temperature sensor count");
_Static_assert(NVOC_RM_FAN_MAX_COOLERS == 16, "fan cooler count");
_Static_assert(NVOC_RM_ENGINES == 6, "engine count");
_Static_assert(NVOC_RM_POLL_MASK == 0xFBu, "RUSD poll mask");
_Static_assert(NVOC_RM_ABI_VERSION == 3, "ABI version");
_Static_assert(NVOC_RM_LAYOUT_SLOTS == 41, "layout fingerprint slot count");
_Static_assert(sizeof(((nvoc_rm_sample *)0)->timestamps) == NVOC_RM_RUSD_FIELDS * 8,
               "sample timestamp array size");
_Static_assert(sizeof(((nvoc_rm_sample *)0)->fan_rpm) == NVOC_RM_FAN_MAX_COOLERS * 4,
               "sample fan array size");
