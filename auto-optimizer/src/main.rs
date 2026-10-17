#![allow(
    unused_crate_dependencies,
    clippy::type_complexity,
    clippy::too_many_arguments
)]
mod arg_help;
mod autoscan_config;
mod cleanup;
mod macos_eval;
#[cfg(debug_assertions)]
mod manual_override;
mod oc_profile_function;
mod oc_scanner;
mod optimize;
mod platform;
mod progressbar;
mod scan_log;
mod scan_strategy;
mod scan_support;
mod stressor_process;

use anyhow::Result;
use clap::ArgMatches;
use cleanup::{AutoscanExit, cleanup_autoscan_exit};
use nvoc_core::{
    BackendSet, ConvertEnum, GpuSelector, GpuTarget, ResetPublicVftableOffset, VfpResetDomain,
    discover_targets, run, select_targets,
};
use oc_profile_function::{export_vfp_from_log, fix_result, handle_vfp_export, handle_vfp_import};
use oc_scanner::{autoscan_gpuboostv3, autoscan_legacy};
use optimize::run_optimize;
use platform::is_elevated;
use progressbar::init_scan_cli_color;
use std::io::{self, Write};
use std::process::exit;

fn main() {
    #[cfg(feature = "stressor-bundled")]
    // A bundled stress worker is still a separate OS process. The hidden argv
    // marker prevents the child from entering the optimizer command router.
    if std::env::args_os().any(|arg| arg == std::ffi::OsStr::new(stressor_process::WORKER_ARG)) {
        cli_stressor_cuda_rs::runner::run_from_args();
        return;
    }

    match main_result() {
        Ok(code) => exit(code),
        Err(e) => {
            let _ = writeln!(io::stderr(), "{}", e);
            exit(1);
        }
    }
}

fn require_elevated() -> Result<(), Box<dyn std::error::Error>> {
    if is_elevated() {
        return Ok(());
    }
    #[cfg(windows)]
    return Err("This command requires Administrator privileges. \
         Please re-run nvoc from an elevated command prompt."
        .into());
    #[cfg(not(windows))]
    Err("This command requires root privileges. \
         Please re-run nvoc with sudo."
        .into())
}

