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

It reports two tiers and keeps them apart, because two different questions were
being answered with one number.  The status tier is "the id appears on a row that
printed a status octet".  The second tier is ids that a capture shows being sent
and answered with a buffer, recorded as a banner plus a hex dump with no status
printed at all; those are dispatches, so "never dispatched" would be wrong for
them, but their answer is only implicit, so they must not be counted as NV_OK.

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
# "status" must not be the tail of a hyphenated word.  The capture set contains
# prose that reads "rails-get-status 0x2080B202" (a settings note, and two banner
# lines in a payload dump); the loose pattern `status[\"'\s:]+0x...` matches the
# `status 0x2080B202` inside it, which turned the whole line into a status row and
# credited every id it happens to mention -- e.g. `0x208090a2`, whose only seven
# "dispatches" in the first census were such prose.  No row shape in the captures
# regresses: the tightened pattern rejects 9 lines and accepts 5152, and only 6 of
# the accepted ones lack a verdict tag, a JSON "status" key and a status name, and
# those 6 are real probe rows ("repeat 1: status 0x..., words differing ...").
STATUS_RE = re.compile(r"(?<![-\w])status[\"'\s:]+0x([0-9a-f]{8})", re.I)
# A run may record a call as `== <label> 0x<id> @ <size> ==` followed by the buffer
# it returned, printing no status at all; that is a dispatch, not a status, so it is
# kept in its own tier (see banner_dispatches).  re.I matters: ids are printed with
# upper-case hex tails (`0x208090A2`).
BANNER_RE = re.compile(r"^=+\s*(.*?)0x(2080[0-9a-f]{4})\b(.*?)=+\s*$", re.I)
DUMP_RE = re.compile(r"^\s*[0-9a-f]{4}: ")
# Third shape, and the one that hid the offset work's own evidence: a case header that
# names the id, followed by rows that print only the octet --
#     == rails-get-info 0x2080B201 boundary ==
#     size-1 (2443) -> 0x0000001f, size (2444) -> 0x00000000
# The id is in the header and nowhere else, so a census that reads ids off rows reports
# this id as never dispatched.  Attribution is by header scope, which is weaker than a
# row naming its own id, so the tier is reported on its own; the scope ends at the next
# line that names an id, so a row cannot drift onto a later case.
CASE_RE = re.compile(r"^\s*[=-]{2,}[^=]*?0x(2080[0-9a-f]{4})\b", re.I)
SWEEP_RE = re.compile(r"->\s*0x([0-9a-f]{8})")

# A few rows name the control by label and print no numeric id, so no census can read
# the id off them: `[OK] V2.4 rated-tdp-get-control status 0x00000000 (NV_OK)`.  The
# pair below is transcribed from the open headers, and --ogkm checks it against them
# when they are at hand; without them the check prints SKIP instead of passing.
LABEL_IDS = {
    "rated-tdp-get-control": (
        "0x2080206e",
        "NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL",
    ),
}
LABEL_STATUS_RE = re.compile(r"(?<![\w-])status[\"\'\s:]+0x([0-9a-f]{8})", re.I)
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


def banner_dispatches(paths: list[Path]) -> dict[str, dict[str, object]]:
    """Ids a capture shows as dispatched with a payload dump but no printed status.

    Some runs record the call as a banner plus the returned buffer --
    `== vfpoints-get 0x208090A2 @ 524 ==` then a hex dump -- and never print the
    status octet.  Those rows are real dispatches, but the answer is only implicit,
    so they must not be mixed into the status census: this is a second, weaker tier
    that exists so "never dispatched" is not claimed for an id that was in fact
    sent.  It is documented as indirect because the accept is inferred from the
    probe's own convention (it dumps a buffer only when the call returned NV_OK).
    """
    found: dict[str, dict[str, object]] = {}
    for path in paths:
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            continue
        for index, line in enumerate(lines):
            banner = BANNER_RE.match(line.strip())
            if not banner:
                continue
            # The dump must be inside this case's own block: stop at the next case
            # header, otherwise a short case borrows the following case's dump (the
            # selftest's second banner caught exactly that).
            window = []
            for ln in lines[index + 1 :]:
                if BANNER_RE.match(ln.strip()) or CASE_RE.match(ln):
                    break
                if ln.strip():
                    window.append(ln)
                if len(window) == 8:
                    break
            if not any(DUMP_RE.match(ln) for ln in window):
                continue
            cid = "0x" + banner.group(2).lower()
            entry = found.setdefault(cid, {"banners": 0, "files": set()})
            entry["banners"] = int(entry["banners"]) + 1  # type: ignore[arg-type]
            files = entry["files"]
            assert isinstance(files, set)
            files.add(path.name)
    return found


