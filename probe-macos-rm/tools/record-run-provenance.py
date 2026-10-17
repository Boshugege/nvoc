#!/usr/bin/env python3
"""Stamp a run bundle with what produced it, before the run cannot be repeated.

The two decisive runbooks here -- `tools/clock-offset-read-run.sh` and
`tools/clock-offset-write-run.sh` -- capture logs into a timestamped directory
under `outputs/`.  Those runs are the expensive kind: the GSP wedges after some
rejected ids, so a block that answered `NV_OK` once may never answer again in
that session, and a single-domain write is only allowed one step.  A bundle
whose provenance lives in the operator's memory is therefore a bundle that
cannot be reviewed later, and the two things that most easily go wrong are
silent:

* the revision.  `git rev-parse HEAD` is recorded when it can be, next to the
  branch, so an artifact says which code answered -- and an artifact without it
  says so too (`unknown`, with the reason) instead of implying the current tree.
* the *binary*.  This project has already been misled once by a stale
  `probe-macos-rm` that lacked a battery row.  The checkout revision does not
  catch that: a binary built before a fix is still "the right revision" on
  paper.  So the binary's md5 and mtime are recorded, the newest source mtime is
  compared against it, and the freshness line says `STALE` (with the file that
  is newer) or `UNKNOWN` (no binary, or no sources) -- never a bare `OK`.

Nothing here reads a device and nothing here can fail a run: it writes
`README.txt` into the bundle, and in `--sums` mode writes `SHA256SUMS` over the
bundle's files so the quoted bytes can be checked later.

Usage:
    python3 tools/record-run-provenance.py --bundle DIR --label 'clkctl read battery' \\
        [--case 'CASE']... [--id 0x2080901b:2108]... [--preregistered 'CRITERION']... \\
        [--binary PATH] [--source DIR] [--ogkm PATH]
    python3 tools/record-run-provenance.py --bundle DIR --sums
    python3 tools/record-run-provenance.py --selftest
"""

from __future__ import annotations

import datetime
import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path

SOURCE_SUFFIXES = (".c", ".h")
SOURCE_NAMES = ("build.sh", "Makefile")
HEAD_LINE = "run bundle provenance (tools/record-run-provenance.py)"
REMINDER = (
    "a live frequency change is not evidence of a write, and neither is its",
    "absence: the read-back and the restore are. A `0x62` anywhere means the",
    "session is over for this block, so quote the bytes, not the intent.",
)


def utc(ts: float) -> str:
    return datetime.datetime.fromtimestamp(ts, datetime.timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )


def git_facts(root: Path) -> str:
    """`<rev> (<branch>)`, or `unknown (<why>)`.  Never guesses the revision."""
    try:
        rev = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as err:
        return f"unknown ({err.__class__.__name__}: {err})"
    if rev.returncode != 0:
        return f"unknown (git rev-parse exit {rev.returncode})"
    branch = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "--abbrev-ref", "HEAD"],
        capture_output=True,
        text=True,
        timeout=10,
    )
    name = branch.stdout.strip() if branch.returncode == 0 else "?"
    return f"{rev.stdout.strip()} ({name})"


def source_files(source: Path) -> list[Path]:
    if not source.is_dir():
        return []
    found = [
        path
        for path in sorted(source.rglob("*"))
        if path.is_file()
        and (
            path.suffix in SOURCE_SUFFIXES
            or (path.parent == source and path.name in SOURCE_NAMES)
        )
        and "target" not in path.parts
    ]
    return found