fn single_target<'a>(targets: &'a [GpuTarget<'a>]) -> Result<&'a GpuTarget<'a>, nvoc_core::Error> {
    let mut targets = targets.iter();
    targets
        .next()
        .ok_or_else(|| nvoc_core::Error::from("no GPU selected"))
        .and_then(|target| match targets.next() {
            None => Ok(target),
            Some(..) => Err(nvoc_core::Error::from("multiple GPUs selected")),
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetRequirement {
    None,
    NvapiAny,
    NvapiSingle,
    /// Native NullMoth RM (macOS); discovered without NVAPI/NVML.
    MacosRm,
}

impl TargetRequirement {
    /// True for requirements satisfiable only by NVAPI/NVML, neither of which
    /// exists on macOS.
    fn requires_nvapi(self) -> bool {
        matches!(
            self,
            TargetRequirement::NvapiAny | TargetRequirement::NvapiSingle
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CommandRequirements {
    target: TargetRequirement,
    elevation: bool,
}

fn command_requirements(command: &str) -> Option<CommandRequirements> {
    let target = match command {
        "export-vfp-log" => TargetRequirement::None,
        "macos-eval" => TargetRequirement::MacosRm,
        "export-vfp" | "import-vfp" | "fix-vfp-result" => TargetRequirement::NvapiSingle,
        "reset-vfp" | "autoscan-vfp" | "autoscan-vfp-legacy" | "optimize" => {
            TargetRequirement::NvapiAny
        }
        _ => return None,
    };
    let elevation = matches!(
        command,
        "reset-vfp" | "import-vfp" | "autoscan-vfp" | "autoscan-vfp-legacy" | "optimize"
    );

    Some(CommandRequirements { target, elevation })
}

fn gpu_selector(matches: &ArgMatches) -> GpuSelector {
    match matches.get_many::<String>("gpu") {
        Some(values) => GpuSelector::from_specs(values.cloned()),
        None => GpuSelector::all(),
    }
}

fn main_result() -> Result<i32, Box<dyn std::error::Error>> {
    let app = arg_help::get_arguments();
    arg_help::check_single_dash_args(&app)?;
    let matches = app.get_matches();
    init_scan_cli_color(matches.get_flag("no_color"));

    let (command_name, _) = matches.subcommand().expect("subcommand required");
    let requirements = command_requirements(command_name).expect("known subcommand");

    // macOS has no NVAPI/NVML runtime, so every command that requires them
    // (`optimize`, `autoscan-vfp*`, `reset-vfp`, `import-vfp`, `export-vfp`)
    // would otherwise fail deep inside library discovery with a cascade of
    // loader / dlopen errors. Fail fast with one actionable message instead:
    // the native NullMoth RM backend is the only macOS backend, and it is
    // read-only.
    if cfg!(target_os = "macos") && requirements.target.requires_nvapi() {
        return Err(format!(
            "`{command_name}` requires NVAPI/NVML, which do not exist on macOS. \
             The native NullMoth RM backend is the only macOS backend; use \
             `macos-eval` for the read-only stock evaluation."
        )
        .into());
    }

    // The second, independent macOS gate, and the one a later change should
    // open: a command that mutates settings needs the backend to be able to
    // write. The backend answers that question itself (the write half of the
    // private ids carries the legacy privileged bits, so an unprivileged client
    // is refused with 0x1b before the GSP is consulted), and this is the only
    // place that turns the answer into a refusal -- so nothing here has to be
    // edited to start writing, only the backend's own constant.
    if cfg!(target_os = "macos")
        && requirements.elevation
        && !nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED
    {
        return Err(format!(
            "`{command_name}` changes settings, and this macOS build's NullMoth RM backend \
             reports settings_write = false: {}. Use `macos-eval` for the read-only \
             stock evaluation until that gate opens.",
            nvoc_core::macos_rm::WRITE_NEEDS_PRIVILEGE_REASON
        )
        .into());
    }

    if requirements.target == TargetRequirement::None {
        match matches.subcommand() {
            Some(("export-vfp-log", matches)) => {
                export_vfp_from_log(matches)?;
            }
            _ => unreachable!("offline command requirements did not match dispatch"),
        }
        return Ok(0);
    }

    if requirements.target == TargetRequirement::MacosRm {
        let inventory = discover_targets(BackendSet::MacosRm)?;
        let selector = gpu_selector(&matches);
        let targets_all = inventory.targets();
        let selected = select_targets(&targets_all, &selector).unwrap_or_default();
        return match matches.subcommand() {
            Some(("macos-eval", matches)) => {
                macos_eval::run_macos_eval(&selected, matches)?;
                Ok(0)
            }
            _ => unreachable!("macOS command requirements did not match dispatch"),
        };
    }

    let inventory = discover_targets(BackendSet::Both)
        .or_else(|both_err| {
            eprintln!("Warning: combined GPU discovery failed: {}", both_err);
            discover_targets(BackendSet::Nvapi)
        })
        .or_else(|nvapi_err| {
            eprintln!("Warning: NvAPI discovery failed: {}", nvapi_err);
            discover_targets(BackendSet::Nvml)
        })?;

    let selector = gpu_selector(&matches);

    let targets_all = inventory.targets();
    let selected_targets = select_targets(&targets_all, &selector).unwrap_or_default();
    let nvapi_selected: Vec<GpuTarget<'_>> = selected_targets
        .iter()
        .copied()
        .filter(|target| target.has_nvapi())
        .collect();

    if nvapi_selected.is_empty() {
        return Err("This command requires NvAPI, but NvAPI initialization failed".into());
    }

    if requirements.elevation {
        require_elevated()?;
    }

    match matches.subcommand() {
        Some(("reset-vfp", matches)) => {
            let domain = matches
                .get_one::<String>("vfp_domain")
                .map(|s| VfpResetDomain::from_str(s.as_str()))
                .transpose()?
                .unwrap_or(VfpResetDomain::All);
            for gpu in &nvapi_selected {
                run(gpu, ResetPublicVftableOffset { domain })?;
            }
        }
        Some(("export-vfp", matches)) => {
            // Static export is retired from user surface; --quick stays
            // functional only until the GUI autoscan.py binary calls are
            // migrated (separate PR).
            if matches.get_flag("quick") {
                eprintln!(
                    "Warning: `export-vfp --quick` is deprecated; static CSV export moved to \
                     `nvoc-cli get-public-vftable --output-csv <PATH> --domain <DOMAIN>`"
                );
            }
            let gpu = single_target(&nvapi_selected)?;
            handle_vfp_export(gpu, matches)?;
        }
        Some(("export-vfp-log", _)) => unreachable!("offline command was already dispatched"),
        Some(("import-vfp", matches)) => {
            // Internal optimize-workflow entry point; user-facing import moved
            // to nvoc-cli (set-public-vftable-point-offset --import-csv).
            eprintln!(
                "Warning: `import-vfp` is an internal workflow command; user imports should use \
                 `nvoc-cli set-public-vftable-point-offset --import-csv <PATH> [--domain <DOMAIN>]`"
            );
            let gpu = single_target(&nvapi_selected)?;
            handle_vfp_import(gpu, matches)?;
        }
        Some(("fix-vfp-result", matches)) => {
            let gpu = single_target(&nvapi_selected)?;
            fix_result(gpu, matches)?
        }
        Some(("autoscan-vfp", matches)) => match autoscan_gpuboostv3(&nvapi_selected, matches) {
            Ok(()) => cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Success),
            Err(e) => {
                eprintln!("Error in autoscan: {:?}", e);
                cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Error);
                return Ok(1);
            }
        },
        Some(("autoscan-vfp-legacy", matches)) => match autoscan_legacy(&nvapi_selected, matches) {
            Ok(()) => cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Success),
            Err(e) => {
                eprintln!("Error in autoscan_legacy: {:?}", e);
                cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Error);
                return Ok(1);
            }
        },
        Some(("optimize", matches)) => match run_optimize(&nvapi_selected, matches) {
            Ok(()) => cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Success),
            Err(e) => {
                eprintln!("Error in optimize: {:?}", e);
                cleanup_autoscan_exit(&nvapi_selected, AutoscanExit::Error);
                return Ok(1);
            }
        },
        _ => unreachable!("unknown command"),
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirements(command: &str) -> CommandRequirements {
        command_requirements(command).expect("command requirements")
    }

    #[test]
    fn every_subcommand_has_requirements() {
        let app = arg_help::get_arguments();
        let missing: Vec<_> = app
            .get_subcommands()
            .filter_map(|subcommand| {
                let name = subcommand.get_name();
                command_requirements(name)
                    .is_none()
                    .then(|| name.to_string())
            })
            .collect();

        assert!(
            missing.is_empty(),
            "subcommands missing requirements: {missing:?}"
        );
    }

    #[test]
    fn export_vfp_log_is_offline() {
        assert_eq!(
            requirements("export-vfp-log"),
            CommandRequirements {
                target: TargetRequirement::None,
                elevation: false,
            }
        );
    }

    #[test]
    fn export_vfp_is_read_only_preflight() {
        assert_eq!(
            requirements("export-vfp"),
            CommandRequirements {
                target: TargetRequirement::NvapiSingle,
                elevation: false,
            }
        );
    }

    #[test]
    fn gpu_write_commands_require_elevation() {
        for command in [
            "reset-vfp",
            "import-vfp",
            "autoscan-vfp",
            "autoscan-vfp-legacy",
        ] {
            assert!(
                requirements(command).elevation,
                "{command} should require elevation"
            );
        }
    }

    #[test]
    fn macos_write_gate_follows_the_backend_capability() {
        // The gate is driven by the backend's own constant, so this test pins the
        // pairing between the capability the backend reports and the commands the
        // gate refuses: on macOS, while the backend reports no write support,
        // exactly the commands that mutate settings are refused. It is vacuous on
        // other targets by design, and meaningful on the machine that runs it.
        let write_supported = nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED;
        // The gate can only refuse on macOS, and only while the backend reports no
        // write support.  Off that combination `refused` is false for every
        // command, so asserting `refused == mutates` there would demand that a
        // mutating command be refused by a gate that is not active -- which is how
        // this test came to fail on every non-macOS host while claiming to be
        // vacuous on them.  Guard the pairing, and keep the mutation set itself
        // non-empty so a stale command list cannot make the test pass by naming
        // nothing that mutates.
        let mutating = [
            "reset-vfp",
            "import-vfp",
            "autoscan-vfp",
            "autoscan-vfp-legacy",
        ];
        assert!(
            mutating.iter().any(|c| requirements(c).elevation),
            "none of the pinned commands mutates settings, so this test proves nothing"
        );
        if !cfg!(target_os = "macos") || write_supported {
            return;
        }
        for command in mutating {
            let refused = requirements(command).elevation;
            assert!(
                refused,
                "{command} mutates settings but the macOS write gate lets it through \
                 while settings_write = {write_supported}"
            );
        }
    }
}
