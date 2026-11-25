/*
 * stress-macos — Metal compute workload with a correctness verifier.
 *
 * The macOS analogue of the CUDA/OpenCL stressors: it runs real compute on the
 * Metal device NullMoth exposes (NVMTL over the dGPU), and results are checked
 * against a CPU reference before an iteration counts as passed. A run that only
 * proves "the GPU did not crash" is not a result.
 *
 * Kernel: a bit-exact integer mix (xorshift64* + splitmix finalizer) over a
 * shared-storage buffer. Integer arithmetic has one correct answer, so a
 * mismatch is unambiguous — there is no floating-point tolerance to hide in.
 *
 * Modes:
 *   --verify               one dispatch, checked, plus a self-test proving the
 *                          comparator catches a corrupted value
 *   --stress <seconds>     repeat verified dispatches until the deadline or
 *                          SIGINT; the first mismatch stops the run
 *   --gpu <name-substring> pick a device by name (default: system default)
 *   --elements <n>         elements (== threads) per dispatch (default 65536)
 *   --iters <n>            inner-loop iterations per element  (default 2000)
 *   --verify-sample <n>    verify at most n evenly spread elements per dispatch
 *                          (default 0 = verify every element)
 *   --stream <seconds>     run the memory-bound stream kernel until the deadline
 *                          (a load generator, not a verifier -- see --stream-*
 *                          below and the note it prints)
 *   --json                 accepted; the JSON summary is always printed
 *   --inject-mismatch      test-only: corrupt one element after the dispatch so
 *                          the verifier must fail (proves detection end to end,
 *                          never used for a real run)
 *
 * Exit codes: 0 verified, 1 verification failed, 2 argument/environment error.
 */
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile sig_atomic_t g_stop;

static void on_signal(int sig)
{
    (void)sig;
    g_stop = 1;
}

static const char *kSrc =
    "#include <metal_stdlib>\n"
    "using namespace metal;\n"
    "static inline ulong mix64(ulong x) {\n"
    "  x ^= x >> 30; x *= 0xbf58476d1ce4e5b9UL;\n"
    "  x ^= x >> 27; x *= 0x94d049bb133111ebUL;\n"
    "  return x ^ (x >> 31);\n"
    "}\n"
    "kernel void nvoc_mix(device ulong *out [[buffer(0)]],\n"
    "                     constant ulong &seed [[buffer(1)]],\n"
    "                     constant uint &iters [[buffer(2)]],\n"
    "                     uint gid [[thread_position_in_grid]]) {\n"
    "  ulong x = mix64(seed + ulong(gid));\n"
    "  for (uint i = 0; i < iters; ++i) { x = mix64(x + ulong(gid) + i); }\n"
    "  out[gid] = x;\n"
    "}\n";

/* The stream kernel: the same integer mix, but every thread walks the whole
 * buffer with a grid stride, so each pass reads and writes the entire buffer.
 * The mix loop above keeps everything in registers and draws about 14 W on the
 * test card; this one is bound by memory traffic, which is what a power wall
 * needs to be reached at all.
 *
 * It is a load generator and says so: there is no CPU reference for a buffer
 * that every thread mutates, so --stream reports dispatch count and achieved
 * bandwidth and claims no correctness. Correctness comes from --verify and
 * --stress, which compare against a CPU reference element by element. */
static const char *kStreamSrc =
    "#include <metal_stdlib>\n"
    "using namespace metal;\n"
    "static inline ulong mix64(ulong x) {\n"
    "  x ^= x >> 30; x *= 0xbf58476d1ce4e5b9UL;\n"
    "  x ^= x >> 27; x *= 0x94d049bb133111ebUL;\n"
    "  return x ^ (x >> 31);\n"
    "}\n"
    "kernel void nvoc_stream(device ulong *buf [[buffer(0)]],\n"
    "                        constant ulong &seed [[buffer(1)]],\n"
    "                        constant uint &count [[buffer(2)]],\n"
    "                        constant uint &passes [[buffer(3)]],\n"
    "                        constant uint &grid [[buffer(4)]],\n"
    "                        uint gid [[thread_position_in_grid]]) {\n"
    "  ulong x = mix64(seed + ulong(gid));\n"
    "  for (uint p = 0; p < passes; ++p) {\n"
    "    for (uint i = gid; i < count; i += grid) {\n"
    "      x = mix64(x ^ buf[i] ^ ulong(p));\n"
    "      buf[i] = x;\n"
    "    }\n"
    "  }\n"
    "  buf[gid] = x;\n"
    "}\n";

