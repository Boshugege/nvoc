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

# The word the matrix uses for "this backend does it". Only used for the
# informational mirror check above, never for a verdict.
IMPLEMENTED = "已实现"

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


# The §一 command table's own structure: a group heading states how many commands
# it lists (`### …（`Group`，N 项）`), and the rows under it are that group's
# commands. The table names `cli/src/lib.rs`'s `CommandSpec` as its source, so
# every one of those headings is a claim about the code.
SECTION_RE = re.compile(r"^## 一、")
HEADLINE_RE = re.compile(r"^## 一、CLI 命令表（(\d+) 条", re.M)
HEADING_RE = re.compile(r"^### .*（`([A-Za-z]+)`，(\d+) 项）")
GROUP_SPEC_RE = re.compile(
    r'CommandSpec::new\(\s*"([a-z0-9][a-z0-9-]*)"\s*,\s*Group::([A-Za-z]+)'
)


def command_table(matrix: str) -> tuple[dict[str, list[str]], dict[str, int]]:
    """The §一 table as (group -> commands, group -> count the heading claims)."""
    listed: dict[str, list[str]] = {}
    claimed: dict[str, int] = {}
    current: str | None = None
    inside = False
    for line in matrix.splitlines():
        if not inside:
            inside = SECTION_RE.match(line) is not None
            continue
        if line.startswith("## "):
            break
        heading = HEADING_RE.match(line)
        if heading:
            current = heading.group(1)
            claimed[current] = int(heading.group(2))
            listed.setdefault(current, [])
            continue
        row = ROW_RE.match(line)
        if row is not None and current is not None:
            listed[current].append(row.group(1))
    return listed, claimed


