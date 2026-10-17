//! `cargo xtask check`: the local mirror of the ci.yml lint gates, plus the
//! `ci --fmt` auto-fix pass that force-applies formatting and machine
//! applicable lint suggestions before the gate re-runs.

use crate::nvapi_cache;
use crate::util::{self, Res};
use std::process::Command;

/// Auto-apply every compliance fix the toolchain can make on its own:
/// rustfmt, clippy's machine-applicable suggestions, ruff format and ruff's
/// fixable lint violations. Verification is deliberately NOT part of this
/// pass — the caller re-runs [`check`] afterwards, which still fails on
/// anything the fixers could not resolve.
pub fn fix_all() -> Res<()> {
    let root = util::repo_root();

    util::step("auto-fix: rustfmt (cargo fmt --all)");
    let mut fmt = Command::new("cargo");
    fmt.args(["fmt", "--all"]).current_dir(&root);
    util::run(&mut fmt)?;

    // clippy --fix cannot run under -D warnings (the gate would abort before
    // applying suggestions); the plain run only emits fixable suggestions.
    // --allow-dirty is required because development trees are rarely clean.
    util::step("auto-fix: clippy --fix (workspace, CUDA stressor excluded)");
    let mut clippy = Command::new("cargo");
    clippy
        .args([
            "clippy",
            "--workspace",
            "--exclude",
            "cli-stressor-cuda-rs",
            "--all-targets",
            "--fix",
            "--allow-dirty",
        ])
        .current_dir(&root);
    nvapi_cache::run_guarded(&root, &mut clippy)?;

    util::step("auto-fix: clippy --fix (cli-stressor-cuda-rs, no default features)");
    let mut stressor = Command::new("cargo");
    stressor
        .args([
            "clippy",
            "-p",
            "cli-stressor-cuda-rs",
            "--all-targets",
            "--no-default-features",
            "--fix",
            "--allow-dirty",
        ])
        .current_dir(&root);
    nvapi_cache::run_guarded(&root, &mut stressor)?;

    let excludes = ruff_exclude_args(&root);
    util::step("auto-fix: ruff format (.)");
    let mut format = util::uv_run(&root, "nvoc-tui", &["ruff", "format", "."]);
    format.args(&excludes);
    util::run(&mut format)?;

    util::step("auto-fix: ruff check --fix (.)");
    let mut lint = util::uv_run(&root, "nvoc-tui", &["ruff", "check", ".", "--fix"]);
    lint.args(&excludes);
    util::run(&mut lint)
}

