#!/bin/sh
# Clock-offset read runbook (read-only, macOS test host).
#
# The private clock ids are answered by the GSP firmware; two of them have
# already taken the whole legacy path down for a session, so the ordering
# matters more than the commands:
#
#   1. --clkctl-hex   full byte dump of every read-only clock block, and the
#                     AUTHORITATIVE record of their contents: 0x2080901b with
#                     its mask seeded at +4 (0xFF, then 0x0), 0x20809025,
#                     0x20809019, and 0x2080901d last (it was only ever read
#                     with a 0xA5 background, and a timeout on it must not cost
#                     the other blocks).  The 0xA5 sweep's own delta printout
#                     is not lossless -- it reported 106 changed dwords for
#                     0x2080901b but the log kept 40 of them.
#   2. --clkctl-read  the wider battery; it refuses 0x20809026@5332 outright
#                     and aborts on the first 0x65/0x62
#
# Nothing here is a write: the probe refuses every documented SET id before it
# builds a buffer, and the idle-only build cannot send stage-3 writes at all.
# A 0x62 anywhere means the GSP is still wedged from an earlier session and
# the run has to stop until the machine is rebooted.
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

run_one() {
    name=$1
    shift
    echo "== $name =="
    "$root/probe-macos-rm-idle" "$@" > "$bundle/$name.txt" 2>&1 && rc=0 || rc=$?
    echo "  rc=$rc -> $bundle/$name.txt"
    if grep -q "NV_ERR_RESET_REQUIRED" "$bundle/$name.txt"; then
        echo "  !! NV_ERR_RESET_REQUIRED present: the GSP legacy path is still"
        echo "     wedged from an earlier session; reboot before trusting any"
        echo "     further private-clock read (see the doc, section 6)."
    fi
}

run_one clkctl-hex   --clkctl-hex
run_one clkctl-read  --clkctl-read

if command -v python3 >/dev/null 2>&1; then
    python3 "$root/tools/decode-clk-containers.py" "$bundle/clkctl-hex.txt" \
        > "$bundle/decode.txt" 2>&1 || true
    echo "  decode -> $bundle/decode.txt"
fi

(
    cd "$bundle"
    shasum -a 256 ./*.txt > SHA256SUMS 2>/dev/null || true
)
echo "done: $bundle"
