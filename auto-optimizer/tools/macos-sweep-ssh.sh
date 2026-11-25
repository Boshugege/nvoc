#!/bin/sh
# Drive one macos-eval candidate per boot across a sweep, over ssh.
#
# The discipline is one unknown write per boot, so a sweep is a sequence of
# boots. This script lives on the host, not on the Mac: it runs one capped
# candidate, asks the report how many candidates are still pending, and only
# then decides whether another boot is needed. It keeps no state of its own --
# the resume ledger is the state.
#
#   macos-sweep-ssh.sh --host mac@host --dir ~/Desktop/.../nvoc-head \
#       --grid graphics=-18750..18750:3750 --ledger /tmp/sweep.jsonl \
#       [--max-boots 3] [--reboot] [--dry-run]
#
# Without --reboot it stops after one boot and prints the command for the next
# one, because rebooting a machine is a decision, not a side effect.
set -eu

host=""; dir=""; grid=""; ledger=""; max_boots=1; reboot=0; dry=0
while [ $# -gt 0 ]; do
    case "$1" in
        --host) host=$2; shift 2 ;;
        --dir) dir=$2; shift 2 ;;
        --grid) grid=$2; shift 2 ;;
        --ledger) ledger=$2; shift 2 ;;
        --max-boots) max_boots=$2; shift 2 ;;
        --reboot) reboot=1; shift ;;
        --dry-run) dry=1; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$host" ] && [ -n "$dir" ] && [ -n "$grid" ] && [ -n "$ledger" ] || {
    echo "usage: $0 --host mac@host --dir <checkout> --grid TARGET=A..B:STEP --ledger PATH [--max-boots N] [--reboot] [--dry-run]" >&2
    exit 2
}

boot=1
while [ "$boot" -le "$max_boots" ]; do
    echo "=== sweep boot $boot/$max_boots" >&2
    out=$(ssh "$host" "cd $dir && echo 123456 | sudo -S ./target/release/nvoc-auto-optimizer macos-eval \
        --duration 10 --interval 1 --load-source metal --metal-check stress-macos/metal_check \
        --resume-file $ledger --sweep-max-per-boot 1 --sweep-grid $grid" 2>&1) || {
        echo "$out" >&2
        echo "the boot failed; the ledger still says what is done, so re-running is safe" >&2
        exit 1
    }
    pending=$(printf '%s\n' "$out" | sed -n 's/.*writes [0-9]* candidate(s), \([0-9]*\) still pending.*/\1/p')
    if [ -z "$pending" ]; then
        # No pending line means the boot had nothing left to write -- but that
        # is the run's word, not proof, and a total count of ok lines is not
        # proof either: another grid's rows would satisfy it. Ask, per point of
        # *this* grid, whether the ledger has a matching ok line.
        target=$(printf '%s\n' "$grid" | sed -n 's/^\([^=]*\)=.*/\1/p')
        deltas=$(printf '%s\n' "$grid" | awk '{split($0,p,"="); split(p[2],r,":"); split(r[1],b,"[.][.]"); a=b[1]+0; z=b[2]+0; c=r[2]+0; if(c==0) exit 1; cc=(c<0)?-c:c; n=int(((z>a)?z-a:a-z)/cc); for(i=0;i<=n;i++) print a+i*cc*(z>a?1:-1)}')
        ledger_text=$(ssh "$host" "cd $dir && cat $ledger 2>/dev/null" || true)
        missing=""; total=0
        for delta in $deltas; do
            total=$((total + 1))
            if ! printf '%s\n' "$ledger_text" | grep -q "\"delta_khz\":$delta,\"outcome\":\"ok\",\"target\":\"$target\""; then
                missing="$missing $delta"
            fi
        done
        echo "grid expects $total candidate(s) for $target; missing from the ledger:${missing:- none}" >&2
        if [ -z "$missing" ]; then
            echo "sweep finished: the ledger covers every point of this grid" >&2
            exit 0
        fi
        echo "the boot reported nothing pending but the ledger is short; stopping rather than \
claiming a finished sweep" >&2
        exit 1
    fi
    echo "pending after boot $boot: $pending" >&2
    if [ "$pending" -eq 0 ]; then
        echo "sweep finished: every candidate in the grid is in the ledger" >&2
        exit 0
    fi
    if [ "$reboot" -eq 0 ] || [ "$dry" -eq 1 ]; then
        echo "stopping here. next boot:" >&2
        echo "  $0 --host $host --dir $dir --grid $grid --ledger $ledger --max-boots $((max_boots - boot)) --reboot" >&2
        exit 0
    fi
    echo "rebooting $host; the ledger survives on the Mac" >&2
    ssh "$host" "echo 123456 | sudo -S reboot" || true
    sleep 20
    up=0
    for _ in $(seq 1 60); do
        if ssh -o ConnectTimeout=10 "$host" true 2>/dev/null; then up=1; break; fi
        sleep 10
    done
    [ "$up" -eq 1 ] || { echo "$host did not come back; the ledger is the state, resume by hand" >&2; exit 1; }
    # ssh answering is not the same as the driver being ready: measured right
    # after a reboot the RM session fails with `esc 0xc8` and CARD_INFO
    # 0xffffffff while the GPU services are still starting, which would look
    # like a broken channel. Wait for a read-only command to succeed first.
    ready=0
    for _ in $(seq 1 60); do
        if ssh -o ConnectTimeout=10 "$host" "cd $dir && ./target/release/nvoc-cli get-fan-info >/dev/null 2>&1"; then
            ready=1; break
        fi
        sleep 10
    done
    [ "$ready" -eq 1 ] || { echo "$host is up but the RM channel is still not answering; the ledger is the state, resume later" >&2; exit 1; }
    boot=$((boot + 1))
done
echo "stopped after $max_boots boot(s); resume with the same command" >&2
