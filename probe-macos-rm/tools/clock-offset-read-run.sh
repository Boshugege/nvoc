#!/bin/sh
# Clock-offset read runbook (read-only, macOS test host).
#
# The private clock ids are answered by the GSP firmware; two of them have
# already taken the whole legacy path down for a session, so the ordering
# matters more than the commands:
#
#   1. --clkctl-hex   full byte dump of every read-only clock block, and the
#                     AUTHORITATIVE record of their contents: 0x2080901b with
#                     its mask seeded at +4 (0xFF, 0x3FF, 0x1, then 0x0 --
#                     see the PREDICTIONS block in nvx_suite.c, which this
#                     run is meant to falsify or confirm).  The first of
#                     those four runs on an 0xA5 background: that is the only
#                     form whose write set is observable, because the handler
#                     writes 13 dwords per record and 12 of them are zero at
#                     stock.  It is the same request that already answered
#                     NV_OK pre-wedge, and it replaces the delta printout that
#                     lost 66 of its 106 changed dwords.  0x20809025,
#                     0x20809019, and 0x2080901d last (it was only ever read
#                     with a 0xA5 background, and a timeout on it must not cost
#                     the other blocks).  The 0xA5 sweep's own delta printout
#                     records the written values and the dword class map
#                     losslessly, but the log captured in the 2026-10-17 sweep
#                     kept only 40 of its 106 changed dwords, so the full dump
#                     below is still the authority.
#   2. --clkctl-read  the wider battery; it refuses 0x20809026@5332 outright
#                     (that call is the one that latched the 2026-10-17 wedge;
#                     the in-tree NVAPI copy sizes this id at 2508 for its own
#                     caller struct, so "the id is dangerous" would overstate it
#                     -- it is the RM-sized form this battery stays away from)
#                     and aborts on the first 0x65/0x62
#   3. --fan-read     the fan family's only named candidate: the private
#                     0x20808532 and its documented public twin 0x20800532, 336 B
#                     on an 0xA5 background.  It is here because the census tool
#                     found that neither id appears in ANY returned status under
#                     outputs/ -- the fan RM face has been described as measured
#                     but was never dispatched in a recorded run, so this run is
#                     what turns it into an artifact (tools/audit-live-attempts.py).
#                     Read-only, and a 0x62 is expected while the wedge lasts.
#
# Nothing here is a write: the probe refuses every documented SET id before it
# builds a buffer, and the idle-only build cannot send stage-3 writes at all.
# A 0x62 anywhere means the GSP is still wedged from an earlier session and
# the run has to stop until the machine is rebooted.
#
# Order matters after a reboot, and every step but the last one is a read:
#   1. this script's first run answers whether the GSP is healthy again (a 0x62
#      in its very first case means stop; do not retry the battery).
#   2. `nvoc-cli get-settings --macos-rm` -- read-only, and the first look at the
#      measured offset window and the offset domains on the real device. Those
#      fields exist in the code and in pynvoc (asserted at -18750/+18750); the
#      2026-10-16 stage-10 report predates them, so no device record shows them
#      yet and this is what turns "implemented" into "measured".
#   3. only then, and only with authorisation, the single gated write:
#      sh tools/clock-offset-write-run.sh <bit>:<off>:<delta_khz>
#
# Usage:
#   OGKM=/path/to/ogkm610 sh tools/clock-offset-read-run.sh [/path/to/outputs]
#   (default output root: $HOME/Desktop/macOS-nvoc-develop/outputs)

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
out_root=${1:-$HOME/Desktop/macOS-nvoc-develop/outputs}
ogkm=${OGKM:-$HOME/Desktop/macOS-nvoc-develop/reference/open-gpu-kernel-modules-610.57.04}

if [ ! -x "$root/probe-macos-rm-idle" ]; then
    echo "building probe-macos-rm-idle (idle profile; no GPU workloads)" >&2
    (cd "$root" && OGKM="$ogkm" sh build.sh)
fi

stamp=$(date -u +%Y%m%dT%H%M%SZ)
bundle="$out_root/stage-clkctl-readrun-$stamp"
mkdir -p "$bundle"
echo "bundle: $bundle"

