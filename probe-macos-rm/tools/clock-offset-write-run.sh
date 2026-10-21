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
# Confirmed additive core case (GSP 610.57.04, TU116):
#   sh tools/clock-offset-write-run.sh 0:12:250
# proves storage/readback/restore but the MHz calculation truncates 250/1000.
#   sh tools/clock-offset-write-run.sh 0:12:15000
# was separately verified under a bounded NVK load: 1965 -> 1980 -> 1965 MHz.
# Record +8 selects units (0 = signed kHz at +12, 1 = i16 /4096 relative).
# It is not an enable bit. Do not use the former --clk-offset-arm switch.
#
# <bit> selects a directory index, NOT a public clock-domain bit; record 0
# names graphics and record 2 names memory on the measured TU116 directory.
# <off> must pass the probe's twelve-value-dword whitelist, even though the
# argument parser alone admits 0..2108. Delta is limited to +/-18750.
# Only +12 with selector 0 has additive-kHz evidence. Other offsets remain
# explicit raw protocol experiments, not established frequency controls.
# :bump changes the unidentified tail +60; no stamp semantic is established.
#
# Refuses to write unless: exactly one candidate is given (this is not a sweep
# tool), the probe can run as root (the operator's own root, or a passwordless
# rule for that one binary -- see below), the driver's own anchor read answers
# NV_OK (never write into a wedge, where every reply collapses to 0x62 and a
# rejection would be unreadable), and the full profile builds (the idle build
# cannot send a stage-3 write at all).
#
# The id is not a guess either. In the private halves a GET's SET twin sits one
# category byte over (the read id's category byte gains 0x40, which for a 0x80-0xBF
# byte is id | 0x4000) with its message id advanced by one, at an identical
# paramSize: 0x2080901b@2108 -> 0x2080d01c@2108, and 0x2080901f@19416 ->
# 0x2080d020@19416, both write rows the dump carries (the same-low-byte variant
# 0x2080d01b is absent). How often the table carries that twin is a measured
# number rather than an assumption: on the 889-row disassembly dump 56 of its 300
# read rows have the twin present, 41 of those at the same paramSize, and a read
# id whose partner exists only at another size is not the pair.
#
# Root is not politeness -- it is the precondition the write is refused on. The
# host forwarder rejects a control id whose bits carry (cmd & 0xC000) == 0xC000
# unless the calling client is at RS_PRIV_LEVEL_USER_ROOT, and that test selects
# exactly the table's write half and nothing else -- 92 of the 1118 rows of the
# recounted export, 89 of the 889 of this disassembly dump, with no read row
# caught either time: the write half and the privileged half are the same set.
# 0x2080d01c carries 0xC000, which is why an unprivileged client is refused with 0x1b
# before the GSP ever sees the request -- so without root the run would report a
# driver refusal that is really a process-privilege one. 0x2080901b carries
# 0x8000 and is not privileged, which is exactly why every read on this driver
# has worked as the login user.
#
# This runbook verifies storage/readback/restore only. Frequency effects need
# a controlled stock -> step -> restored comparison inside the SAME load:
# NVK's own PERF_BOOST makes a load-vs-idle frequency rise inconclusive.
#
# Output: $OUT_ROOT/stage-clkoffset-write-<stamp>/ with the anchor log and the
# write log, so the run is reviewable after the fact.

set -eu

