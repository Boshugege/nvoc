#!/usr/bin/env python3
"""Check the docs' per-id claims against the captures instead of against prose.

Round 83 found two claims in `docs/design/macos-backend-feature-matrix.md` that had
no artifact behind them: a measured fan status whose raw row was never saved, and
an NV_OK attributed to the wrong id (the legacy alias instead of the public one).
Both were found by hand. This tool finds that class of error mechanically.

It reads markdown (the feature matrix, the reviews, the reverse-engineering notes)
and, for every claim about an RM control id -- "measured", "never issued", "answered
NV_OK", "answered 0x62" -- asks the census produced by
`probe-macos-rm/tools/audit-live-attempts.py` what the captures actually show:

  * CONTRADICTED   the claim and the captures disagree (a status the captures never
                   recorded, or "never issued" for an id that has status rows);
  * NO ARTIFACT    the id has no status row at all, so the claim is prose-only --
                   this is the fan case from round 83, and it is not proof of an
                   error, but it is proof that the claim cannot be checked yet;
  * SUPPORTED      the captures contain what the claim says.

The association between a claim and an id is a proximity heuristic on one line (the
nearest id before the claim), and every reported item quotes the raw claim text, so
a human can re-read it. The point is to make the review list short and specific,
not to be clever: an item this tool cannot decide is printed, never silently passed.

Usage:
    python3 tools/audit-matrix-claims.py --census <census.txt> <file.md> [<file.md> ...]
    python3 tools/audit-matrix-claims.py --selftest

Exit status: 0 when nothing contradicts, 1 when a claim is contradicted, 2 when the
census was missing or unreadable (a claim must never pass silently).
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

ID_RE = re.compile(r"0x(2080[0-9a-f]{4})", re.I)
NEVER_LINE_RE = re.compile(r"never in a returned status")
INDIRECT_ROW_RE = re.compile(r"^\d+ dump banner\(s\), no status row")
# A claim is attached inside one sentence, clause or table cell, never across
# one, and a segment naming more ids than this is too dense to guess at.
SEGMENT_RE = re.compile(r"[；;。|、，,]")
MAX_IDS_PER_SEGMENT = 3
RETRACTED_RE = re.compile(r"[✗✘]|曾写|误记|笔误|retract|superseded|更正一|was wrong")
NEG_PREFIX_RE = re.compile(r"(?:0\s*[x×]|no\s|无\s?|没有)\s*$")
# A claim the doc itself marks as unverifiable must name the missing artifact in
# the same segment: the class is listed and kept out of the gate, so it cannot be
# used to hide a claim, only to admit a gap in the captures.
UNVERIFIABLE_RE = re.compile(
    r"产物未存|产物缺失|无法归属|no captured row|artifact not kept"
)
CENSUS_ROW_RE = re.compile(r"^\s*(0x[0-9a-f]{8}):\s*(.+?)\s*$", re.I)
CENSUS_OK_RE = re.compile(r"^(\d+)x NV_OK")
CENSUS_CODE_RE = re.compile(r"\b([0-9a-f]{8})\s+[A-Z_]+")
CENSUS_ALL_OK_RE = re.compile(r"^(\d+)x NV_OK;?\s*$")

# A status the docs name as "0x62" is a code; the census prints eight digits.
CODE_MAP = {
    "0x1b": "0000001b",
    "0x1f": "0000001f",
    "0x25": "00000025",
    "0x3a": "0000003a",
    "0x56": "00000056",
    "0x57": "00000057",
    "0x62": "00000062",
    "0x65": "00000065",
}

CLAIMS = (
    (
        "never-dispatched",
        re.compile(r"从未(?:被)?发|从未出现在|never in a returned status"),
    ),
    ("nv-ok-absent", re.compile(r"(?:0\s*[x×]|no\s|无\s?|没有)NV_OK")),
    ("nv-ok", re.compile(r"NV_OK")),
    ("indirect", re.compile(r"间接|indirect", re.I)),
    ("code-0x1b", re.compile(r"(?<![0-9a-f])0x1b(?![0-9a-f])", re.I)),
    ("code-0x1f", re.compile(r"(?<![0-9a-f])0x1f(?![0-9a-f])", re.I)),
    ("code-0x25", re.compile(r"(?<![0-9a-f])0x25(?![0-9a-f])", re.I)),
    ("code-0x3a", re.compile(r"(?<![0-9a-f])0x3a(?![0-9a-f])", re.I)),
    ("code-0x56", re.compile(r"(?<![0-9a-f])0x56(?![0-9a-f])", re.I)),
    ("code-0x57", re.compile(r"(?<![0-9a-f])0x57(?![0-9a-f])", re.I)),
    ("code-0x62", re.compile(r"(?<![0-9a-f])0x62(?![0-9a-f])", re.I)),
    ("code-0x65", re.compile(r"(?<![0-9a-f])0x65(?![0-9a-f])", re.I)),
    ("measured", re.compile(r"实测|measured")),
)


def census_facts(text: str) -> dict[str, dict[str, object]]:
    """id -> {dispatched, ok, codes, indirect} from the census tool's own output.

    The census keeps dispatches whose answer was never printed in their own tier
    ("N dump banner(s), no status row"); those rows are dispatches with no octet, so
    they set dispatched without touching ok or codes, and they carry indirect=True.
    """
    facts: dict[str, dict[str, object]] = {}
    for line in text.splitlines():
        row = CENSUS_ROW_RE.match(line)
        if not row:
            continue
        cid = row.group(1).lower()
        rest = row.group(2)
        if INDIRECT_ROW_RE.search(rest):
            entry = facts.setdefault(
                cid, {"dispatched": True, "ok": 0, "codes": set(), "indirect": False}
            )
            entry["dispatched"] = True
            entry["indirect"] = True
            continue
        if NEVER_LINE_RE.search(rest):
            facts[cid] = {
                "dispatched": False,
                "ok": 0,
                "codes": set(),
                "indirect": False,
            }
            continue
        ok_match = CENSUS_OK_RE.match(rest)
        # An id can have plain rows and rows that name it by label; both are its
        # evidence, so the counts merge instead of the last section winning.
        before = facts.get(cid)
        prev_codes = set(before["codes"]) if before else set()
        prev_indirect = bool(before["indirect"]) if before else False
        prev_ok = int(before["ok"]) if before else 0  # type: ignore[arg-type]
        facts[cid] = {
            "dispatched": True,
            "ok": max(prev_ok, int(ok_match.group(1)) if ok_match else 0),
            "codes": prev_codes | set(CENSUS_CODE_RE.findall(rest)),
            "indirect": prev_indirect,
        }
    return facts


def claims_in(text: str) -> tuple[list[tuple[int, str, str, str]], int]:
    """(items, dense segments skipped) for the id-claims in `text`.

    A claim is attached to the nearest preceding id *within the same segment*,
    where a segment ends at a sentence, clause or cell separator.  Line-level
    association was the first attempt and it produced mostly false positives: a
    summary row for the whole public control surface mentions eight ids and a dozen
    status codes, and every code got attached to whichever id happened to precede
    it.  A segment that names more than MAX_IDS_PER_SEGMENT ids is skipped rather
    than guessed at, and the skip is counted so the report never quietly loses it.
    """
    out: list[tuple[int, str, str, str]] = []
    skipped = 0
    seen: set[tuple[int, str, str]] = set()
    for lineno, line in enumerate(text.splitlines(), 1):
        for segment in SEGMENT_RE.split(line):
            ids = [
                (m.start(), "0x" + m.group(1).lower()) for m in ID_RE.finditer(segment)
            ]
            if not ids:
                continue
            if len({cid for _, cid in ids}) > MAX_IDS_PER_SEGMENT:
                skipped += 1
                continue
            # A segment that quotes a claim in order to retract it is not the claim:
            # the docs' own correction sections quote the wrong sentence and mark it.
            retracted = bool(RETRACTED_RE.search(segment))
            unverifiable = bool(UNVERIFIABLE_RE.search(segment))
            if retracted:
                mark = "retracted"
            elif unverifiable:
                mark = "unverifiable"
            else:
                mark = ""
            for kind, pattern in CLAIMS:
                for claim in pattern.finditer(segment):
                    if kind == "nv-ok" and NEG_PREFIX_RE.search(
                        segment[max(0, claim.start() - 8) : claim.start()]
                    ):
                        # "0x NV_OK" / "no NV_OK" is handled as nv-ok-absent; without
                        # this it was also read as a claim that NV_OK had been seen.
                        continue
                    before = [cid for pos, cid in ids if pos < claim.start()]
                    if not before:
                        continue
                    item = (lineno, before[-1], mark or kind)
                    if item in seen:
                        continue
                    seen.add(item)
                    out.append((*item, segment.strip()))
    return out, skipped


def verdict(kind: str, facts: dict[str, dict[str, object]] | None) -> tuple[str, str]:
    """(verdict, explanation) for one claim against one id's census facts."""
    if kind == "retracted":
        # A claim quoted in order to correct it is a record of a past mistake, not a
        # statement about the captures, so it is listed and kept out of the gate.
        return "RETRACTED", "quoted in a correction, not asserted"
    if kind == "unverifiable":
        # Same idea with a different reason: the claim may well be true, but the run
        # behind it left no row in outputs/, and the sentence says so.
        return "UNVERIFIABLE", "the segment names the missing artifact"
    if kind == "never-dispatched":
        # An id the census does not mention at all has no status row by definition,
        # so "never dispatched" is what the captures show for it too. That is a
        # different question from "does this claim have an artifact", which is what
        # the other kinds ask.
        if facts is None:
            return "SUPPORTED", "the census has no status row for it at all"
        assert isinstance(facts, dict)
        if facts["dispatched"]:
            kind_word = "dump banner(s)" if facts["indirect"] else "status rows"
            return "CONTRADICTED", (
                f"the captures have {kind_word} for it (NV_OK {int(facts['ok'])})"  # type: ignore[arg-type]
            )
        return "SUPPORTED", "no status row names it"
    if facts is None:
        return "NO ARTIFACT", "no status row names this id in any capture"
    assert isinstance(facts, dict)
    dispatched = bool(facts["dispatched"])
    ok = int(facts["ok"])  # type: ignore[arg-type]
    codes = facts["codes"]
    assert isinstance(codes, set)
    if not dispatched:
        return "NO ARTIFACT", "nothing recorded for this id at all"
    if kind == "nv-ok":
        if ok:
            return "SUPPORTED", f"NV_OK recorded {ok} time(s)"
        if facts["indirect"]:
            return "CONTRADICTED", (
                "no printed NV_OK row: only a dump banner, so say it was indirect"
            )
        return "CONTRADICTED", "zero NV_OK rows, only rejections"
    if kind == "nv-ok-absent":
        if ok:
            return "CONTRADICTED", f"the captures have {ok} NV_OK row(s) for it"
        return "SUPPORTED", "no NV_OK row, so stating its absence is right"
    if kind == "indirect":
        # An accept claimed as indirect needs the dump-banner tier; a printed NV_OK
        # is stronger evidence for the same conclusion, so it supports the claim too.
        if facts["indirect"]:
            return "SUPPORTED", "a dump banner with no printed status backs it"
        if ok:
            return "SUPPORTED", f"not the weak tier, but NV_OK is recorded {ok} time(s)"
        return "CONTRADICTED", "neither a dump banner nor a printed NV_OK row"
    if kind == "measured":
        return "SUPPORTED", "the id has at least one returned status"
    code = CODE_MAP[kind.split("-", 1)[1]]
    if code in codes:
        return "SUPPORTED", f"the captures contain {code}"
    return "CONTRADICTED", f"no {code} row among {sorted(codes) or ['(none)']}"


