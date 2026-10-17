#!/usr/bin/env python3
"""Check that the macOS RM entry's allow-list and the feature table agree.

The defect this exists for is on the record: the command table said
`get-pstate-global-freq-offset` was "未实现：macOS RM 在访问驱动前拒绝" while the
CLI allowed it and the same document described its read in detail. A blanket
refusal that covers a command the entry runs is wrong in the one direction a
reader cannot check by eye, and it is wrong in a way that matters: it tells a
user the backend cannot do something it can.

So the check is mechanical, from the two sources that decide the answer:

* the allow-list, read out of the CLI's own refusal path (`validate`'s
  `matches!` on `Command` variants), which is what actually runs;
* the user-facing list in that same refusal message, which must be the same set
  -- a message that omits an allowed command or lists a refused one sends users
  to the wrong place;
* every table row in the feature matrix, so an allowed command sitting behind
  the blanket refusal is reported with its line number.

What it does **not** check: whether the *prose* of a non-allowed command's row is
accurate. That text is free-form (it names RM statuses, sizes and preconditions)
and judging it is the claim audit's job, not this one's. This tool only fails on
the two set-level contradictions above.

Prototype note: this tool is developed and run on the client. It reads files, so
it needs no device and no GPU.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# The blanket refusal. A row carrying it claims the entry refuses before the
# driver is opened, which is checkable against the allow-list; nothing else in
# the status column is, so only this phrase gates.
BLANKET = "未实现：macOS RM 在访问驱动前拒绝"

# `Command::GetInfo` inside the `matches!` of the refusal path.
VARIANT_RE = re.compile(r"Command::([A-Za-z0-9]+)")
# The command string a variant is dispatched as, from the command table.
SPEC_WINDOW = 400
SPEC_RE = re.compile(r'CommandSpec::new\(\s*"([a-z0-9][a-z0-9-]*)"')
MESSAGE_RE = re.compile(r"supported commands: ([^.]+)\.")
ROW_RE = re.compile(r"^\|\s*`([a-z0-9][a-z0-9-]*)`\s*\|")


def allow_list(cli_src: str) -> list[str]:
    """The command variants the macOS RM path lets through, in source order."""
    start = cli_src.find("if matches!(")
    if start < 0:
        return []
    end = cli_src.find(") {", start)
    if end < 0:
        return []
    seen: list[str] = []
    for name in VARIANT_RE.findall(cli_src[start:end]):
        if name not in seen:
            seen.append(name)
    return seen


def variant_names(cli_lib: str) -> dict[str, str]:
    """Variant -> command string, from the command table."""
    mapping: dict[str, str] = {}
    for match in VARIANT_RE.finditer(cli_lib):
        window = cli_lib[match.end() : match.end() + SPEC_WINDOW]
        spec = SPEC_RE.search(window)
        if spec:
            mapping.setdefault(match.group(1), spec.group(1))
    return mapping


def message_list(cli_src: str) -> list[str]:
    """The commands the refusal message names as supported."""
    match = MESSAGE_RE.search(cli_src)
    if match is None:
        return []
    return [part.strip() for part in match.group(1).split(",") if part.strip()]


def table_rows(matrix: str) -> list[tuple[int, str, str]]:
    """(line number, command, status cell) for every backticked table row."""
    rows: list[tuple[int, str, str]] = []
    for number, line in enumerate(matrix.splitlines(), 1):
        if ROW_RE.match(line) is None:
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 2:
            continue
        command = cells[0].strip("` ")
        rows.append((number, command, cells[-1]))
    return rows


def verdicts(cli_src: str, cli_lib: str, matrix: str) -> list[tuple[bool, str]]:
    """Every check, as (ok, sentence). Pure: the caller does the printing."""
    out: list[tuple[bool, str]] = []
    variants = allow_list(cli_src)
    if not variants:
        return [(False, "the refusal path lists no allowed command at all")]
    mapping = variant_names(cli_lib)
    unmapped = [name for name in variants if name not in mapping]
    if unmapped:
        out.append((
            False,
            "these allowed variants have no command string in the table: "
            + ", ".join(unmapped),
        ))
    allowed = {mapping[name] for name in variants if name in mapping}

    named = set(message_list(cli_src))
    for command in sorted(allowed - named):
        out.append((
            False,
            f"the entry allows {command} but the refusal message omits it",
        ))
    for command in sorted(named - allowed):
        out.append((
            False,
            f"the refusal message offers {command} but the entry refuses it",
        ))

    rows = table_rows(matrix)
    if not rows:
        out.append((False, "the feature table has no command rows to read"))
    behind = [
        (number, command)
        for number, command, status in rows
        if command in allowed and status.startswith(BLANKET)
    ]
    for number, command in behind:
        out.append((
            False,
            f"matrix:{number} {command} carries the blanket refusal while the entry allows it",
        ))
    if not behind:
        out.append((
            True,
            f"{len(allowed)} allowed command(s); none of them is behind the blanket refusal "
            f"({len(rows)} table rows read)",
        ))
    # Informational, never gating: a command the entry allows and the table has
    # no row for is a documentation gap, not a contradiction.
    absent = sorted(allowed - {command for _, command, _ in rows})
    if absent:
        out.append((
            True,
            "informational: allowed but absent from the table: " + ", ".join(absent),
        ))
    return out


def selftest() -> int:
    """The checks against small inputs, including the defect that motivated them."""
    cli = """pub(super) fn validate(command: Command) -> CliResult<()> {
    if matches!(
        command,
        Command::GetInfo
            | Command::GetPstateGlobalFreqOffset
    ) {
        Ok(())
    } else {
        Err(CliError::new(format!(
            "{} is unavailable on macOS RM; supported commands: get-info, get-pstate-global-freq-offset. Performance writes stay gated.",
            command.name()
        )))
    }
}"""
    lib = """(Command::GetInfo, CommandSpec::new("get-info", Group::Info, "Read")),
        (Command::GetPstateGlobalFreqOffset, CommandSpec::new(
            "get-pstate-global-freq-offset", Group::Clock, "Read",
        )),"""
    clean = "| `get-info` | Read identity | **已实现且实测（读面）**：入口允许 |\n"
    dirty_row = (
        f"| `get-pstate-global-freq-offset` | Read clock offset in MHz | {BLANKET} |\n"
    )

    ok, why = verdicts(cli, lib, clean)[-1]
    assert ok, why
    assert "get-pstate-global-freq-offset" in why, why

    checks = verdicts(cli, lib, clean + dirty_row)
    bad = [why for ok, why in checks if not ok]
    assert any("carries the blanket refusal" in why for why in bad), bad
    # The motivating defect, verbatim: the row above is exactly what the matrix
    # said before it was fixed, and the tool must fail on it.
    assert "matrix:2" in " ".join(bad), bad

    # A message that omits an allowed command contradicts the behaviour.
    thin = cli.replace(
        "supported commands: get-info, get-pstate-global-freq-offset",
        "supported commands: get-info",
    )
    assert any(
        "refusal message omits it" in why
        for ok, why in verdicts(thin, lib, clean)
        if not ok
    )

    # ... and one that offers a command the entry refuses.
    fat = cli.replace(
        "supported commands: get-info,", "supported commands: get-info, get-fan-info,"
    )
    assert any(
        "but the entry refuses it" in why
        for ok, why in verdicts(fat, lib, clean)
        if not ok
    )

    # A variant with no command string must be reported, not skipped silently.
    assert any(
        "no command string" in why for ok, why in verdicts(cli, "", clean) if not ok
    )

    # An allow-list that cannot be read at all is a failure, not a pass.
    assert verdicts("pub(super) fn validate() {}", lib, clean)[0][0] is False

    # A table with no command rows cannot certify anything.
    assert any(
        "no command rows" in why
        for ok, why in verdicts(cli, lib, "# nothing\n")
        if not ok
    )

    print(
        "selftest ok: allow-list, message agreement, blanket-refusal rows, empty inputs"
    )
    return 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()

    root = Path(argv[0]).parent if argv else Path(".")
    argv = argv[1:] if argv else []
    cli_src = Path("cli/src/macos_rm.rs")
    cli_lib = Path("cli/src/lib.rs")
    matrix = Path("docs/design/macos-backend-feature-matrix.md")
    rest = []
    while argv:
        arg = argv.pop(0)
        if arg == "--cli-src" and argv:
            cli_src = Path(argv.pop(0))
        elif arg == "--cli-lib" and argv:
            cli_lib = Path(argv.pop(0))
        elif arg == "--matrix" and argv:
            matrix = Path(argv.pop(0))
        else:
            rest.append(arg)
    if rest:
        matrix = Path(rest[0])

    for path in (cli_src, cli_lib, matrix):
        if not path.is_file():
            print(f"SKIP: {path} not found", file=sys.stderr)
            return 2
    checks = verdicts(cli_src.read_text(), cli_lib.read_text(), matrix.read_text())
    failed = 0
    for ok, why in checks:
        print(f"  {'ok  ' if ok else 'FAIL'} {why}")
        failed += 0 if ok else 1
    print(
        f"\nentry parity: {len(checks) - failed} ok, {failed} failed "
        f"({cli_src}, {cli_lib}, {matrix})"
    )
    _ = root
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