if [ $# -ne 1 ]; then
    echo "usage: sh tools/clock-offset-write-run.sh <bit>:<off>:<delta_khz>" >&2
    echo "  e.g. sh tools/clock-offset-write-run.sh 0:12:250" >&2
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
#
#    The probe, not the script, needs the privilege: the bundle belongs to the
#    operator, so a per-binary passwordless rule is enough and `sudo -n true` is
#    the wrong question -- it asks about /usr/bin/true and fails under exactly the
#    rule that makes this run possible.  `sudo -n -l <cmd>` asks about the command
#    that matters and executes nothing.
can_sudo_n() {
    sudo -n -l "$1" >/dev/null 2>&1
}
sudo_cmd=
if [ "$(id -u)" -ne 0 ]; then
    if can_sudo_n "$root/probe-macos-rm"; then
        sudo_cmd="sudo -n"
        echo "not root; the probe runs under sudo -n (the bundle stays the" >&2
        echo "operator's, not root's). The write half of the private table is" >&2
        echo "privileged and an unprivileged SET would come back 0x1b" >&2
    else
        echo "not root, and sudo will not run the probe without a password --" >&2
        echo "refusing to attempt the write." >&2
        echo "  The host forwarder rejects (cmd & 0xC000) == 0xC000 below root," >&2
        echo "  and 0x2080d01c carries those bits: an unprivileged SET comes back" >&2
        echo "  0x1b before the GSP sees it, which would look like the driver" >&2
        echo "  refusing the field instead of the process." >&2
        echo "  Either rerun as: sudo OGKM=$ogkm OUT_ROOT=$out_root $0 $cand" >&2
        echo "  or authorise just these two binaries, once -- paste this block" >&2
        echo "  as it stands: a here-document's terminator has to start at the" >&2
        echo "  beginning of its line, so the indentation above does not carry" >&2
        echo "  into the block:" >&2
        echo "sudo tee /etc/sudoers.d/nvoc-probe >/dev/null <<EOF" >&2
        echo "mac ALL=(root) NOPASSWD: $root/probe-macos-rm, $root/probe-macos-rm-idle" >&2
        echo "EOF" >&2
        echo "sudo chmod 440 /etc/sudoers.d/nvoc-probe && sudo visudo -c" >&2
        echo "sudo -n -l $root/probe-macos-rm && echo readable-here" >&2
        exit 4
    fi
fi

# 1. the write vehicle: the full profile, never the idle one.
needs_build=no
if [ ! -x "$root/probe-macos-rm" ]; then
    needs_build=yes
else
    for source in "$root"/src/*.[ch] "$root/build.sh"; do
        if [ "$source" -nt "$root/probe-macos-rm" ]; then needs_build=yes; fi
    done
fi
if [ "$needs_build" = yes ]; then
    echo "building probe-macos-rm (profile full; the write path is compiled out" >&2
    echo "of the idle build)" >&2
    (cd "$root" && PROFILE=full OGKM="$ogkm" sh build.sh)
fi
if [ ! -x "$root/probe-macos-rm" ]; then
    echo "no probe-macos-rm after build; build.sh reports failure only in its" >&2
    echo "output, so check for 'built probe-macos-rm (profile full)'" >&2
    exit 1
fi

for source in "$root"/src/*.[ch] "$root/build.sh"; do
    if [ "$source" -nt "$root/probe-macos-rm" ]; then
        echo "probe-macos-rm is still older than $source after build; refusing to write" >&2
        exit 1
    fi
done

stamp=$(date -u +%Y%m%dT%H%M%SZ)
bundle="$out_root/stage-clkoffset-write-$stamp"
mkdir -p "$bundle"
echo "bundle: $bundle"

# The candidate, the revision and the binary that will carry the write go into
# the bundle before the write: a gated run happens once, and afterwards
# "which candidate was that?" must be answerable from the artifact.  The vehicle
# here is the full-profile probe, not the idle one, so it is named explicitly.
if command -v python3 >/dev/null 2>&1; then
    python3 "$root/tools/record-run-provenance.py" \
        --bundle "$bundle" \
        --label "gated clock-offset write (single domain, one field, one step)" \
        --binary "$root/probe-macos-rm" \
        --source "$root" \
        --ogkm "$ogkm" \
        --case "$cand (bit:off:delta_khz)" \
        --id "0x2080d01c:2108 (SET twin of the 0x2080901b control GET)" \
        --preregistered "success = SET answers NV_OK AND the named dword reads back orig+delta AND every other value dword is byte-identical AND the restored block is byte-identical" \
        --preregistered "a live frequency change is not a criterion, in either direction (NVK issues its own PERF_BOOST)" \
        --preregistered "root is a precondition, not politeness: 0x2080d01c carries (cmd & 0xC000) == 0xC000 and is refused with 0x1b below root" \
        || echo "  (provenance stamp failed; the logs are still the artifact)" >&2
fi

stamp_sums() {
    # Same digest pass as the read runbook, so a quoted byte can be checked from
    # the artifact alone.
    if command -v python3 >/dev/null 2>&1; then
        python3 "$root/tools/record-run-provenance.py" --bundle "$bundle" --sums \
            || echo "  (digest pass failed; the logs are still the artifact)" >&2
    else
        (cd "$bundle" && shasum -a 256 ./*.txt > SHA256SUMS 2>/dev/null || true)
    fi
}

finish() {
    # The exit status goes into the bundle: "the driver refused before the write"
    # and "the write ran and was not verified" must not look the same to a reader.
    want=$1
    printf 'write run exit %s at %s\n' "$want" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        >> "$bundle/README.txt"
    stamp_sums
    exit "$want"
}

# 2. preflight: the driver's own state check, before anything is written.
echo "== anchor (decides whether the GSP is able to answer at all) =="
$sudo_cmd "$root/probe-macos-rm" --anchor-read > "$bundle/anchor.txt" 2>&1 && arc=0 || arc=$?
echo "  rc=$arc -> $bundle/anchor.txt"
if [ "$arc" -ne 0 ] || \
   ! grep -q "forward ALIVE and healthy" "$bundle/anchor.txt"; then
    echo "  !! the GSP is not answering normally; NOT writing." >&2
    echo "     A 0x62 below would make a rejected write unreadable -- reboot" >&2
    echo "     first, then rerun this script. Nothing was written this run." >&2
    # The refusal is an artifact too: it is the record that the GSP, not the
    # candidate, is why no write happened.
    finish 3
fi

# 3. the single gated write, with its own read-back and restore.
echo "== write: $cand (one domain, one field, one step) =="
$sudo_cmd "$root/probe-macos-rm" --write --clk-offset-set "$cand" \
    > "$bundle/write.txt" 2>&1 && wrc=0 || wrc=$?
echo "  rc=$wrc -> $bundle/write.txt"

# 4. the verdict, quoted rather than summarised.
echo "== verdict =="
grep -E "V3\.5[a-f]|clk-offset" "$bundle/write.txt" || \
    echo "  (no clk-offset lines: the write path did not run -- read the log)"

# The success claim needs all three parts, not just the restore. V3.5d can say
# the block came back while V3.5e rejects the write for a read-back that did not
# move, and reporting "all verified" then would be exactly the mistake this
# exercise exists to avoid. The fragments below are quoted from the probe's own
# format strings, so they do not depend on how the overall verdict token renders.
verified=no
if grep -q "clk-offset-verdict" "$bundle/write.txt" && \
   grep -q "SET 0x0, readback 0x0" "$bundle/write.txt" &&
   grep -q "same-field yes" "$bundle/write.txt" && \
   grep -q "others-unchanged yes" "$bundle/write.txt" && \
   grep -q "restore verified" "$bundle/write.txt"; then
    verified=yes
fi

# The restore is a separate claim from the write, and its failure is the one
# state a user must not have to infer from two log lines: the device may still
# carry the step. Both halves are read out of the log (pre-step value, post-SET
# read) so the corrective step is printed as a number, not as advice to think.
restore_bad=no
if grep -q "RESTORE NOT VERIFIED" "$bundle/write.txt"; then
    restore_bad=yes
fi

if [ "$verified" = yes ] && [ "$restore_bad" = no ]; then
    echo "== SET + read-back + restore all verified; block restored =="
else
    echo "== NOT verified: read $bundle/write.txt before retrying ==" >&2
fi

if [ "$restore_bad" = yes ]; then
    echo ""
    echo "!! RESTORE NOT VERIFIED -- THE DEVICE MAY STILL CARRY THE STEP." >&2
    grep -h "clk-offset-restore\|clk-offset-pre-record\|clk-offset-readback" \
        "$bundle/write.txt" | sed 's/^/   /' >&2
    # The step to print comes from the probe's own alarm when there is one: it
    # knows whether the read-back was usable, and when it was not (the GET itself
    # failed, as it does in the reset-required state) its number is the inverse of
    # the step that was attempted. The first real gated run showed why the
    # recomputation below must not be trusted first: the failed GET left the
    # 0xffffffff sentinel in the log, and `orig - (-1)` printed a corrective step
    # of 1 kHz for a step that was 250 kHz.
    corr=$(python3 - "$bundle/write.txt" <<'PY' 2>/dev/null || true
import re, sys
text = open(sys.argv[1]).read()
alarm = re.search(r"clk-offset-restore-alarm.*?field \+(\d+).*?"
                  r"the corrective step is ([+-]?\d+) kHz", text, re.S)
if alarm:
    print("%s %s" % (alarm.group(2), alarm.group(1)))
    raise SystemExit
pre = re.findall(r"clk-offset-pre-record.*?field \+(\d+) current 0x[0-9a-f]+ \((-?\d+)\)", text)
after = re.findall(r"clk-offset-readback.*?field \+(\d+) = 0x[0-9a-f]+ \((-?\d+)\)", text)
if pre and after and pre[-1][0] == after[-1][0] and after[-1][1] != "-1":
    print("%d %s" % (int(pre[-1][1]) - int(after[-1][1]), pre[-1][0]))
PY
)
    if [ -n "$corr" ]; then
        set -- $corr
        bit=$(printf '%s' "$cand" | cut -d: -f1)
        echo "   corrective single step (inverse of what is left applied):" >&2
        echo "     sudo OUT_ROOT=$out_root sh $0 $bit:$2:$1" >&2
        echo "   ...but re-read first: if the field is not what the read-back" >&2
        echo "   line above quotes, this step is the wrong one." >&2
    else
        echo "   (could not recompute the step from the log: read the lines above)" >&2
    fi
    echo "   Do not issue any other write until the read runbook shows the" >&2
    echo "   snapshot's block again. Nothing else is claimed about this run." >&2
    echo ""
fi

echo "note: this verdict covers control storage and restore. A frequency effect"
echo "      needs a separate comparison within the same bounded workload."

# Digests last, so they cover the finished logs and the exit status. The exit
# code says which of the three states the run ended in: 0 verified, 1 not
# verified (block back), 2 not verified with the block NOT proven back.
if [ "$verified" = yes ] && [ "$restore_bad" = no ]; then
    printf 'write run: VERIFIED (SET + read-back + restore)\n' >> "$bundle/README.txt"
    stamp_sums
    exit 0
fi
if [ "$restore_bad" = yes ]; then
    printf 'write run: RESTORE NOT VERIFIED -- device may still carry the step\n' \
        >> "$bundle/README.txt"
    stamp_sums
    exit 2
fi
printf 'write run: not verified, block restored\n' >> "$bundle/README.txt"
stamp_sums
exit 1