pub fn check() -> Res<()> {
    let root = util::repo_root();

    util::step("rustfmt (cargo fmt --all -- --check)");
    let mut fmt = Command::new("cargo");
    fmt.args(["fmt", "--all", "--", "--check"])
        .current_dir(&root);
    util::run(&mut fmt)?;

    util::step("clippy: workspace (CUDA stressor excluded)");
    let mut clippy = Command::new("cargo");
    clippy
        .args([
            "clippy",
            "--workspace",
            "--exclude",
            "cli-stressor-cuda-rs",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])
        .current_dir(&root);
    nvapi_cache::run_guarded(&root, &mut clippy)?;

    cross_clippy_mirror(&root)?;

    util::step("clippy: cli-stressor-cuda-rs (no default features)");
    let mut stressor = Command::new("cargo");
    stressor
        .args([
            "clippy",
            "-p",
            "cli-stressor-cuda-rs",
            "--all-targets",
            "--no-default-features",
            "--",
            "-D",
            "warnings",
        ])
        .current_dir(&root);
    nvapi_cache::run_guarded(&root, &mut stressor)?;

    util::step("cross-check: release Rust packages (aarch64-linux, where c_char = u8)");
    cross_check_arm64_release(&root)?;

    util::step("ruff format (.)");
    let excludes = ruff_exclude_args(&root);
    if !excludes.is_empty() {
        let skipped: Vec<&String> = excludes.iter().skip(1).step_by(2).collect();
        println!(
            "  local-only paths invisible to CI are excluded: {}",
            skipped
                .iter()
                .map(|path| path.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let mut format = util::uv_run(&root, "nvoc-tui", &["ruff", "format", ".", "--check"]);
    format.args(&excludes);
    util::run(&mut format)?;

    util::step("ruff check (.)");
    let mut lint = util::uv_run(&root, "nvoc-tui", &["ruff", "check", "."]);
    lint.args(&excludes);
    util::run(&mut lint)?;

    util::step("probe tool selftests (device-free instruments)");
    probe_tool_selftests(&root)
}

/// The probe tools are the instruments behind every claim about what the driver
/// answered, and until now nothing executed them: ruff reads their syntax and
/// no step ran their assertions, so what they pin -- the dump format a real
/// capture must still parse, the Rust field map a decode must agree with, the
/// inputs behind a claim tally -- could rot silently between runs. Each tool
/// below is executed with `python3` (the interpreter the runbooks call them
/// with, so the gate follows the runbooks rather than a private environment)
/// and must both exit 0 and print its own `selftest ok` line: a selftest that
/// has quietly become a no-op fails here instead of passing.
///
/// `probe-macos-rm/tools/gsp-handler-map.py --selftest` needs the GSP firmware
/// blob, which is not in the repo, so it is named as needing input and is run
/// only when `NVOC_GSP_FIRMWARE` points at one; unset, the step prints a SKIP
/// that names the reason rather than a pass. The remaining tools are listed as
/// having no selftest at all, which is what stops a *new* tool from being added
/// and never run: the directory is enumerated and an unlisted file fails.
fn probe_tool_selftests(root: &std::path::Path) -> Res<()> {
    const RUNNABLE: [&str; 5] = [
        "audit-entry-parity.py",
        "audit-live-attempts.py",
        "audit-matrix-claims.py",
        "decode-clk-containers.py",
        "record-run-provenance.py",
    ];
    const NEEDS_FIRMWARE: &str = "gsp-handler-map.py";
    const NO_SELFTEST: [&str; 6] = [
        "audit-probe-ids.py",
        "audit-recorded-rm-ids.py",
        "dump-gsp-families.py",
        "extract-gsp-method-table.py",
        "extract-nvrm-method-table.py",
        "summarise-method-table.py",
    ];
    const FIRMWARE_ENV: &str = "NVOC_GSP_FIRMWARE";
    let dir = root.join("probe-macos-rm").join("tools");
    let entries = std::fs::read_dir(&dir)
        .map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    let mut found = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read an entry of {}: {error}", dir.display()))?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("py") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let declared = RUNNABLE.contains(&name.as_str())
            || name == NEEDS_FIRMWARE
            || NO_SELFTEST.contains(&name.as_str());
        if !declared {
            return Err(format!(
                "probe-macos-rm/tools/{name} is neither run by this step nor declared as having \
                 no selftest; add it to RUNNABLE, NEEDS_FIRMWARE or NO_SELFTEST in \
                 xtask/src/checker.rs"
            ));
        }
        found.push(name);
    }
    for name in RUNNABLE {
        run_tool_selftest(root, &dir, name, None)?;
    }
    match std::env::var(FIRMWARE_ENV) {
        Ok(firmware) if !firmware.is_empty() => {
            if !std::path::Path::new(&firmware).is_file() {
                return Err(format!(
                    "{FIRMWARE_ENV} is set to {firmware}, which is not a file; unset it or point \
                     it at the GSP firmware blob"
                ));
            }
            run_tool_selftest(root, &dir, NEEDS_FIRMWARE, Some(&firmware))?;
        }
        _ => println!(
            "  SKIP probe-macos-rm/tools/{NEEDS_FIRMWARE} --selftest: it reads the GSP firmware \
             blob, which is not in the repo; set {FIRMWARE_ENV}=/path/to/gsp_tu10x.bin to run it"
        ),
    }
    Ok(())
}

/// Runs one tool's `--selftest` and requires its own success line, quoted back
/// into the gate log so the run is visible rather than merely counted.
fn run_tool_selftest(
    root: &std::path::Path,
    dir: &std::path::Path,
    name: &str,
    firmware: Option<&str>,
) -> Res<()> {
    let tool = dir.join(name);
    let mut command = Command::new("python3");
    command.arg(&tool);
    if let Some(firmware) = firmware {
        command.arg(firmware);
    }
    command.arg("--selftest").current_dir(root);
    let output = command.output().map_err(|error| {
        format!(
            "failed to run python3 for probe-macos-rm/tools/{name} (the runbooks call the tools \
             with python3): {error}"
        )
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        print!("{stdout}");
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "probe-macos-rm/tools/{name} --selftest exited with {}",
            output.status
        ));
    }
    let reported = stdout
        .lines()
        .rev()
        .find(|line| line.contains("selftest ok"))
        .ok_or_else(|| {
            format!(
                "probe-macos-rm/tools/{name} --selftest exited 0 without printing a `selftest ok` \
                 line, so no assertion result was reported"
            )
        })?;
    println!("  {}", reported.trim());
    Ok(())
}

/// Re-runs the CI clippy shape against the platform this host is NOT, so
/// `#[cfg(...)]`-gated code compiles in the gate instead of silently
/// vanishing: platform-skewed imports and dead items pass the host clippy
/// yet fail `-D warnings` on the other platform's CI job (xtask's own
/// doctor.rs unused `PathBuf`, core's dead non-Windows stub — both caught
/// here). Windows hosts mirror the ubuntu job (`x86_64-unknown-linux-gnu`);
/// Linux hosts mirror the Windows jobs (`x86_64-pc-windows-msvc` — a
/// workspace-wide superset of ci.yml's auto-optimizer/srv Windows runs);
/// hosts with no CI platform (macOS) mirror nothing. Static checks only —
/// cross-compiled test binaries cannot execute locally, so behavioral
/// platform gaps still belong to CI. The target's rust-std is installed on
/// first use.
fn cross_clippy_mirror(root: &std::path::Path) -> Res<()> {
    let target = match std::env::consts::OS {
        "windows" => "x86_64-unknown-linux-gnu",
        "linux" => "x86_64-pc-windows-msvc",
        _ => return Ok(()),
    };

    util::step(&format!("clippy: cross-target mirror ({target})"));
    let installed = util::capture(Command::new("rustup").args(["target", "list", "--installed"]))?
        .lines()
        .any(|line| line.trim() == target);
    if !installed {
        let mut add = Command::new("rustup");
        add.args(["target", "add", target]);
        util::run(&mut add)?;
    }

    let mut clippy = Command::new("cargo");
    clippy
        .args([
            "clippy",
            "--workspace",
            "--exclude",
            "cli-stressor-cuda-rs",
            "--all-targets",
            "--target",
            target,
            "--",
            "-D",
            "warnings",
        ])
        .current_dir(root);
    nvapi_cache::run_guarded(root, &mut clippy)
}

/// Cross-checks every crate the release matrix natively compiles for
/// aarch64-linux — the one release target where `core::ffi::c_char` is `u8`
/// rather than `i8`: the release.yml Rust build line (nvoc-cli /
/// nvoc-auto-optimizer / cli-stressor-cuda-rs) plus pynvoc, which the
/// GUI/TUI onefile jobs compile through maturin. A c_char-skewed buffer
/// (e.g. `[i8]` passed to cudarc's `*mut c_char` APIs) passes every host
/// gate and only explodes in the arm64 release cell (E0308, the failed
/// 0.2.0-alpha.2 release attempt's linux-arm64 build). The workspace is
/// deliberately NOT
/// checked wholesale: nvoc-srv is Windows-only and would false-fail.
/// Check-only — no cross linker and no CUDA toolkit are needed; the
/// target's rust-std is installed on first use, mirroring
/// [`cross_clippy_mirror`].
fn cross_check_arm64_release(root: &std::path::Path) -> Res<()> {
    const TARGET: &str = "aarch64-unknown-linux-gnu";

    let installed = util::capture(Command::new("rustup").args(["target", "list", "--installed"]))?
        .lines()
        .any(|line| line.trim() == TARGET);
    if !installed {
        let mut add = Command::new("rustup");
        add.args(["target", "add", TARGET]);
        util::run(&mut add)?;
    }

    let mut check = Command::new("cargo");
    check
        .args([
            "check",
            "-p",
            "nvoc-cli",
            "-p",
            "nvoc-auto-optimizer",
            "-p",
            "cli-stressor-cuda-rs",
            "-p",
            "pynvoc",
            "--features",
            "cuda12,vulkan",
            "--target",
            TARGET,
        ])
        .current_dir(root);
    nvapi_cache::run_guarded(root, &mut check)
}

/// `--exclude` arguments for the ruff steps covering top-level paths git does
/// not track at all. ci.yml runs on a fresh checkout, so purely local working
/// trees (reverse-engineering scripts, wiki clones, …) must not fail the local
/// gate. Directories that also contain tracked files stay in scope: a new
/// untracked source file there is fair game for the gate.
fn ruff_exclude_args(root: &std::path::Path) -> Vec<String> {
    let Some(untracked) =
        git_top_paths(root, &["ls-files", "--others", "--exclude-standard", "-z"])
    else {
        return Vec::new();
    };
    let tracked = git_top_paths(root, &["ls-files", "-z"]).unwrap_or_default();
    exclude_tops(&untracked, &tracked)
        .iter()
        .flat_map(|top| ["--exclude".to_string(), top.clone()])
        .collect()
}

/// Distinct first path components reported by `git ls-files`.
fn git_top_paths(root: &std::path::Path, args: &[&str]) -> Option<Vec<String>> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let listing = util::capture(&mut command).ok()?;
    let mut tops: Vec<String> = listing
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| first_component(entry).to_string())
        .collect();
    tops.sort();
    tops.dedup();
    Some(tops)
}

fn exclude_tops(untracked: &[String], tracked: &[String]) -> Vec<String> {
    let mut tops: Vec<String> = untracked
        .iter()
        .map(|path| first_component(path).to_string())
        .filter(|top| {
            !tracked
                .iter()
                .any(|tracked_path| first_component(tracked_path) == top.as_str())
        })
        .collect();
    tops.sort();
    tops.dedup();
    tops
}

fn first_component(path: &str) -> &str {
    match path.find(['/', '\\']) {
        Some(index) => &path[..index],
        None => path,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_fully_local_top_level_paths_are_excluded() {
        let excludes = super::exclude_tops(
            &[
                "reverse/a.py".to_string(),
                "nvoc.wiki/x.md".to_string(),
                "tui/new_file.py".to_string(),
            ],
            &["tui/lib.py".to_string(), "README.md".to_string()],
        );
        assert_eq!(
            excludes,
            vec!["nvoc.wiki".to_string(), "reverse".to_string()]
        );
    }

    #[test]
    fn path_first_component_handles_both_separators() {
        assert_eq!(super::first_component("reverse/a.py"), "reverse");
        assert_eq!(super::first_component("reverse\\a.py"), "reverse");
        assert_eq!(super::first_component("README.md"), "README.md");
    }
}