/* CPU reference: the same definition, so equality is exact. */
static uint64_t mix64_cpu(uint64_t x)
{
    x ^= x >> 30;
    x *= 0xbf58476d1ce4e5b9ULL;
    x ^= x >> 27;
    x *= 0x94d049bb133111ebULL;
    return x ^ (x >> 31);
}

static uint64_t reference_one(uint64_t seed, uint64_t gid, uint32_t iters)
{
    uint64_t x = mix64_cpu(seed + gid);
    for (uint32_t i = 0; i < iters; ++i) x = mix64_cpu(x + gid + i);
    return x;
}

struct mismatch {
    uint64_t count;
    uint64_t index;
    uint64_t got;
    uint64_t want;
};

static struct mismatch compare_one(const uint64_t *gpu, uint64_t gid,
                                   uint64_t seed, uint32_t iters)
{
    uint64_t want = reference_one(seed, gid, iters);
    struct mismatch m = {0, gid, gpu[gid], want};
    if (gpu[gid] != want) m.count = 1;
    return m;
}

static void note_mismatch(struct mismatch *acc, const struct mismatch *m)
{
    if (!m->count) return;
    if (!acc->count) {
        acc->index = m->index;
        acc->got = m->got;
        acc->want = m->want;
    }
    acc->count += m->count;
}

