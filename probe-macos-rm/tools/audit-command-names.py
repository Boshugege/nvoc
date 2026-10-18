#!/usr/bin/env python3
"""Check that every command a message tells the user to run really exists.

The defect this exists for is on the record four times over: a refusal in
`cli/src/lib.rs` told users to run `nvoc macos-eval --plan <target>=<delta>`,  # audit-command-names: quoted
`core/src/gpu.rs` told them to run `nvoc list`, and two comments repeated the  # audit-command-names: quoted
same short spelling. This workspace builds `nvoc-cli`, `nvoc-auto-optimizer`,
`nvoc-srv` and friends; **nothing is called `nvoc`**. A message that names a
program the build does not produce sends the user to a shell error, and the
mistake is invisible to every test that only asserts the rest of the sentence.

Two contradictions are checkable, both from sources that decide the answer:

* a bare `nvoc` followed by a token that *is* a subcommand of one of the
  binaries (so the token is a command and the program in front of it has the
  wrong spelling) -- this is how `nvoc list` and `nvoc macos-eval` are caught;  # audit-command-names: quoted
* a real program name followed by a subcommand that belongs to a *different*
  program (`nvoc-cli macos-eval`) -- right token, wrong program.  # audit-command-names: quoted

What it does **not** check: whether an arbitrary lowercase word after a program
name is a command at all. English prose is allowed to say "nvoc-cli refuses",
so a token that is nobody's subcommand is left alone rather than reported.

Scope: `.rs` and `.py` sources only, skipping `target/`, virtualenvs and the
vendored submodule. Markdown is deliberately out of scope: the feature matrix
and the review records quote superseded wording verbatim, and a record that
cannot quote its own history is worse than one that quotes a fixed string.

Prototype note: this tool is developed and run on the client. It reads files, so
it needs no device and no GPU.
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

# A manifest's package name or a `[[bin]]` name: the programs this workspace
# actually produces. `[package] name` is read loosely on purpose -- a lib target
# in the set only widens the informational direction, never a verdict.
BIN_RE = re.compile(r'^name\s*=\s*"([A-Za-z0-9_-]+)"', re.M)
BIN_ENTRY_RE = re.compile(r"\[\[bin\]\][^\[]*?name\s*=\s*\"([A-Za-z0-9_-]+)\"")
# The CLI's command table (`CommandSpec::new("list", Group::Meta, ...)`).
SPEC_RE = re.compile(r'CommandSpec::new\(\s*"([a-z0-9][a-z0-9-]*)"')
# The auto-optimizer's declared subcommands, i.e. the ones its `--help` lists.
ARG_COMMAND_RE = re.compile(r'Command::new\(\s*"([a-z0-9][a-z0-9-]*)"')
# A bare `nvoc` (not `nvoc-cli`, not `nvoc-next`) followed by one token.
BARE_RE = re.compile(r"(?<![\w-])nvoc(?![\w-])[ \t]+([a-z][a-z0-9-]*)")
# A real program name followed by one token.
PROGRAM_RE = re.compile(r"(?<![\w-])(nvoc-[a-z0-9-]+)[ \t]+([a-z][a-z0-9-]*)")
# A line that deliberately quotes a wrong spelling (a record of the defect, a
# fixture) carries this marker and is skipped; the count is reported so that
# silence stays visible.
MARKER = "audit-command-names: quoted"
# The fixture prose, kept as constants so each can carry the marker on its own
# line without burying the case list in long string literals.
PROSE_CLEAN = (
    "// run `nvoc-cli list` to see indices; see nvoc-auto-optimizer macos-eval --plan x=1\n"
    "// Please re-run nvoc from an elevated command prompt.\n"
    "// Please re-run nvoc with sudo.\n"
)
PROSE_BARE_LIST = (
    "// use `nvoc list` to see available indices\n"  # audit-command-names: quoted
)
PROSE_BARE_OTHER = (
    "// answered by `nvoc macos-eval --plan <t>=<d>`\n"  # audit-command-names: quoted
)
PROSE_WRONG = "// run `nvoc-cli macos-eval --plan x=1`\n"  # audit-command-names: quoted
PROSE_UNKNOWN = "// nvoc-cli refuses this flag and nvoc-next is a directory\n"
# A line that quotes a wrong name and carries the marker: the whole line is
# skipped, and the exemption is counted.
PROSE_MARKED = f"// quotes `nvoc list` and also runs `nvoc get-info`  {MARKER}\n"  # audit-command-names: quoted
SKIP_DIRS = {".git", ".venv", "__pycache__", "node_modules", "target", "nvapi-rs"}
SCAN_SUFFIXES = {".rs", ".py"}

# Where each binary declares its subcommands. The map is the point: a tool that
# hard-coded the names would not notice a rename.
SUBCOMMAND_SOURCES = {
    "nvoc-cli": ("cli", "cli/src/lib.rs", SPEC_RE),
    "nvoc-auto-optimizer": (
        "auto-optimizer",
        "auto-optimizer/src/arg_help.rs",
        ARG_COMMAND_RE,
    ),
}


def binaries(root: Path) -> dict[str, set[str]]:
    """Program names per manifest, keyed by crate directory."""
    found: dict[str, set[str]] = {}
    for manifest in sorted(root.glob("*/Cargo.toml")):
        text = manifest.read_text()
        names = set(BIN_RE.findall(text)) | set(BIN_ENTRY_RE.findall(text))
        if names:
            found[manifest.parent.name] = names
    return found


def subcommands(root: Path, by_crate: dict[str, set[str]]) -> dict[str, set[str]]:
    """Declared subcommands per program, keyed by the name its manifest gives it.

    The key comes from the manifest, not from `SUBCOMMAND_SOURCES`, so a renamed
    binary is reported under its real name; the root command some tools declare
    is dropped because it is the program, not one of its subcommands.
    """
    every = {name for names in by_crate.values() for name in names}
    table: dict[str, set[str]] = {}
    for canonical, (crate, source, pattern) in SUBCOMMAND_SOURCES.items():
        names = by_crate.get(crate)
        if not names:
            continue
        path = root / source
        if not path.is_file():
            raise SystemExit(f"SKIP: {path} not found")
        program = canonical if canonical in names else sorted(names)[0]
        table[program] = {
            name for name in pattern.findall(path.read_text()) if name not in every
        }
    return table


def prose_files(root: Path) -> list[Path]:
    """The sources worth reading, skipping build output, venvs and the submodule."""
    out = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.suffix not in SCAN_SUFFIXES:
            continue
        if any(part in SKIP_DIRS for part in path.relative_to(root).parts):
            continue
        out.append(path)
    return out


def verdicts(root: Path) -> list[tuple[bool, str]]:
    """Every contradiction found, in file order, as (ok, why) pairs."""
    programs = {name for names in binaries(root).values() for name in names}
    if not programs:
        return [(False, "no Cargo manifest declares a program name to check against")]
    table = subcommands(root, binaries(root))
    if not table or not {name for names in table.values() for name in names}:
        return [(False, "no binary declares a subcommand list to check against")]
    every = {name for names in table.values() for name in names}
    files = prose_files(root)
    if not files:
        return [(False, "no .rs or .py source was found to scan")]

    out: list[tuple[bool, str]] = []
    bare = 0
    crossed = 0
    exempt = 0
    for path in files:
        relative = path.relative_to(root)
        for number, line in enumerate(path.read_text().splitlines(), start=1):
            if MARKER in line:
                exempt += 1
                continue
            for token in BARE_RE.findall(line):
                if token in every:
                    bare += 1
                    out.append((
                        False,
                        f"{relative}:{number} says `nvoc {token}`, but the token after a bare "
                        f"`nvoc` is a declared command; write the program that owns it "
                        f"({owners(token, table)})",
                    ))
            for program, token in PROGRAM_RE.findall(line):
                if program not in table:
                    continue
                for other, names in table.items():
                    if (
                        other != program
                        and token in names
                        and token not in table[program]
                    ):
                        crossed += 1
                        out.append((
                            False,
                            f"{relative}:{number} says `{program} {token}`, but {token} is "
                            f"{other}'s subcommand",
                        ))
                        break
    if not bare:
        out.append((
            True,
            f"no message asks for a bare `nvoc <command>` ({len(every)} declared command(s) "
            f"over {len(table)} program(s))",
        ))
    if not crossed:
        out.append((
            True,
            "no program name is followed by another program's subcommand",
        ))
    out.append((
        True,
        f"scanned {len(files)} source file(s) against {len(programs)} declared program name(s); "
        "commands with no declared owner are left alone by design",
    ))
    if exempt:
        out.append((
            True,
            f"{exempt} line(s) exempt as quoted spellings (marked `{MARKER}`)",
        ))
    return out


def owners(token: str, table: dict[str, set[str]]) -> str:
    """Which programs declare this command, for the message."""
    return " or ".join(
        sorted(program for program, names in table.items() if token in names)
    )


def fixture(
    root: Path,
    cli_body: str,
    prose: str,
    program: str = "nvoc-cli",
    arg_body: str = 'Command::new("macos-eval")\n',
) -> Path:
    """A workspace shaped like the real one, small enough to reason about."""
    (root / "cli" / "src").mkdir(parents=True, exist_ok=True)
    (root / "auto-optimizer" / "src").mkdir(parents=True, exist_ok=True)
    (root / "cli" / "Cargo.toml").write_text(f'[package]\nname = "{program}"\n')
    (root / "auto-optimizer" / "Cargo.toml").write_text(
        '[package]\nname = "nvoc-auto-optimizer"\n'
    )
    (root / "cli" / "src" / "lib.rs").write_text(cli_body)
    (root / "auto-optimizer" / "src" / "arg_help.rs").write_text(arg_body)
    (root / "cli" / "src" / "message.rs").write_text(prose)
    return root


def selftest() -> int:
    """The checks against fixtures, including the defect that motivated them."""
    # A table with two commands, one of which the prose names correctly and one
    # of which it names with the wrong program in front of it.
    table = (
        'CommandSpec::new("list", Group::Meta, "List all commands")\n'
        'CommandSpec::new("get-info", Group::Info, "GPU identity")\n'
    )
    cases: list[tuple[str, str, bool, str]] = [
        (
            "clean prose",
            PROSE_CLEAN,
            True,
            "a program name followed by its own subcommand, plus bare `nvoc` before "
            "English words, must not fail",
        ),
        (
            "bare nvoc with a real command",
            PROSE_BARE_LIST,
            False,
            "the recorded defect in core/src/gpu.rs: `nvoc` is not a program this workspace "
            "produces",
        ),
        (
            "bare nvoc with the other binary's command",
            PROSE_BARE_OTHER,
            False,
            "the recorded defect in cli/src/lib.rs",
        ),
        (
            "right command, wrong program",
            PROSE_WRONG,
            False,
            "macos-eval belongs to nvoc-auto-optimizer",
        ),
        (
            "unknown token is left alone",
            PROSE_UNKNOWN,
            True,
            "a token that is nobody's subcommand must not be reported",
        ),
    ]
    failures = 0
    for name, prose, want_ok, why in cases:
        with tempfile.TemporaryDirectory() as tmp:
            root = fixture(Path(tmp), table, prose)
            checks = verdicts(root)
            failed = [text for ok, text in checks if not ok]
            got_ok = not failed
            if got_ok != want_ok:
                failures += 1
                print(f"  FAIL selftest/{name}: expected ok={want_ok}, got {checks}")
            elif why:
                print(f"  ok   selftest/{name}: {why}")
    # An exemption must still leave the surrounding line checked: a line whose
    # other half is wrong is not saved by the marker.
    with tempfile.TemporaryDirectory() as tmp:
        root = fixture(Path(tmp), table, PROSE_MARKED)
        checks = verdicts(root)
        failed = [text for ok, text in checks if not ok]
        reported = [text for ok, text in checks if ok and "exempt as quoted" in text]
        if not failed and reported:
            print(
                "  ok   selftest/marked line: the line is skipped whole and the exemption is "
                "counted in the output"
            )
        else:
            failures += 1
            print(f"  FAIL selftest/marker case: failed={failed} reported={reported}")
    # Two ways for the input to be empty, both of which must fail rather than
    # pass: no manifest to read the program names from, and no source to read the
    # commands from.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "somewhere.rs").write_text("// nothing to check against\n")
        checks = verdicts(root)
        if any(not ok and "manifest" in text for ok, text in checks):
            print("  ok   selftest/no manifest: an input with no program names fails")
        else:
            failures += 1
            print(
                f"  FAIL selftest/no manifest: expected a failure naming the manifests: {checks}"
            )
    with tempfile.TemporaryDirectory() as tmp:
        root = fixture(
            Path(tmp),
            cli_body="// no command table here\n",
            prose="// hi\n",
            arg_body="// no commands declared either\n",
        )
        checks = verdicts(root)
        if any(not ok and "subcommand list" in text for ok, text in checks):
            print(
                "  ok   selftest/no command source: an input with no declared commands fails"
            )
        else:
            failures += 1
            print(
                f"  FAIL selftest/no command source: expected a failure naming the source: {checks}"
            )

    # The command list comes from the source, not from a hard-coded vocabulary:
    # rename the command in the table and `nvoc zap` becomes a defect whose
    # message names the program that declares it.
    with tempfile.TemporaryDirectory() as tmp:
        root = fixture(
            Path(tmp),
            table.replace('"list"', '"zap"'),
            "// use `nvoc zap` here\n",
        )
        failed = [text for ok, text in verdicts(root) if not ok]
        if len(failed) == 1 and "nvoc-cli" in failed[0] and "nvoc zap" in failed[0]:
            print(
                "  ok   selftest/renamed command: `nvoc zap` is caught once the source declares "
                "it, and the message names `nvoc-cli` as its owner"
            )
        else:
            failures += 1
            print(
                f"  FAIL selftest/renamed command: expected one failure naming nvoc-cli: {failed}"
            )
    if failures:
        print(f"selftest FAILED: {failures} case(s)")
        return 1
    print(
        "selftest ok: 10 cases (the two recorded defects, the wrong-program direction, "
        "three false-positive guards, two empty inputs, a renamed command read from the "
        "source, and an exemption that is skipped whole)"
    )
    return 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    root = Path(__file__).resolve().parents[2]
    if "--root" in argv:
        root = Path(argv[argv.index("--root") + 1])
    if not (root / "cli" / "src" / "lib.rs").is_file():
        print(f"SKIP: {root} does not look like the workspace root", file=sys.stderr)
        return 2
    checks = verdicts(root)
    failed = 0
    for ok, why in checks:
        print(f"  {'ok  ' if ok else 'FAIL'} {why}")
        failed += 0 if ok else 1
    print(f"\ncommand names: {len(checks) - failed} ok, {failed} failed ({root})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
