#!/bin/sh
# Gated clock-offset write runbook: one domain, one candidate dword, one step.
#
# Why this exists: gate G1 -- which of the twelve measured value dwords of the
# 0x2080901b record actually carries the offset -- is still open, because every
# value dword reads zero at stock and a zero cannot name its own field. Only a
# write (or a read at a non-zero offset) can decide it, and a write this project
# treats as expensive: it must be a single domain, a single minimal step, with a
# snapshot taken first and a proven restore afterwards.
#
# The probe already implements exactly that sequence
# (nvx_stage3_clk_offset_set: pre-record -> arm restore -> SET -> same-field
# readback -> restore -> verify, success requiring SET == NV_OK AND the named
# dword reading back orig+DELTA AND every other value dword byte-identical AND
# the restored block byte-identical). What it cannot do is choose the candidate
# for you, so this script refuses to run without one.
#
# Usage:
#   sh tools/clock-offset-write-run.sh <bit>:<off>:<delta_khz>
#
#   sh tools/clock-offset-write-run.sh 0:8:250    # core domain, candidate +8
#   sh tools/clock-offset-write-run.sh 0:16:250   # core domain, candidate +16
#
#   <bit>   index-space row (bit i selects record i), so 0 is the row whose
#           1<<id names the core/GPU domain and 2 the memory domain
#   <off>   record-relative field offset; must be one of the twelve measured
#           value dwords 8,12,16,20,24,28,32,36,40,44,52,56 -- the probe
#           rejects anything else rather than guessing
#   <delta> kHz, one minimal step, capped at +-1000 by the probe
#
# Refuses to write unless: exactly one candidate is given (this is not a sweep
# tool), the process is root (see below), the driver's own anchor read answers
# NV_OK (never write into a wedge, where every reply collapses to 0x62 and a
# rejection would be unreadable), and the full profile builds (the idle build
# cannot send a stage-3 write at all).
#
# Root is not politeness -- it is the precondition the write is refused on. The
# host forwarder rejects a control id whose bits carry (cmd & 0xC000) == 0xC000
# unless the calling client is at RS_PRIV_LEVEL_USER_ROOT, and on the corrected
# 1118-row table that test selects exactly the 92 write-half ids and nothing
# else: the write half and the privileged half are the same set. 0x2080d01c
# carries 0xC000, which is why an unprivileged client is refused with 0x1b
# before the GSP ever sees the request -- so without root the run would report a
# driver refusal that is really a process-privilege one. 0x2080901b carries
# 0x8000 and is not privileged, which is exactly why every read on this driver
# has worked as the login user.
#
# A live frequency change is NOT a criterion either way: NVK issues its own
# PERF_BOOST, so the verdict is the read-back and the restore, nothing else.
#
# Output: $OUT_ROOT/stage-clkoffset-write-<stamp>/ with the anchor log and the
# write log, so the run is reviewable after the fact.

set -eu

if [ $# -ne 1 ]; then
    echo "usage: sh tools/clock-offset-write-run.sh <bit>:<off>:<delta_khz>" >&2
    echo "  e.g. sh tools/clock-offset-write-run.sh 0:8:250" >&2
    exit 2
fi
cand=$1
case "$cand" in
    [0-9]*:[0-9]*:*|[0-9]*:[0-9]*:-*) ;;
    *) echo "candidate must look like BIT:OFF:DELTA (got '$cand')" >&2; exit 2 ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
out_root=${OUT_ROOT:-$HOME/Desktop/macOS-nvoc-develop/outputs}
ogkm=${OGKM:-$HOME/Desktop/macOS-nvoc-develop/reference/open-gpu-kernel-modules-610.57.04}

# 0. the privilege precondition, checked before anything else so that a refusal
#    here is never mistaken for the driver refusing the id.
if [ "$(id -u)" -ne 0 ]; then
    if sudo -n true 2>/dev/null; then
        echo "not root; re-executing under sudo -n (passwordless) -- the write" >&2
        echo "half is privileged, so an unprivileged SET would return 0x1b" >&2
        exec sudo -n OGKM="$ogkm" OUT_ROOT="$out_root" "$0" "$@"
    fi
    echo "not root, and sudo needs a password -- refusing to attempt the write." >&2
    echo "  The host forwarder rejects (cmd & 0xC000) == 0xC000 below root, and" >&2
    echo "  0x2080d01c carries those bits: an unprivileged SET comes back 0x1b" >&2
    echo "  before the GSP sees it, which would look like the driver refusing." >&2
    echo "  Rerun as: sudo OUT_ROOT=$out_root $0 $cand" >&2
    exit 4
fi

# 1. the write vehicle: the full profile, never the idle one.
if [ ! -x "$root/probe-macos-rm" ]; then
    echo "building probe-macos-rm (profile full; the write path is compiled out" >&2
    echo "of the idle build)" >&2
    (cd "$root" && PROFILE=full OGKM="$ogkm" sh build.sh)
fi
if [ ! -x "$root/probe-macos-rm" ]; then
    echo "no probe-macos-rm after build; build.sh reports failure only in its" >&2
    echo "output, so check for 'built probe-macos-rm (profile full)'" >&2
    exit 1
fi

stamp=$(date -u +%Y%m%dT%H%M%SZ)
bundle="$out_root/stage-clkoffset-write-$stamp"
mkdir -p "$bundle"
echo "bundle: $bundle"

# 2. preflight: the driver's own state check, before anything is written.
echo "== anchor (decides whether the GSP is able to answer at all) =="
"$root/probe-macos-rm" --anchor-read > "$bundle/anchor.txt" 2>&1 && arc=0 || arc=$?
echo "  rc=$arc -> $bundle/anchor.txt"
if grep -q "NV_ERR_RESET_REQUIRED" "$bundle/anchor.txt" || \
   ! grep -q "NV_OK" "$bundle/anchor.txt"; then
    echo "  !! the GSP is not answering normally; NOT writing." >&2
    echo "     A 0x62 below would make a rejected write unreadable -- reboot" >&2
    echo "     first, then rerun this script. Nothing was written this run." >&2
    exit 3
fi

# 3. the single gated write, with its own read-back and restore.
echo "== write: $cand (one domain, one field, one step) =="
"$root/probe-macos-rm" --write --clk-offset-set "$cand" \
    > "$bundle/write.txt" 2>&1 && wrc=0 || wrc=$?
echo "  rc=$wrc -> $bundle/write.txt"

# 4. the verdict, quoted rather than summarised.
echo "== verdict =="
grep -E "V3\.5[a-e]|clk-offset" "$bundle/write.txt" || \
    echo "  (no clk-offset lines: the write path did not run -- read the log)"

if grep -q "clk-offset-verdict" "$bundle/write.txt" && \
   grep -q "restore verified" "$bundle/write.txt"; then
    echo "== SET + read-back + restore all verified; block restored =="
else
    echo "== NOT verified: read $bundle/write.txt before retrying ==" >&2
fi
echo "note: a frequency change (or its absence) is not evidence; the lines above" 
echo "      are. Rerun the read runbook afterwards to confirm the blocks agree."
exit $wrc
