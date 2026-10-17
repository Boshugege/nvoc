#!/usr/bin/env python3
"""Which RM control ids has this host ever actually dispatched, and with what answer?

The feature matrix and the review records make per-id claims -- "never issued",
"implemented awaiting measurement", "the driver refused it" -- and those claims
are only as good as the artifact behind them.  This tool derives the census from
the capture directory instead of from prose: it reads every raw *.txt and *.jsonl
capture, takes the rows that carry a status, and counts the ids named in those
rows.  Markdown is skipped on purpose: a review that mentions an id is not
evidence that the id was ever sent.

Two things it is for:

  * an id that appears only in the method table has never been dispatched, which
    is a different statement from "the driver refused it" and from "the command
    does not exist" (see the audit doc's naming rules);
  * an id that was dispatched but never answered NV_OK is unmeasured, not
    unsupported -- while the GSP reset is pending every forwarded control gets
    NV_ERR_RESET_REQUIRED, so those rows record the wedge, not the command.

Usage:
    python3 tools/audit-live-attempts.py <outputs-dir> [--table <tsv>] [--id 0x...]
    python3 tools/audit-live-attempts.py --selftest

Exit status is 0 when it printed a census; --id makes it exit 1 if that id never
answered NV_OK, so a claim can be gated on the artifact.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ID_RE = re.compile(r"0x(2080[0-9a-f]{4})", re.I)
STATUS_RE = re.compile(r"status[\"'\s:]+0x([0-9a-f]{8})", re.I)
STATUS_NAME_RE = re.compile(r"\(([A-Z_]+)\)|\"statusName\"\s*:\s*\"([A-Z_]+)\"")
VERDICT_RE = re.compile(
    r"\[(OK|REJECTED|HARD|SKIP|FAILED)\]|\"verdict\"\s*:\s*\"([A-Z_]+)\""
)
# The status regex captures the eight digits without the 0x, so the success value
# is compared in the same form.
OK = "00000000"
SCANNED_SUFFIXES = (".txt", ".jsonl")


def rows(text: str) -> list[tuple[str, str, str]]:
    """(status, status name, the whole row) for every row that carries a status."""
    out = []
    for line in text.splitlines():
        status = STATUS_RE.search(line)
        if not status:
            continue
        name = STATUS_NAME_RE.search(line)
        verdict = VERDICT_RE.search(line)
        label = (name.group(1) or name.group(2)) if name else "(unnamed)"
        if verdict:
            label = (verdict.group(1) or verdict.group(2)) + " " + label
        out.append((status.group(1).lower(), label, line.strip()))
    return out


def row_ids(line: str) -> set[str]:
    """The ids a row is about, each counted once.

    One row is one dispatch, so the same id written twice on it -- a probe prints
    both the label ("fan-private-0x20808532") and the argument ("cmd 0x20808532
    size 336") -- must not be counted twice.  Counting mentions instead of rows
    inflated every figure in the first census by exactly the number of times a row
    happened to repeat its id; see the selftest, which asserts the dedupe.

    An id that a row only *mentions* in its detail (a twin, a note) is still
    credited with that row, so the census is exact for the id a row is about and
    an upper bound for one merely named inside it.  "Never dispatched" is sound
    either way: no row names the id at all.
    """
    return {"0x" + m.group(1).lower() for m in ID_RE.finditer(line)}


def census(paths: list[Path]) -> dict[str, dict[str, object]]:
    seen: dict[str, dict[str, object]] = {}
    for path in paths:
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        for status, label, line in rows(text):
            for cid in sorted(row_ids(line)):
                entry = seen.setdefault(cid, {"ok": 0, "labels": {}, "files": set()})
                if status == OK:
                    entry["ok"] += 1  # type: ignore[operator]
                labels = entry["labels"]
                assert isinstance(labels, dict)
                labels[status + " " + label] = labels.get(status + " " + label, 0) + 1
                files = entry["files"]
                assert isinstance(files, set)
                files.add(path.name)
    return seen


def table_ids(path: Path) -> set[str]:
    return {
        "0x" + m.group(1).lower()
        for m in ID_RE.finditer(path.read_text(errors="replace"))
    }


def report(
    seen: dict[str, dict[str, object]], table: set[str], only: str | None
) -> int:
    ids = sorted(only and {only} or seen)
    answered = [cid for cid in ids if seen.get(cid, {}).get("ok")]
    dispatched = [cid for cid in ids if cid in seen]
    print(
        f"ids named in a returned status: {len(seen)}; answered NV_OK at least once: {len(answered)}"
    )
    for cid in ids:
        entry = seen.get(cid)
        if entry is None:
            print(
                f"  {cid}: NEVER DISPATCHED in these captures"
                + (" (in the method table)" if cid in table else "")
            )
            continue
        labels = entry["labels"]
        assert isinstance(labels, dict)
        files = entry["files"]
        assert isinstance(files, set)
        shown = ", ".join(f"{n}x {lab}" for lab, n in sorted(labels.items()))
        print(f"  {cid}: {entry['ok']}x NV_OK; {shown}  [{', '.join(sorted(files))}]")
    if table:
        silent = sorted(table - set(dispatched))
        print(f"method-table ids never dispatched: {len(silent)} of {len(table)}")
        for cid in silent:
            print(f"  {cid}: in the table, never in a returned status")
    if only:
        if not seen.get(only, {}).get("ok"):
            print(f"GATE: {only} never answered NV_OK in these captures")
            return 1
    return 0


def selftest() -> int:
    import io
    import tempfile

    sample_jsonl = (
        '{"time":1,"id":"V2.thm","name":"thermal","status":"0x00000062","statusName":"NV_ERR_RESET_REQUIRED",'
        '"verdict":"REJECTED","detail":"EXECUTE_V2_PHYSICAL (0x20808513): refused"}\n'
        '{"time":2,"id":"V2.ok","name":"ok","status":"0x00000000","statusName":"NV_OK",'
        '"verdict":"OK","detail":"domains-get-info cmd 0x20809019 size 12336 -> NV_OK"}\n'
    )
    sample_txt = (
        "[OK] V2.5 thermal-read status 0x00000000 (NV_OK)  cmd 0x20800513 size 1432 -> NV_OK\n"
        "[REJECTED] V2.clkctl gc-1d status 0x00000062 (NV_ERR_RESET_REQUIRED)  cmd 0x2080901d size 32680 -> 0x62\n"
        # The probe's own shape: the label and the argument repeat the same id, and
        # one dispatch has to stay one count.
        "[REJECTED] V2.fan fan-private-0x20808532 status 0x00000062 (NV_ERR_RESET_REQUIRED)  cmd 0x20808532 size 336 -> NV_ERR_RESET_REQUIRED\n"
        # A row that names a second id (a twin, a note) credits both: exact for the
        # id the row is about, an upper bound for one only mentioned in it.
        "[OK] V2.twin twin-read status 0x00000000 (NV_OK)  private 0x20809027 twin 0x20809028 -> NV_OK\n"
        "no status on this line 0x20809999 is not a row\n"
    )
    prose = "A review saying 0x2080ffff returned NV_OK is not evidence.\n"
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "a.jsonl").write_text(sample_jsonl)
        (root / "b.txt").write_text(sample_txt)
        (root / "c.md").write_text(prose)
        (root / "table.tsv").write_text(
            "0x20800513\tthermal\n0x20808513\tphysical\n0x2080FFFF\tprose-only\n"
        )
        paths = sorted(p for p in root.iterdir() if p.suffix in SCANNED_SUFFIXES)
        seen = census(paths)
        assert seen["0x20800513"]["ok"] == 1, seen["0x20800513"]
        assert seen["0x20809019"]["ok"] == 1, seen["0x20809019"]
        assert seen["0x2080901d"]["ok"] == 0, seen["0x2080901d"]
        assert seen["0x20808513"]["ok"] == 0, seen["0x20808513"]
        assert seen["0x20808532"]["labels"] == {
            "00000062 REJECTED NV_ERR_RESET_REQUIRED": 1
        }, "a row that spells its id twice is one dispatch, not two"
        assert seen["0x20809027"]["ok"] == 1 and seen["0x20809028"]["ok"] == 1, (
            "an id a row only mentions is still credited (documented upper bound)"
        )
        assert "0x20809999" not in seen, "a row without a status must not count"
        assert "0x2080ffff" not in seen, "prose in an .md file must not count"
        table = table_ids(root / "table.tsv")
        assert table == {"0x20800513", "0x20808513", "0x2080ffff"}, table
        sink = io.StringIO()
        real_stdout, sys.stdout = sys.stdout, sink
        try:
            assert report(seen, table, None) == 0
            assert report(seen, table, "0x20808513") == 1, (
                "the gate must fail on an unread id"
            )
            assert report(seen, table, "0x20800513") == 0
        finally:
            sys.stdout = real_stdout
        text = sink.getvalue()
        assert "0x20808513: NEVER DISPATCHED" not in text, text
        assert "in the table, never in a returned status" in text, text
        assert "GATE: 0x20808513 never answered NV_OK" in text, text
    print(
        "selftest ok: row detection, id extraction, prose and statusless rows rejected, "
        "method-table cross-check, --id gate both ways"
    )
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("--selftest", "-h", "--help"):
        if argv and argv[0] == "--selftest":
            return selftest()
        print(__doc__)
        return 0
    root = Path(argv[0])
    if not root.is_dir():
        print(f"{root}: not a directory", file=sys.stderr)
        return 2
    table: set[str] = set()
    only = None
    rest = argv[1:]
    while rest:
        flag = rest.pop(0)
        if flag == "--table" and rest:
            table = table_ids(Path(rest.pop(0)))
        elif flag == "--id" and rest:
            only = rest.pop(0).lower()
        else:
            print(f"unknown argument: {flag}", file=sys.stderr)
            return 2
    paths = sorted(p for p in root.rglob("*") if p.suffix in SCANNED_SUFFIXES)
    print(f"scanning {len(paths)} capture files under {root}")
    return report(census(paths), table, only)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