def audit(paths: list[Path], census_text: str) -> int:
    facts = census_facts(census_text)
    counts = {
        "SUPPORTED": 0,
        "NO ARTIFACT": 0,
        "CONTRADICTED": 0,
        "RETRACTED": 0,
        "UNVERIFIABLE": 0,
    }
    print(f"census ids with facts: {len(facts)}")
    for path in paths:
        items, dense = claims_in(path.read_text(errors="replace"))
        if not items and not dense:
            print(f"{path}: no id-claims found")
            continue
        print(
            f"\n{path}: {len(items)} id-claim(s); "
            f"{dense} dense segment(s) skipped (more than {MAX_IDS_PER_SEGMENT} ids)"
        )
        for lineno, cid, kind, raw in items:
            what, why = verdict(kind, facts.get(cid))
            counts[what] += 1
            if what == "SUPPORTED":
                continue
            print(f"  {what:13} {path}:{lineno} {cid} [{kind}] -- {why}")
            print(f"                {raw[:150]}")
    print(
        f"\nclaims: {counts['SUPPORTED']} supported, "
        f"{counts['NO ARTIFACT']} with no artifact, {counts['CONTRADICTED']} contradicted, "
        f"{counts['RETRACTED']} quoted as retracted, "
        f"{counts['UNVERIFIABLE']} marked unverifiable"
    )
    # Only a contradiction about the captures fails the gate: a retraction and a
    # claim that names its own missing artifact are records, and are reported.
    return 1 if counts["CONTRADICTED"] else 0