# What produced this bundle, written before the first request: a healthy window
# cannot be repeated (a later rejection can wedge the session), so the revision,
# the binary's identity and its build freshness have to be in the artifact
# rather than in the operator's memory.  A binary older than the sources prints
# STALE -- the failure that once cost this project a "row is missing" hunt.
if command -v python3 >/dev/null 2>&1; then
    python3 "$root/tools/record-run-provenance.py" \
        --bundle "$bundle" \
        --label "clock-offset read battery (read-only; idle profile)" \
        --binary "$root/probe-macos-rm-idle" \
        --source "$root" \
        --ogkm "$ogkm" \
        --case "clkctl-hex: full byte dump, first case 0x2080901b@2108 in@4=0xFF on an 0xA5 background" \
        --case "clkctl-read: the wider battery, refusing 0x20809026@5332" \
        --case "fan-read: 0x20808532 and its public twin, 336 B on 0xA5" \
        --case "hwfs-read: 0x20808540, 24 B" \
        --id "0x2080901b:2108 (offset control GET)" \
        --id "0x20809025:7188 (offset range)" \
        --id "0x20809019:12336 (domain directory)" \
        --id "0x2080901d:32680 (record stride still open)" \
        --preregistered "0x2080901d: a third occurrence at +268 means a 128 B period; at +1160 means the 32x1020 split; at both or neither, both readings are wrong" \
        --preregistered "any non-zero value dword in the 0x2080901b record is the first G1 current-offset candidate" \
        --preregistered "a 0x62 or 0x65 anywhere stops the run: read the bytes, do not retry" \
        || echo "  (provenance stamp failed; the logs are still the artifact)" >&2
fi

stamp_sums() {
    # One digest implementation for both runbooks, over every file in the bundle.
    if command -v python3 >/dev/null 2>&1; then
        python3 "$root/tools/record-run-provenance.py" --bundle "$bundle" --sums \
            || echo "  (digest pass failed; the logs are still the artifact)" >&2
    else
        (cd "$bundle" && shasum -a 256 ./*.txt > SHA256SUMS 2>/dev/null || true)
    fi
}

run_one() {
    name=$1
    shift
    echo "== $name =="
    "$root/probe-macos-rm-idle" "$@" > "$bundle/$name.txt" 2>&1 && rc=0 || rc=$?
    echo "  rc=$rc -> $bundle/$name.txt"
    # Both wedge signals stop the run. 0x65 is the latch that wedges the GSP for
    # the session and 0x62 is every later reply: once either appears, the wider
    # battery answers 0x62 for every id and proves nothing about the ids it
    # reaches, so continuing would cost a session and buy no information.
    if grep -q "NV_ERR_RESET_REQUIRED" "$bundle/$name.txt" ||
       grep -q "NV_ERR_TIMEOUT" "$bundle/$name.txt"; then
        wedge=1
    fi
}

run_one clkctl-hex   --clkctl-hex

if [ "${wedge:-0}" -ne 0 ]; then
    echo "  !! NV_ERR_RESET_REQUIRED / NV_ERR_TIMEOUT in the first case: the GSP" >&2
    echo "     legacy path is wedged (see the doc, section 6). STOPPING here --" >&2
    echo "     the wider battery would answer 0x62 for every id. Reboot, then" >&2
    echo "     rerun; nothing was written." >&2
    # The abort bundle is an artifact too -- the wedge datapoint -- so it gets its
    # digests before the run ends, not only on the healthy path.
    stamp_sums
    exit 5
fi

run_one clkctl-read  --clkctl-read

# The fan candidate has no artifact at all yet, so it is read on every healthy
# run rather than described as measured.  Its result lands in fan-read.txt and the
# census tool picks the id up from there.
run_one fan-read    --fan-read

# Same reasoning for the cooler plane: 0x20808540 is the family's only named and
# in-table id and the census lists it as never dispatched.  24 B is the paramSize
# its method-table row declares.
run_one hwfs-read   --hwfs-read

if command -v python3 >/dev/null 2>&1; then
    python3 "$root/tools/decode-clk-containers.py" "$bundle/clkctl-hex.txt" \
        > "$bundle/decode.txt" 2>&1 || true
    echo "  decode -> $bundle/decode.txt"
fi

stamp_sums
echo "done: $bundle"
