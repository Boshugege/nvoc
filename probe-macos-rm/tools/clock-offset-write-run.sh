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
# tool), the driver's own anchor read answers NV_OK (never write into a wedge,
# where every reply collapses to 0x62 and a rejection would be unreadable), and
# the full profile builds (the idle build cannot send a stage-3 write at all).
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
