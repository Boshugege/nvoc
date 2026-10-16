#ifndef NVOC_RUSD_READ_H
#define NVOC_RUSD_READ_H
#include "class/cl00de.h"
#include <stdatomic.h>
#include <string.h>

/* Non-polled fields use an even sequence number; polled fields must have
 * advanced since this session started observing them. Never expose old data
 * merely because allocation or mapping succeeded. */
static inline int nvoc_rusd_fresh(NvU64 timestamp, NvU64 baseline)
{
    if (!RUSD_SEQ_DATA_VALID(timestamp) || timestamp == RUSD_TIMESTAMP_WRITE_IN_PROGRESS)
        return 0;
    return timestamp >= RUSD_SEQ_START || !RUSD_SEQ_DATA_VALID(baseline) || timestamp > baseline;
}

/* Coherent copy with no advancement requirement: it only requires a
 * data-valid sequence and that the timestamp did not change while copying.
 * The continuous monitor uses this and tracks per-field advancement itself, so
 * a section that legitimately did not refresh between samples still yields its
 * last good value instead of a fabricated zero. */
static inline int nvoc_rusd_copy(const volatile NvU64 *timestamp, const void *field,
                                 void *dest, size_t size, NvU64 *observed)
{
    for (unsigned attempt = 0; attempt < 10; attempt++) {
        NvU64 before = *timestamp;
        atomic_thread_fence(memory_order_acquire);
        if (!RUSD_SEQ_DATA_VALID(before) || before == RUSD_TIMESTAMP_WRITE_IN_PROGRESS)
            continue;
        memcpy(dest, field, size);
        atomic_thread_fence(memory_order_acquire);
        if (before == *timestamp) {
            *observed = before;
            return 1;
        }
    }
    return 0;
}

static inline int nvoc_rusd_read(const volatile NvU64 *timestamp, const void *field,
                                 void *dest, size_t size, NvU64 baseline, NvU64 *observed)
{
    for (unsigned attempt = 0; attempt < 10; attempt++) {
        NvU64 before = *timestamp;
        atomic_thread_fence(memory_order_acquire);
        if (!nvoc_rusd_fresh(before, baseline)) continue;
        memcpy(dest, field, size);
        atomic_thread_fence(memory_order_acquire);
        if (before == *timestamp) {
            *observed = before;
            return 1;
        }
    }
    return 0;
}
#endif