def freshness(binary: Path, source: Path) -> str:
    """`FRESH` / `STALE (<file> newer than the binary)` / `UNKNOWN (<why>)`.

    The point is that a stale binary is the failure this line exists for, so a
    missing half is `UNKNOWN` -- never a pass.
    """
    if not binary.is_file():
        return f"UNKNOWN (no binary at {binary})"
    sources = source_files(source)
    if not sources:
        return f"UNKNOWN (no sources under {source})"
    newest = max(sources, key=lambda path: path.stat().st_mtime)
    built = binary.stat().st_mtime
    if newest.stat().st_mtime > built:
        return (
            f"STALE ({newest} {utc(newest.stat().st_mtime)} is newer than the"
            f" binary {utc(built)}) -- rebuild before trusting this run"
        )
    return (
        f"FRESH (binary {utc(built)} is not older than {newest}"
        f" {utc(newest.stat().st_mtime)})"
    )


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_readme(bundle: Path, args: dict) -> int:
    lines = [HEAD_LINE, f"label: {args['label']}"]
    now = datetime.datetime.now()
    lines.append(f"stamp (UTC): {utc(now.timestamp())}")
    lines.append(f"stamp (local): {now.strftime('%Y-%m-%dT%H:%M:%S%z')}")
    lines.append(f"uid: {os.getuid()}" + (" (root)" if os.getuid() == 0 else ""))
    lines.append(
        f"host: {os.uname().nodename} {os.uname().sysname} {os.uname().release}"
    )
    lines.append(f"git: {git_facts(Path(args['root']))}")
    binary = Path(args["binary"])
    if binary.is_file():
        lines.append(
            f"binary: {binary} md5 {hashlib.md5(binary.read_bytes()).hexdigest()}"
            f" mtime {utc(binary.stat().st_mtime)}"
        )
    else:
        lines.append(f"binary: {binary} (missing)")
    lines.append(f"build freshness: {freshness(binary, Path(args['source']))}")
    ogkm = Path(args["ogkm"])
    lines.append(
        f"OGKM: {ogkm} ({'present' if ogkm.is_dir() else 'MISSING'})"
        if args["ogkm"]
        else "OGKM: (not passed)"
    )
    for text in args["case"] or ["(no case recorded)"]:
        lines.append(f"case: {text}")
    for text in args["id"] or ["(no id recorded)"]:
        lines.append(f"id: {text}")
    for text in args["preregistered"] or ["(nothing pre-registered)"]:
        lines.append(f"pre-registered: {text}")
    lines.append("note: " + REMINDER[0])
    lines.extend(f"      {line}" for line in REMINDER[1:])
    target = bundle / "README.txt"
    target.write_text("\n".join(lines) + "\n")
    print(f"provenance -> {target}")
    for line in lines[1:]:
        if line.startswith(("build freshness", "git:", "binary:")):
            print(f"  {line}")
    return 0


def write_sums(bundle: Path) -> int:
    entries = []
    for path in sorted(bundle.iterdir()):
        if not path.is_file() or path.name == "SHA256SUMS":
            continue
        entries.append(f"{digest(path)}  {path.name}")
    target = bundle / "SHA256SUMS"
    header = f"# {HEAD_LINE}, completed {utc(datetime.datetime.now().timestamp())}"
    target.write_text(header + "\n" + "\n".join(entries) + "\n")
    print(f"digests -> {target} ({len(entries)} file(s))")
    return 0


def parse(argv: list[str]) -> dict:
    args = {
        "bundle": None,
        "label": None,
        "sums": False,
        "selftest": False,
        "case": [],
        "id": [],
        "preregistered": [],
        "root": str(Path(__file__).resolve().parents[2]),
        "source": None,
        "binary": None,
        "ogkm": "",
    }
    wants = {
        "--bundle": "bundle",
        "--label": "label",
        "--case": "case",
        "--id": "id",
        "--preregistered": "preregistered",
        "--source": "source",
        "--binary": "binary",
        "--ogkm": "ogkm",
        "--root": "root",
    }
    index = 0
    while index < len(argv):
        token = argv[index]
        if token == "--sums":
            args["sums"] = True
            index += 1
        elif token == "--selftest":
            args["selftest"] = True
            index += 1
        elif token in wants:
            if index + 1 >= len(argv):
                raise ValueError(f"{token} needs a value")
            key = wants[token]
            if isinstance(args[key], list):
                args[key].append(argv[index + 1])
            else:
                args[key] = argv[index + 1]
            index += 2
        else:
            raise ValueError(f"unexpected argument {token!r}")
    if args["source"] is None:
        args["source"] = str(Path(args["root"]) / "probe-macos-rm")
    if args["binary"] is None:
        args["binary"] = str(
            Path(args["root"]) / "probe-macos-rm" / "probe-macos-rm-idle"
        )
    return args