def table_completeness(cli_lib: str, matrix: str) -> list[tuple[bool, str]]:
    """Every command in the code's spec table must have a row, and a group heading
    must count the rows under it.

    This is the check nothing else here can make, because every other check walks
    the table's rows: a command the table simply omits is invisible to all of
    them, and a heading that undercounts is self-consistently wrong. `list` was
    exactly that -- a real command, in the code, absent from the table, with the
    heading claiming 101 for a table that described 102.
    """
    specs: dict[str, list[str]] = {}
    for command, group in GROUP_SPEC_RE.findall(cli_lib):
        specs.setdefault(group, []).append(command)
    if not specs:
        return [
            (
                False,
                "the CLI's CommandSpec table has no parseable (command, group) pair",
            )
        ]
    listed, claimed = command_table(matrix)
    if not listed:
        return [(False, "the §一 command table has no group heading to read")]
    out: list[tuple[bool, str]] = []
    for group in sorted(set(specs) | set(listed)):
        in_code = sorted(set(specs.get(group, [])))
        in_table = listed.get(group, [])
        missing = [command for command in in_code if command not in in_table]
        extra = sorted({command for command in in_table if command not in in_code})
        if missing:
            out.append((
                False,
                f"§一 {group}: the code declares {len(in_code)} command(s); the table omits "
                + ", ".join(missing),
            ))
        if extra:
            out.append((
                False,
                f"§一 {group}: the table lists "
                + ", ".join(extra)
                + " which the code's CommandSpec table does not declare",
            ))
        if group in claimed and claimed[group] != len(in_table):
            out.append((
                False,
                f"§一 {group}: the heading claims {claimed[group]} 项 but "
                f"{len(in_table)} row(s) follow it",
            ))
    if not out:
        out.append((
            True,
            f"the §一 table matches the code: {sum(len(v) for v in specs.values())} spec(s) "
            f"over {len(specs)} group(s), and every heading counts its rows",
        ))
    # The section's own headline is the same claim in one number ("101 条"), and
    # it was the number that was wrong about a code table of 102.
    headline = HEADLINE_RE.search(matrix)
    if headline is None:
        out.append((False, "the §一 heading states no command count to check"))
    else:
        stated = int(headline.group(1))
        declared = sum(len(v) for v in specs.values())
        rows = sum(len(v) for v in listed.values())
        if stated != declared or stated != rows:
            out.append((
                False,
                f"§一 states {stated} 条 but the code declares {declared} and the table "
                f"has {rows} row(s)",
            ))
    return out


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
    out.extend(table_completeness(cli_lib, matrix))
    # Informational, never gating: a command the entry allows and the table has
    # no row for is a documentation gap, not a contradiction.
    absent = sorted(allowed - {command for _, command, _ in rows})
    if absent:
        out.append((
            True,
            "informational: allowed but absent from the table: " + ", ".join(absent),
        ))
    # The mirror direction, also informational. A row can claim work is
    # implemented while the entry refuses the command -- which is the same defect
    # from the other side -- but "implemented" is prose here, so this cannot gate.
    # It is still worth printing: today it names one row, and that row is right
    # for a reason a reader can check (`cli/src/lib.rs` short-circuits
    # `get-vbios -i <file>` before the backend is chosen, so the offline parse
    # never reaches the refusal at all). A new name appearing here is a question
    # to answer, not a failure.
    claims = [
        (number, command)
        for number, command, status in rows
        if command not in allowed and IMPLEMENTED in status
    ]
    if claims:
        out.append((
            True,
            "informational: rows claiming implemented while the entry refuses the command: "
            + ", ".join(f"matrix:{number} {command}" for number, command in claims),
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
    # The table has to be a consistent table for the completeness check to pass:
    # a §一 section, a heading per group that counts its rows, and the rows
    # themselves. That is the shape the real file has, and the shape `list` was
    # missing from.
    # The real file has sections before §一, so the fixture does too: a check that
    # only worked at the start of the file would pass here and fail there.
    head = (
        "# Feature matrix\n\n## 状态图例\n\nlegend\n\n"
        "## 一、CLI 命令表（2 条，来源：`cli/src/lib.rs` 的 `CommandSpec`）\n\n"
    )
    info_row = "| `get-info` | Read identity | **已实现且实测（读面）**：入口允许 |\n"
    offset_row = "| `get-pstate-global-freq-offset` | Read clock offset in MHz | 读面入口允许 |\n"
    clean = (
        head
        + "### GPU 与显示信息（`Info`，1 项）\n\n"
        + "| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |\n| --- | --- | --- |\n"
        + info_row
        + "\n### 时钟、偏移与 P-State 锁（`Clock`，1 项）\n\n"
        + "| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |\n| --- | --- | --- |\n"
        + offset_row
    )
    dirty_row = (
        f"| `get-pstate-global-freq-offset` | Read clock offset in MHz | {BLANKET} |\n"
    )
    # Same table, with the offset row's status swapped for the blanket refusal.
    dirty = clean.replace(offset_row, dirty_row)
    assert dirty_row in dirty and dirty != clean

    clean_checks = verdicts(cli, lib, clean)
    assert all(ok for ok, _ in clean_checks), [
        why for ok, why in clean_checks if not ok
    ]

    checks = verdicts(cli, lib, dirty)
    bad = [why for ok, why in checks if not ok]
    assert any("carries the blanket refusal" in why for why in bad), bad
    # The motivating defect, verbatim: the row above is exactly what the matrix
    # said before it was fixed, and the tool must fail on it. Its line number is
    # computed, not hard-coded, so the fixture can grow.
    assert f"matrix:{dirty.splitlines().index(dirty_row.strip()) + 1}" in " ".join(
        bad
    ), bad

    # A command the code declares and the table omits is invisible to every
    # other check here: only the completeness check can see it. This is `list`
    # to the letter -- present in the code, absent from the table. The
    # informational check says the same thing without failing, which is what it
    # is for; the completeness check is what makes it an error.
    partial = clean.split("### 时钟、偏移与 P-State 锁")[0]
    partial_checks = verdicts(cli, lib, partial)
    assert any(
        "the table omits get-pstate-global-freq-offset" in why
        for ok, why in partial_checks
        if not ok
    ), [why for ok, why in partial_checks if not ok]
    assert any(
        ok and "informational" in why and "get-pstate-global-freq-offset" in why
        for ok, why in partial_checks
    ), partial_checks

    # A heading that claims more than the rows under it is the same defect stated
    # in numbers, so it must fail even when every name is present.
    undercounted = clean.replace("（`Info`，1 项）", "（`Info`，2 项）")
    assert any(
        "the heading claims 2 项" in why
        for ok, why in verdicts(cli, lib, undercounted)
        if not ok
    ), verdicts(cli, lib, undercounted)

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

    # The mirror check reports but does not fail: `get-vbios` claims a partial
    # implementation while the entry does not allow it.
    mirror = [
        why
        for ok, why in verdicts(
            cli,
            lib,
            clean
            + "| `get-vbios` | Offline ROM | 部分：`-i/--input` 离线解析已实现 |\n",
        )
        if "claims claiming implemented" in why or "claiming implemented" in why
    ]
    assert len(mirror) == 1 and "get-vbios" in mirror[0], mirror
    assert all(
        ok
        for ok, why in verdicts(cli, lib, "# nothing\n")
        if "claiming implemented" in why
    )

    # A table with no command rows cannot certify anything.
    assert any(
        "no command rows" in why
        for ok, why in verdicts(cli, lib, "# nothing\n")
        if not ok
    )

    print(
        "selftest ok: allow-list, message agreement, blanket-refusal rows, "
        "mirror rows, table completeness (omitted command, wrong heading count, "
        "wrong headline count), empty inputs"
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