def label_rows(paths: list[Path]) -> dict[str, dict[str, object]]:
    """id -> counts for rows that print a label instead of the numeric id."""
    found: dict[str, dict[str, object]] = {}
    for path in paths:
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            continue
        for line in lines:
            octet = LABEL_STATUS_RE.search(line)
            if not octet:
                continue
            for label, (cid, _symbol) in LABEL_IDS.items():
                if label not in line:
                    continue
                entry = found.setdefault(cid, {"ok": 0, "labels": {}, "files": set()})
                if octet.group(1) == "00000000":
                    entry["ok"] = int(entry["ok"]) + 1  # type: ignore[arg-type]
                labels = entry["labels"]
                assert isinstance(labels, dict)
                verdict = "OK" if octet.group(1) == "00000000" else "REJECTED"
                key = octet.group(1) + " " + verdict + " (by label)"
                labels[key] = labels.get(key, 0) + 1
                files = entry["files"]
                assert isinstance(files, set)
                files.add(path.name)
    return found


def sweep_rows(paths: list[Path]) -> dict[str, dict[str, object]]:
    """id -> octet counts for rows whose status carries no id, under a case header.

    Kept apart from the status tier because the association is the header's, not the
    row's, so a count from here is an upper bound; whatever it attributes, it says
    which header line did the attributing.
    """
    found: dict[str, dict[str, object]] = {}
    for path in paths:
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError:
            continue
        current: str | None = None
        header = ""
        for lineno, line in enumerate(lines, 1):
            case = CASE_RE.match(line)
            if case:
                current = "0x" + case.group(1).lower()
                header = f"{path.name}:{lineno}"
                continue
            if ID_RE.search(line) and not SWEEP_RE.search(line):
                # A row naming its own id ends the header's scope.
                current = None
                continue
            if current is None:
                continue
            for octet in SWEEP_RE.findall(line):
                entry = found.setdefault(
                    current, {"ok": 0, "octets": {}, "headers": [], "files": set()}
                )
                if octet == "00000000":
                    entry["ok"] = int(entry["ok"]) + 1  # type: ignore[arg-type]
                octets = entry["octets"]
                assert isinstance(octets, dict)
                key = octet + (
                    " OK (by sweep)" if octet == "00000000" else " REJECTED (by sweep)"
                )
                octets[key] = octets.get(key, 0) + 1
                headers = entry["headers"]
                assert isinstance(headers, list)
                if header not in headers:
                    headers.append(header)
                files = entry["files"]
                assert isinstance(files, set)
                files.add(path.name)
    return found


def verify_labels(src: Path | None, ogkm: Path | None) -> list[str]:
    """Check each hand-transcribed label->id pair against what is on disk."""
    notes = []
    if src is None or not src.is_file():
        notes.append(f"SKIP label->id check: no probe source at {src}")
    else:
        text = src.read_text(errors="replace")
        for label, (cid, symbol) in LABEL_IDS.items():
            notes.append(
                f"{'ok' if label in text else 'MISSING'} label {label!r} in {src.name}"
            )
            notes.append(
                f"{'ok' if symbol in text else 'MISSING'} call {symbol} in {src.name}"
            )
    if ogkm is None or not ogkm.is_dir():
        notes.append(f"SKIP define check: no open headers at {ogkm}")
        return notes
    for label, (cid, symbol) in LABEL_IDS.items():
        hits = []
        for header in sorted(ogkm.rglob("*.h")):
            for match in re.finditer(
                rf"#define\s+{symbol}\s+\(?0x([0-9a-f]{{8}})",
                header.read_text(errors="replace"),
            ):
                hits.append("0x" + match.group(1).lower())
        if not hits:
            notes.append(f"MISSING define {symbol} under {ogkm}")
        elif set(hits) == {cid}:
            notes.append(f"ok {symbol} == {cid} in the open headers")
        else:
            notes.append(f"MISMATCH {symbol} -> {sorted(set(hits))}, recorded {cid}")
    return notes