def selftest() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        base = Path(tmp)
        source = base / "probe-macos-rm"
        (source / "src").mkdir(parents=True)
        unit = source / "src" / "nvx_suite.c"
        unit.write_text("int main(void) { return 0; }\n")
        script = source / "build.sh"
        script.write_text("#!/bin/sh\n")
        binary = source / "probe-macos-rm-idle"
        binary.write_text("fake binary\n")

        # A binary built before the sources is exactly what the line is for, and
        # the line has to name the file that is newer -- build.sh counts as a
        # source even without the suffix, so the newest one of the two is what
        # should appear.
        newest_source = max((unit, script), key=lambda path: path.stat().st_mtime)
        os.utime(binary, (newest_source.stat().st_mtime - 100,) * 2)
        verdict = freshness(binary, source)
        assert verdict.startswith("STALE ("), verdict
        # Which of two same-second sources is named depends on iteration order,
        # so the assertion is on the timestamp and on membership, not on a tie.
        assert utc(newest_source.stat().st_mtime) in verdict, verdict
        assert any(str(path) in verdict for path in (unit, script)), verdict
        # Newer than every source: FRESH, and it must not be a bare OK.
        newest = max(unit.stat().st_mtime, script.stat().st_mtime)
        os.utime(binary, (newest + 100,) * 2)
        verdict = freshness(binary, source)
        assert verdict.startswith("FRESH ("), verdict
        # Touching one source past the binary flips it back to STALE.
        os.utime(script, (binary.stat().st_mtime + 200,) * 2)
        verdict = freshness(binary, source)
        assert verdict.startswith("STALE (") and str(script) in verdict, verdict
        # A missing half is UNKNOWN, never a pass.
        assert freshness(source / "gone", source).startswith("UNKNOWN ("), (
            "missing binary"
        )
        empty = base / "empty"
        empty.mkdir()
        assert freshness(binary, empty).startswith("UNKNOWN ("), "missing sources"

        # README mode: the three lines a reviewer needs must be there, and an
        # unknown revision has to say so rather than print the current tree.
        # (The binary is made newest first, so this block tests the fields and
        # not the staleness branch again.)
        os.utime(binary, (script.stat().st_mtime + 300,) * 2)
        bundle = base / "outputs" / "stage-fake-20261018T000000Z"
        bundle.mkdir(parents=True)
        args = parse([
            "--bundle",
            str(bundle),
            "--label",
            "selftest fake run",
            "--case",
            "case one",
            "--id",
            "0x2080901b:2108",
            "--preregistered",
            "a third occurrence at +268 means 128 B",
            "--binary",
            str(binary),
            "--source",
            str(source),
            "--root",
            str(source),
        ])
        assert write_readme(bundle, args) == 0
        text = (bundle / "README.txt").read_text()
        assert HEAD_LINE in text, text
        assert "label: selftest fake run" in text, text
        assert "case: case one" in text and "id: 0x2080901b:2108" in text, text
        assert "pre-registered: a third occurrence at +268 means 128 B" in text, text
        assert "build freshness: FRESH (" in text, text
        assert "git: unknown (" in text, text
        assert "uid: " in text, text

        # Sums mode: the written digests must equal the bytes on disk, and the
        # sums file must not hash itself.
        (bundle / "one.txt").write_text("one\n")
        (bundle / "two.txt").write_text("two\n")
        assert write_sums(bundle) == 0
        lines = [
            line
            for line in (bundle / "SHA256SUMS").read_text().splitlines()
            if line and not line.startswith("#")
        ]
        assert len(lines) == 3, lines  # README.txt, one.txt, two.txt
        for line in lines:
            want, name = line.split("  ", 1)
            assert want == digest(bundle / name), line
            assert name != "SHA256SUMS", line
        assert "one.txt" in (bundle / "SHA256SUMS").read_text()

        # Usage errors are errors, not empty artifacts.
        try:
            parse(["--bundle", str(bundle), "--label"])
            raise AssertionError("a flag without a value was accepted")
        except ValueError as err:
            assert "needs a value" in str(err), err
        try:
            parse(["--label", "x", "--nope", "y"])
            raise AssertionError("an unknown flag was accepted")
        except ValueError as err:
            assert "unexpected argument" in str(err), err
    print(
        "selftest ok: freshness STALE/FRESH/UNKNOWN, README fields, sums round-trip,"
        " argument rejection"
    )
    return 0


def main(argv: list[str]) -> int:
    try:
        args = parse(argv)
    except ValueError as err:
        print(f"usage error: {err}", file=sys.stderr)
        return 2
    if args["selftest"]:
        return selftest()
    if args["bundle"] is None:
        print(__doc__)
        return 2
    bundle = Path(args["bundle"])
    if not bundle.is_dir():
        print(f"no bundle directory at {bundle}", file=sys.stderr)
        return 3
    if args["sums"]:
        return write_sums(bundle)
    if args["label"] is None:
        print("--label is required unless --sums is given", file=sys.stderr)
        return 2
    return write_readme(bundle, args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