def selftest() -> int:
    census = (
        "scanning 3 capture files\n"
        "  0x20808532: 0x NV_OK; 2x 00000062 REJECTED NV_ERR_RESET_REQUIRED  [a.txt]\n"
        "  0x20808513: 0x NV_OK; 2x 00000062 REJECTED NV_ERR_RESET_REQUIRED  [a.txt]\n"
        "  0x20800513: 3x NV_OK; 3x 00000000 OK NV_OK  [a.txt]\n"
        "  0x20808540: in the table, never in a returned status\n"
        "  0x208090a2: 1 dump banner(s), no status row  [tierb.txt]\n"
        "  0x2080b201: 0x NV_OK; 1x 0000001f REJECTED NV_ERR_INVALID_ARGUMENT  [t.txt]\n"
    )
    doc = (
        "1 `0x20808532` measured 0x62 NV_ERR_RESET_REQUIRED\n"  # supported
        "2 `0x20808513` 已实测 NV_OK\n"  # contradicted: zero NV_OK rows
        "3 `0x20800532` 实测 0x56\n"  # no artifact: nothing recorded for it
        "4 `0x20808540` 从未发\n"  # supported
        "5 `0x20800513` 从未发\n"  # contradicted: it has NV_OK rows
        "6 `0x20809026` 怎么读都行\n"  # no claim keyword -> not an item
        "7 see 0x20809019 above\n"  # cannot be decided, and that is printed not passed
        "8 `0x208090a2` 以 524 B 被接受（间接）\n"  # supported by the dump-banner tier
        "9 `0x208090a2` NV_OK\n"  # contradicted: no printed octet, say indirect
        "10 `0x208090a2` 从未发\n"  # contradicted: it was dispatched
        "11 `0x2080b201` 间接接受\n"  # contradicted: only 0x1f rows
        "12 `0x20808532` 0×NV_OK（2×`0x62`）\n"  # supported: a stated absence
        "13 `0x20800513` 0×NV_OK\n"  # contradicted: it has NV_OK rows
        "14 `0x20808513` 曾写“已实测 NV_OK” ✗\n"  # retracted, not gated
        "15 `0x20809999` 接受 NV_OK（产物未存）\n"  # unverifiable, not gated
    )
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "doc.md"
        path.write_text(doc)
        facts = census_facts(census)
        assert facts["0x20808532"] == {
            "dispatched": True,
            "ok": 0,
            "codes": {"00000062"},
            "indirect": False,
        }, facts["0x20808532"]
        assert facts["0x20808540"]["dispatched"] is False
        assert facts["0x20800513"]["ok"] == 3
        assert facts["0x208090a2"] == {
            "dispatched": True,
            "ok": 0,
            "codes": set(),
            "indirect": True,
        }, facts["0x208090a2"]
        items, _ = claims_in(doc)
        assert not any(cid == "0x20809026" for _, cid, _, _ in items), items
        assert verdict("code-0x62", facts["0x20808532"])[0] == "SUPPORTED"
        assert verdict("nv-ok", facts["0x20808513"])[0] == "CONTRADICTED"
        assert verdict("code-0x56", facts.get("0x20800532"))[0] == "NO ARTIFACT"
        assert verdict("never-dispatched", facts["0x20808540"])[0] == "SUPPORTED"
        assert verdict("never-dispatched", facts["0x20800513"])[0] == "CONTRADICTED"
        assert verdict("never-dispatched", facts.get("0x2080122a"))[0] == "SUPPORTED"
        assert verdict("indirect", facts["0x208090a2"])[0] == "SUPPORTED"
        assert verdict("nv-ok", facts["0x208090a2"])[0] == "CONTRADICTED", (
            "a dump banner is not a printed NV_OK and must not be reported as one"
        )
        assert verdict("never-dispatched", facts["0x208090a2"])[0] == "CONTRADICTED", (
            "the id was sent, so 'never dispatched' is wrong for it"
        )
        assert verdict("indirect", facts["0x2080b201"])[0] == "CONTRADICTED", (
            "rejections only: nothing backs an accept"
        )
        assert verdict("nv-ok-absent", facts["0x20808532"])[0] == "SUPPORTED", (
            "stating that NV_OK never came back must not be read as claiming it did"
        )
        assert verdict("nv-ok-absent", facts["0x20800513"])[0] == "CONTRADICTED"
        assert verdict("retracted", None)[0] == "RETRACTED"
        assert verdict("unverifiable", facts.get("0x20809999"))[0] == "UNVERIFIABLE"
        kinds = {(cid, kind) for _, cid, kind, _ in items}
        assert ("0x20809999", "unverifiable") in kinds, kinds
        assert ("0x20808532", "nv-ok-absent") in kinds, kinds
        assert not any(
            cid == "0x20808532" and kind == "nv-ok" for cid, kind in kinds
        ), "a negated NV_OK must not also be read as a positive claim"
        assert ("0x20808513", "retracted") in kinds, kinds
        assert ("0x20808513", "nv-ok") in kinds, (
            "line 2 of the sample doc asserts NV_OK for the same id and is still checked"
        )
        # The exit code has to separate the two failures it is meant to find.
        import io

        sink = io.StringIO()
        real, sys.stdout = sys.stdout, sink
        try:
            rc = audit([path], census)
        finally:
            sys.stdout = real
        out = sink.getvalue()
        assert rc == 1, out
        assert "NO ARTIFACT" in out and "CONTRADICTED" in out, out
    print(
        "selftest ok: claim extraction, proximity, no-keyword lines ignored, "
        "supported / no-artifact / contradicted verdicts, census parsing, exit code"
    )
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("--selftest", "-h", "--help"):
        if argv and argv[0] == "--selftest":
            return selftest()
        print(__doc__)
        return 0
    census_path = None
    files: list[Path] = []
    rest = list(argv)
    while rest:
        arg = rest.pop(0)
        if arg == "--census" and rest:
            census_path = Path(rest.pop(0))
        else:
            files.append(Path(arg))
    if census_path is None:
        print(
            "SKIP: --census <census.txt> is required, no claim is checked",
            file=sys.stderr,
        )
        return 2
    if not census_path.is_file():
        print(f"SKIP: {census_path} is not a file", file=sys.stderr)
        return 2
    missing = [f for f in files if not f.is_file()]
    if missing or not files:
        print(f"SKIP: no readable markdown input ({missing})", file=sys.stderr)
        return 2
    return audit(files, census_path.read_text(errors="replace"))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