static uint64_t now_ns(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static int parse_u64(const char *raw, uint64_t max, uint64_t *out)
{
    if (!raw || !*raw || *raw < '0' || *raw > '9') return -1;
    char *end = NULL;
    unsigned long long v = strtoull(raw, &end, 10);
    if (*end || v > max) return -1;
    *out = v;
    return 0;
}

/* Runs the memory-bound kernel until the deadline. Returns a process exit code:
 * 0 when dispatches completed, 1 when the device reported a failed command
 * buffer. Nothing here claims the data is right -- see the note above. */
static int run_stream(id<MTLDevice> dev, const char *dev_name, double seconds,
                      uint64_t stream_elements, uint64_t stream_threads,
                      uint64_t stream_passes)
{
    NSError *err = nil;
    id<MTLLibrary> lib = [dev newLibraryWithSource:[NSString stringWithUTF8String:kStreamSrc]
                                           options:nil
                                             error:&err];
    if (!lib) {
        fprintf(stderr, "stream library error: %s\n", err.description.UTF8String);
        return 2;
    }
    id<MTLFunction> fn = [lib newFunctionWithName:@"nvoc_stream"];
    id<MTLComputePipelineState> pipe = [dev newComputePipelineStateWithFunction:fn error:&err];
    if (!pipe) {
        fprintf(stderr, "stream pipeline error: %s\n", err.description.UTF8String);
        return 2;
    }
    id<MTLCommandQueue> queue = [dev newCommandQueue];

    const uint64_t bytes = stream_elements * sizeof(uint64_t);
    id<MTLBuffer> buf = [dev newBufferWithLength:bytes options:MTLResourceStorageModeShared];
    if (!buf) {
        fprintf(stderr, "stream buffer allocation failed (%llu bytes)\n",
                (unsigned long long)bytes);
        return 2;
    }
    uint64_t *host = buf.contents;
    for (uint64_t i = 0; i < stream_elements; ++i) host[i] = (i * 0x9e3779b97f4a7c15ULL) ^ 0xa5a5a5a5ULL;
    const uint64_t first_before = host[0];

    signal(SIGINT, on_signal);
    signal(SIGTERM, on_signal);

    MTLSize grid = MTLSizeMake(stream_threads, 1, 1);
    MTLSize group = MTLSizeMake(64, 1, 1);
    const uint32_t count = (uint32_t)stream_elements;
    const uint32_t passes = (uint32_t)stream_passes;
    const uint32_t grid_u32 = (uint32_t)stream_threads;
    const uint64_t per_dispatch_bytes = bytes * stream_passes * 2;

    const uint64_t deadline = now_ns() + (uint64_t)(seconds * 1e9);
    uint64_t launched = 0, failed = 0;
    const uint64_t t0 = now_ns();
    enum { DEPTH = 3 };
    id<MTLCommandBuffer> cbs[DEPTH];
    for (int i = 0; i < DEPTH; ++i) cbs[i] = nil;

    while (!g_stop && now_ns() < deadline && !failed) {
        const int slot = (int)(launched % DEPTH);
        if (cbs[slot]) {
            [cbs[slot] waitUntilCompleted];
            if (cbs[slot].status != MTLCommandBufferStatusCompleted) {
                fprintf(stderr, "stream command buffer failed: %s\n",
                        cbs[slot].error.description.UTF8String);
                ++failed;
                cbs[slot] = nil;
                break;
            }
            cbs[slot] = nil;
        }
        uint64_t seed = 0x9e3779b97f4a7c15ULL * (launched + 1);
        id<MTLCommandBuffer> cb = [queue commandBuffer];
        id<MTLComputeCommandEncoder> enc = [cb computeCommandEncoder];
        [enc setComputePipelineState:pipe];
        [enc setBuffer:buf offset:0 atIndex:0];
        [enc setBytes:&seed length:sizeof(seed) atIndex:1];
        [enc setBytes:&count length:sizeof(count) atIndex:2];
        [enc setBytes:&passes length:sizeof(passes) atIndex:3];
        [enc setBytes:&grid_u32 length:sizeof(grid_u32) atIndex:4];
        [enc dispatchThreads:grid threadsPerThreadgroup:group];
        [enc endEncoding];
        [cb commit];
        cbs[slot] = cb;
        ++launched;
    }
    for (int i = 0; i < DEPTH; ++i) {
        if (!cbs[i]) continue;
        [cbs[i] waitUntilCompleted];
        if (cbs[i].status != MTLCommandBufferStatusCompleted) {
            fprintf(stderr, "stream command buffer failed: %s\n",
                    cbs[i].error.description.UTF8String);
            ++failed;
        }
        cbs[i] = nil;
    }

    const uint64_t elapsed = now_ns() - t0;
    /* Cheap evidence that the kernel ran at all: the first element it owns must
     * have changed. It is not a correctness check and does not pretend to be. */
    const int state_changed = host[0] != first_before;
    const double seconds_elapsed = (double)elapsed / 1e9;
    const double gigabytes = (double)per_dispatch_bytes * (double)(launched - failed) / 1e9;
    printf("{\"schema\":\"nvoc/macos-metal-stream/1\",\"device\":\"%s\","
           "\"backend\":\"metal\",\"mode\":\"stream\","
           "\"elements\":%llu,\"threads\":%llu,\"passes\":%llu,"
           "\"bytes_per_dispatch\":%llu,\"dispatches\":%llu,\"failed\":%llu,"
           "\"elapsed_ms\":%llu,\"gigabytes_moved\":%.3f,"
           "\"gigabytes_per_second\":%.3f,\"state_changed\":%s,"
           "\"verification\":\"none\",\"stopped_by_signal\":%s}\n",
           dev_name,
           (unsigned long long)stream_elements, (unsigned long long)stream_threads,
           (unsigned long long)stream_passes, (unsigned long long)per_dispatch_bytes,
           (unsigned long long)launched, (unsigned long long)failed,
           (unsigned long long)(elapsed / 1000000), gigabytes,
           seconds_elapsed > 0.0 ? gigabytes / seconds_elapsed : 0.0,
           state_changed ? "true" : "false", g_stop ? "true" : "false");
    fflush(stdout);
    return failed ? 1 : 0;
}

int main(int argc, char **argv)
{
    @autoreleasepool {
        double stress_seconds = 0.0;
        int want_verify = 0;
        const char *gpu_filter = NULL;
        uint64_t elements = 65536;
        uint64_t iters = 2000;
        uint64_t verify_sample = 0;
        int inject_mismatch = 0;
        double stream_seconds = 0.0;
        uint64_t stream_elements = 4194304;
        uint64_t stream_threads = 8192;
        uint64_t stream_passes = 8;

        for (int i = 1; i < argc; i++) {
            if (!strcmp(argv[i], "--verify")) want_verify = 1;
            else if (!strcmp(argv[i], "--stress") && i + 1 < argc) {
                stress_seconds = atof(argv[++i]);
                if (stress_seconds <= 0.0) { fprintf(stderr, "bad --stress\n"); return 2; }
            } else if (!strcmp(argv[i], "--gpu") && i + 1 < argc) gpu_filter = argv[++i];
            else if (!strcmp(argv[i], "--elements") && i + 1 < argc) {
                if (parse_u64(argv[++i], 1u << 26, &elements) || !elements) {
                    fprintf(stderr, "bad --elements\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--iters") && i + 1 < argc) {
                if (parse_u64(argv[++i], 10000000, &iters)) {
                    fprintf(stderr, "bad --iters\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--verify-sample") && i + 1 < argc) {
                if (parse_u64(argv[++i], 1u << 26, &verify_sample)) {
                    fprintf(stderr, "bad --verify-sample\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--stream") && i + 1 < argc) {
                stream_seconds = atof(argv[++i]);
                if (stream_seconds <= 0.0) { fprintf(stderr, "bad --stream\n"); return 2; }
            } else if (!strcmp(argv[i], "--stream-elements") && i + 1 < argc) {
                if (parse_u64(argv[++i], 1u << 26, &stream_elements) || !stream_elements) {
                    fprintf(stderr, "bad --stream-elements\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--stream-threads") && i + 1 < argc) {
                if (parse_u64(argv[++i], 1u << 22, &stream_threads) || !stream_threads) {
                    fprintf(stderr, "bad --stream-threads\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--stream-passes") && i + 1 < argc) {
                if (parse_u64(argv[++i], 4096, &stream_passes) || !stream_passes) {
                    fprintf(stderr, "bad --stream-passes\n"); return 2;
                }
            } else if (!strcmp(argv[i], "--inject-mismatch")) inject_mismatch = 1;
            else if (!strcmp(argv[i], "--json")) {
                /* the summary line is always emitted */
            } else if (!strcmp(argv[i], "--help")) {
                printf("usage: metal_check (--verify | --stress <sec>) [--gpu <substr>] "
                       "[--elements N] [--iters N] [--verify-sample N]\n"
                       "       metal_check --stream <sec> [--gpu <substr>] "
                       "[--stream-elements N] [--stream-threads N] [--stream-passes N]\n");
                return 0;
            } else {
                fprintf(stderr, "unknown argument: %s\n", argv[i]);
                return 2;
            }
        }
        const int stream_only = stream_seconds > 0.0 && !want_verify && stress_seconds <= 0.0;
        if (!want_verify && stress_seconds <= 0.0 && stream_seconds <= 0.0) {
            fprintf(stderr, "one of --verify, --stress <sec> or --stream <sec> is required\n");
            return 2;
        }
        if (stream_seconds > 0.0 && (want_verify || stress_seconds > 0.0)) {
            fprintf(stderr, "--stream cannot be combined with --verify or --stress: the stream "
                            "kernel is a load generator with no reference to compare against\n");
            return 2;
        }
        if (verify_sample == 0 || verify_sample > elements) verify_sample = elements;
        const uint64_t sample_stride = elements / verify_sample;

        id<MTLDevice> dev = nil;
        if (gpu_filter) {
            NSArray<id<MTLDevice>> *all = MTLCopyAllDevices();
            for (id<MTLDevice> candidate in all) {
                NSString *name = [candidate name];
                if (name && [name rangeOfString:[NSString stringWithUTF8String:gpu_filter]].location != NSNotFound) {
                    dev = candidate;
                    break;
                }
            }
            if (!dev) {
                fprintf(stderr, "no Metal device matching %s\n", gpu_filter);
                return 2;
            }
        } else {
            dev = MTLCreateSystemDefaultDevice();
        }
        if (!dev) { fprintf(stderr, "no Metal device\n"); return 2; }
        if (stream_only) {
            return run_stream(dev, dev.name ? dev.name.UTF8String : "unknown", stream_seconds,
                              stream_elements, stream_threads, stream_passes);
        }

        NSError *err = nil;
        id<MTLLibrary> lib = [dev newLibraryWithSource:[NSString stringWithUTF8String:kSrc]
                                               options:nil
                                                 error:&err];
        if (!lib) { fprintf(stderr, "library error: %s\n", err.description.UTF8String); return 2; }
        id<MTLFunction> fn = [lib newFunctionWithName:@"nvoc_mix"];
        id<MTLComputePipelineState> pipe = [dev newComputePipelineStateWithFunction:fn error:&err];
        if (!pipe) { fprintf(stderr, "pipeline error: %s\n", err.description.UTF8String); return 2; }
        id<MTLCommandQueue> queue = [dev newCommandQueue];

        const uint64_t bytes = elements * sizeof(uint64_t);

        /* Two buffers in flight: dispatch N+1 is enqueued before N is verified,
         * so the CPU never idles the GPU the way a dispatch/verify/dispatch
         * loop would (worth ~10x on the duty cycle). Both buffers are verified
         * — nothing is skipped to buy the overlap. */
        enum { DEPTH = 2 };
        id<MTLBuffer> bufs[DEPTH];
        uint64_t seeds[DEPTH];
        id<MTLCommandBuffer> cbs[DEPTH];
        for (int i = 0; i < DEPTH; ++i) {
            bufs[i] = [dev newBufferWithLength:bytes options:MTLResourceStorageModeShared];
            if (!bufs[i]) {
                fprintf(stderr, "buffer allocation failed (%llu bytes)\n",
                        (unsigned long long)bytes);
                return 2;
            }
            cbs[i] = nil;
            seeds[i] = 0;
        }

        /* Self-test: a deliberately corrupted value must be rejected. */
        const uint64_t selftest_seed = 0x0123456789abcdefULL;
        uint64_t *selftest = malloc((size_t)bytes);
        if (!selftest) { fprintf(stderr, "host allocation failed\n"); return 2; }
        for (uint64_t i = 0; i < elements; ++i) {
            selftest[i] = reference_one(selftest_seed, i, (uint32_t)iters);
        }
        struct mismatch st = compare_one(selftest, elements / 2, selftest_seed, (uint32_t)iters);
        if (st.count) {
            fprintf(stderr, "verifier self-test failed: reference disagrees with itself\n");
            free(selftest);
            return 2;
        }
        uint64_t saved = selftest[elements / 2];
        selftest[elements / 2] ^= 1;
        st = compare_one(selftest, elements / 2, selftest_seed, (uint32_t)iters);
        selftest[elements / 2] = saved;
        if (st.count != 1) {
            fprintf(stderr, "verifier self-test failed: comparator missed a corrupted value\n");
            free(selftest);
            return 2;
        }
        free(selftest);

        signal(SIGINT, on_signal);
        signal(SIGTERM, on_signal);

        const char *dev_name = dev.name ? dev.name.UTF8String : "unknown";
        MTLSize grid = MTLSizeMake(elements, 1, 1);
        MTLSize group = MTLSizeMake(64, 1, 1);
        /* Verify mode runs exactly one dispatch; stress mode runs until the
         * deadline or a signal. `single_shot` keeps those two apart, so a
         * deadline of 0 can never be read as "loop forever". */
        const int single_shot = (want_verify && stress_seconds <= 0.0);
        const uint64_t deadline = stress_seconds > 0.0
            ? now_ns() + (uint64_t)(stress_seconds * 1e9)
            : 0;
        uint64_t launched = 0, verified = 0, failures = 0, checked_elements = 0;
        struct mismatch acc = {0, 0, 0, 0};
        uint64_t seed_state = 0x9e3779b97f4a7c15ULL;
        const uint64_t t0 = now_ns();

        /* Verifies one finished slot; returns non-zero on the first mismatch. */
        uint64_t *slot_data[DEPTH];
        for (int i = 0; i < DEPTH; ++i) slot_data[i] = bufs[i].contents;

        int done = 0;
        while (!done) {
            const int slot = (int)(launched % DEPTH);
            const int can_launch = !g_stop && (deadline == 0 || now_ns() < deadline);

            if (can_launch) {
                seed_state ^= 0xa0761d6478bd642fULL * (launched + 1);
                seeds[slot] = seed_state;
                id<MTLCommandBuffer> cb = [queue commandBuffer];
                id<MTLComputeCommandEncoder> enc = [cb computeCommandEncoder];
                [enc setComputePipelineState:pipe];
                [enc setBuffer:bufs[slot] offset:0 atIndex:0];
                [enc setBytes:&seeds[slot] length:sizeof(uint64_t) atIndex:1];
                uint32_t inner = (uint32_t)iters;
                [enc setBytes:&inner length:sizeof(inner) atIndex:2];
                [enc dispatchThreads:grid threadsPerThreadgroup:group];
                [enc endEncoding];
                [cb commit];
                cbs[slot] = cb;
                ++launched;
                if (single_shot) {
                    /* one dispatch only: fall through to the drain below */
                }
            } else {
                done = 1;
            }
            if (single_shot && launched >= 1) done = 1;

            /* Verify the oldest in-flight slot once it has completed. */
            if (launched > verified) {
                const int old = (int)(verified % DEPTH);
                id<MTLCommandBuffer> ocb = cbs[old];
                if (ocb) {
                    [ocb waitUntilCompleted];
                    if (ocb.status != MTLCommandBufferStatusCompleted) {
                        fprintf(stderr, "command buffer failed: %s\n",
                                ocb.error.description.UTF8String);
                        return 2;
                    }
                    cbs[old] = nil;
                    if (inject_mismatch) {
                        /* Test-only fault injection: the verifier must notice. */
                        slot_data[old][elements / 2] ^= 1;
                        inject_mismatch = 0;
                    }
                    const uint64_t *gpu = slot_data[old];
                    for (uint64_t n = 0; n < verify_sample; ++n) {
                        uint64_t gid = n * sample_stride;
                        if (gid >= elements) break;
                        struct mismatch m = compare_one(gpu, gid, seeds[old], (uint32_t)iters);
                        note_mismatch(&acc, &m);
                        ++checked_elements;
                    }
                    ++verified;
                    if (acc.count) {
                        ++failures;
                        break; /* never keep hammering a device that returned wrong data */
                    }
                }
            }
        }

        /* Drain whatever is still in flight. */
        while (!failures && verified < launched) {
            const int old = (int)(verified % DEPTH);
            id<MTLCommandBuffer> ocb = cbs[old];
            if (!ocb) { ++verified; continue; }
            [ocb waitUntilCompleted];
            if (ocb.status != MTLCommandBufferStatusCompleted) {
                fprintf(stderr, "command buffer failed: %s\n", ocb.error.description.UTF8String);
                return 2;
            }
            cbs[old] = nil;
            const uint64_t *gpu = slot_data[old];
            for (uint64_t n = 0; n < verify_sample; ++n) {
                uint64_t gid = n * sample_stride;
                if (gid >= elements) break;
                struct mismatch m = compare_one(gpu, gid, seeds[old], (uint32_t)iters);
                note_mismatch(&acc, &m);
                ++checked_elements;
            }
            ++verified;
            if (acc.count) ++failures;
        }

        const uint64_t elapsed = now_ns() - t0;
        printf("{\"schema\":\"nvoc/macos-metal-check/1\",\"device\":\"%s\",\"backend\":\"metal\","
               "\"mode\":\"%s\",\"elements\":%llu,\"iters_per_thread\":%llu,"
               "\"verify_sample\":%llu,\"verified_full_buffer\":%s,"
               "\"iterations\":%llu,\"verified\":%llu,\"failures\":%llu,"
               "\"checked_elements\":%llu,\"mismatched_elements\":%llu,"
               "\"first_bad_index\":%llu,\"first_bad_got\":%llu,\"first_bad_want\":%llu,"
               "\"elapsed_ms\":%llu,\"stopped_by_signal\":%s}\n",
               dev_name,
               want_verify && stress_seconds <= 0.0 ? "verify" : "stress",
               (unsigned long long)elements, (unsigned long long)iters,
               (unsigned long long)verify_sample,
               verify_sample == elements ? "true" : "false",
               (unsigned long long)launched, (unsigned long long)verified,
               (unsigned long long)failures, (unsigned long long)checked_elements,
               (unsigned long long)acc.count, (unsigned long long)acc.index,
               (unsigned long long)acc.got, (unsigned long long)acc.want,
               (unsigned long long)(elapsed / 1000000), g_stop ? "true" : "false");
        fflush(stdout);
        return failures ? 1 : 0;
    }
}