def report(
    seen: dict[str, dict[str, object]],
    indirect: dict[str, dict[str, object]],
    by_label: dict[str, dict[str, object]],
    by_sweep: dict[str, dict[str, object]],
    table: set[str],
    only: str | None,
    notes: list[str] | None = None,
) -> int:
    ids = sorted(only and {only} or seen)
    answered = [cid for cid in ids if seen.get(cid, {}).get("ok")]
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
    print(
        f"ids dispatched with a label but no numeric id on the row: {len(by_label)}"
        " (attribute only through the label->id map, which --ogkm checks)"
    )
    for cid, entry in sorted(by_label.items()):
        labels = entry["labels"]
        assert isinstance(labels, dict)
        files = entry["files"]
        assert isinstance(files, set)
        if cid in seen:
            print(
                f"  {cid} also has plain rows; the by-label tier adds "
                f"{entry['ok']}x NV_OK (merge before quoting a count)"
            )
            continue
        shown = ", ".join(f"{n}x {lab}" for lab, n in sorted(labels.items()))
        print(
            f"  {cid}: {entry['ok']}x NV_OK by label; {shown}  [{', '.join(sorted(files))}]"
        )
    print(
        f"ids whose statuses were printed without an id, under a case header: "
        f"{len(by_sweep)} (attributed by the header; the header is printed with it)"
    )
    for cid, entry in sorted(by_sweep.items()):
        octets = entry["octets"]
        assert isinstance(octets, dict)
        headers = entry["headers"]
        assert isinstance(headers, list)
        shown = ", ".join(f"{n}x {lab}" for lab, n in sorted(octets.items()))
        print(f"  {cid}: {entry['ok']}x NV_OK by sweep; {shown}")
        print(f"                headers: {', '.join(headers[:3])}")
    dumped = sorted(set(indirect) - set(seen))
    print(
        f"ids dispatched with a payload dump but no printed status: {len(dumped)}"
        " (indirect: the probe dumps a buffer only on success)"
    )
    for cid in dumped:
        entry = indirect[cid]
        files = entry["files"]
        assert isinstance(files, set)
        print(
            f"  {cid}: {entry['banners']} dump banner(s), no status row  "
            f"[{', '.join(sorted(files))}]"
        )
    if notes:
        for note in notes:
            print(f"label->id check: {note}")
    if table:
        silent = sorted(
            table - set(seen) - set(indirect) - set(by_label) - set(by_sweep)
        )
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
        # Prose inside a capture is not a dispatch row: here the word before the
        # octet is "rails-get-status", so nothing on this line is a status.
        "-- rails-get-status 0x2080b202 @3232 (3232 bytes) non-zero words -- 0x208090a2\n"
        # A dump banner is a dispatch with no printed status: it must not be counted
        # as a status, and it must not be reported as "never dispatched" either.
        # A row that names the control by label only: the id is not on the row at all.
        "[OK] V2.4 rated-tdp-get-control status 0x00000000 (NV_OK)  raw[0..2]=0x0 0x0 0x0\n"
        "[REJECTED] V2.rtdp rated-tdp-get-control status 0x00000062 (NV_ERR_RESET_REQUIRED)  client GLOBAL(0)\n"
        # A case header names the id; the row under it prints only the octet.
        "== rails-get-info 0x2080B201 boundary ==\n"
        " smallest accepted size = 2444\n"
        " size-1 (2443) -> 0x0000001f, size (2444) -> 0x00000000\n"
        "[OK] V2.reg other-read status 0x00000000 (NV_OK)  cmd 0x20801303 size 4 -> NV_OK\n"
        " size-1 (7) -> 0x0000001f, size (8) -> 0x00000000\n"
        "== vfpoints-get 0x208090A2 @ 524 ==\n"
        "-- vfpoints-get @ 524 --\n"
        "  0000: 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  |................|\n"
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
        assert "0x208090a2" not in seen, (
            "a hyphenated word ending in 'status' must not turn a prose line into a row"
        )
        by_sweep = sweep_rows(paths)
        assert set(by_sweep) == {"0x2080b201"}, by_sweep
        assert by_sweep["0x2080b201"]["ok"] == 1, by_sweep
        assert by_sweep["0x2080b201"]["octets"] == {
            "0000001f REJECTED (by sweep)": 1,
            "00000000 OK (by sweep)": 1,
        }, by_sweep
        assert all(h.startswith("b.txt:") for h in by_sweep["0x2080b201"]["headers"]), (
            by_sweep
        )
        assert "0x20801303" not in by_sweep, (
            "a row naming its own id must end the header scope, not be read as a sweep row"
        )
        by_label = label_rows(paths)
        assert set(by_label) == {"0x2080206e"}, by_label
        assert by_label["0x2080206e"]["ok"] == 1, by_label
        assert "0x2080206e" not in seen, (
            "a label-only row must not be read as a numeric-id row"
        )
        assert verify_labels(None, None) == [
            "SKIP label->id check: no probe source at None",
            "SKIP define check: no open headers at None",
        ]
        (root / "probe.c").write_text(
            "/* rated-tdp-get-control uses NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL */\n"
        )
        headers = root / "headers"
        headers.mkdir()
        (headers / "ctrl2080perf.h").write_text(
            "#define NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206eU)\n"
        )
        notes = verify_labels(root / "probe.c", headers)
        assert all(note.startswith("ok ") for note in notes), notes
        (headers / "ctrl2080perf.h").write_text(
            "#define NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206fU)\n"
        )
        assert any(
            "MISMATCH" in note for note in verify_labels(root / "probe.c", headers)
        ), "a wrong define must be reported, not passed"
        indirect = banner_dispatches(paths)
        assert set(indirect) == {"0x208090a2"}, indirect
        assert indirect["0x208090a2"]["banners"] == 1, indirect
        table = table_ids(root / "table.tsv")
        assert table == {"0x20800513", "0x20808513", "0x2080ffff"}, table
        sink = io.StringIO()
        real_stdout, sys.stdout = sys.stdout, sink
        try:
            assert report(seen, indirect, by_label, by_sweep, table, None) == 0
            assert (
                report(seen, indirect, by_label, by_sweep, table, "0x20808513") == 1
            ), "the gate must fail on an unread id"
            assert report(seen, indirect, by_label, by_sweep, table, "0x20800513") == 0
        finally:
            sys.stdout = real_stdout
        text = sink.getvalue()
        assert "0x20808513: NEVER DISPATCHED" not in text, text
        assert "in the table, never in a returned status" in text, text
        assert "GATE: 0x20808513 never answered NV_OK" in text, text
        assert "0x208090a2: 1 dump banner(s), no status row" in text, text
        assert "0x2080206e: 1x NV_OK by label" in text, text
        assert "0x2080b201: 1x NV_OK by sweep" in text, text
    print(
        "selftest ok: row detection, id extraction, prose, statusless rows and hyphenated-status words rejected, "
        "dump banners kept as indirect dispatches, label-only rows attributed through a checked map, "
        "thus-scoped sweep rows attributed through their header, "
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
    src: Path | None = None
    ogkm: Path | None = None
    rest = argv[1:]
    while rest:
        flag = rest.pop(0)
        if flag == "--table" and rest:
            table = table_ids(Path(rest.pop(0)))
        elif flag == "--probe-src" and rest:
            src = Path(rest.pop(0))
        elif flag == "--ogkm" and rest:
            ogkm = Path(rest.pop(0))
        elif flag == "--id" and rest:
            only = rest.pop(0).lower()
        else:
            print(f"unknown argument: {flag}", file=sys.stderr)
            return 2
    paths = sorted(p for p in root.rglob("*") if p.suffix in SCANNED_SUFFIXES)
    print(f"scanning {len(paths)} capture files under {root}")
    return report(
        census(paths),
        banner_dispatches(paths),
        label_rows(paths),
        sweep_rows(paths),
        table,
        only,
        verify_labels(src, ogkm),
    )


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
