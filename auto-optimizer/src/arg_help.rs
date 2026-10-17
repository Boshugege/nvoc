use super::platform::{
    default_vfp_csv_path, default_vfp_init_csv_path, default_vfp_log_path,
    default_vfp_temp_csv_path,
};
use clap::{Arg, ArgAction, Command};
use nvoc_core::{ConvertEnum, VfpResetDomain};

pub fn get_arguments() -> Command {
    Command::new("nvoc-auto-optimizer")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Skyworks")
        .about("NVIDIA GPU VFP curve workflows and autoscan")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("gpu")
                .short('g')
                .long("gpu")
                .value_name("GPU_ID")
                .num_args(1..)
                .action(ArgAction::Append)
                .global(true)
                .help("GPU ID selector; accepts decimal or hex and can be repeated"),
        )
        .arg(
            Arg::new("no_color")
                .long("no-color")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Disable ANSI color output (also honors NO_COLOR)"),
        )
        .subcommand(vfp_export_command())
        .subcommand(vfp_export_log_command())
        .subcommand(vfp_import_command())
        .subcommand(vfp_fix_result_command())
        .subcommand(vfp_autoscan_command(false))
        .subcommand(vfp_autoscan_command(true))
        .subcommand(optimize_command())
        .subcommand(vfp_reset_command())
        .subcommand(macos_eval_command())
}

fn macos_eval_command() -> Command {
    Command::new("macos-eval")
        .about(
            "macOS-only stock evaluation: bounded Vulkan load + native RM telemetry + \
             Metal correctness verifier. Read-only; performs no writes.",
        )
        .after_help(
            "Runs the original cli-stressor-cuda-rs --vulkan-only load for --duration \
             while sampling the NullMoth RM telemetry, then runs the stress-macos \
             Metal verifier (when --metal-check points at it). No candidate is \
             applied: the offset write path exists (0x2080d01c, same block as the \
             readable 0x2080901b) but a write waits on authorisation, a healthy \
             session and the field identity only the write itself can give, so \
             this is the preflight + load + verify step only.\n\n\
             --dry-run drops the load entirely and adds the plan: --plan \
             <target>=<delta> is evaluated against the read-only snapshot and \
             reports, per step, which record it resolves to, what that record \
             holds now, whether the step stays inside the window the driver \
             reported, and which of those facts is missing when it cannot say. \
             A step is never applied, and `settings_write` reports whether this \
             backend would write at all.",
        )
        .arg(
            Arg::new("duration")
                .long("duration")
                .value_name("SECS")
                .num_args(1)
                .value_parser(clap::value_parser!(f64))
                .default_value("30")
                .help("Bounded stock load duration in seconds"),
        )
        .arg(
            Arg::new("interval")
                .long("interval")
                .value_name("SECS")
                .num_args(1)
                .value_parser(clap::value_parser!(f64))
                .default_value("2")
                .help("Telemetry sample interval in seconds"),
        )
        .arg(
            Arg::new("vulkan-stressor")
                .long("vulkan-stressor")
                .value_name("PATH")
                .num_args(1)
                .default_value("cli-stressor-cuda-rs")
                .help("Path to the original Vulkan stressor binary to reuse"),
        )
        .arg(
            Arg::new("metal-check")
                .long("metal-check")
                .value_name("PATH")
                .num_args(1)
                .default_value("")
                .help("Path to stress-macos/metal_check; empty skips the verifier"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Preflight and plan only: no Vulkan load, no Metal verifier, no writes"),
        )
        .arg(
            Arg::new("plan")
                .long("plan")
                .value_name("TARGET=DELTA")
                .num_args(1)
                .action(ArgAction::Append)
                .help(
                    "Evaluate one bounded offset step without writing it, e.g. \
                     --plan graphics=+8mhz or --plan bit:0x1=-2000; repeatable. \
                     TARGET is a public domain or bit:<n>; DELTA is signed kHz, \
                     optionally suffixed khz/mhz",
                ),
        )
        .arg(
            Arg::new("out")
                .long("out")
                .value_name("PATH")
                .num_args(1)
                .default_value("")
                .help("Optional path to also write the JSON report to"),
        )
        .arg(
            Arg::new("vulkan-width")
                .long("vulkan-width")
                .value_name("PX")
                .num_args(1)
                .value_parser(clap::value_parser!(u32))
                .help("Override the stressor render width (default: tool default)"),
        )
        .arg(
            Arg::new("vulkan-height")
                .long("vulkan-height")
                .value_name("PX")
                .num_args(1)
                .value_parser(clap::value_parser!(u32))
                .help("Override the stressor render height"),
        )
        .arg(
            Arg::new("vulkan-shells")
                .long("vulkan-shells")
                .value_name("N")
                .num_args(1)
                .value_parser(clap::value_parser!(u32))
                .help("Override the stressor shell count"),
        )
        .arg(
            Arg::new("vulkan-iters")
                .long("vulkan-iters")
                .value_name("N")
                .num_args(1)
                .value_parser(clap::value_parser!(u32))
                .help("Override the stressor iteration count"),
        )
}

fn optimize_command() -> Command {
    let mut cmd = Command::new("optimize")
        .about("Run the complete VFP optimization workflow")
        .arg(
            Arg::new("mode")
                .long("mode")
                .value_name("MODE")
                .default_value("standard")
                .value_parser(["standard", "ultrafast", "legacy"])
                .help("Optimization workflow mode"),
        )
        .arg(
            Arg::new("fresh")
                .long("fresh")
                .action(ArgAction::SetTrue)
                .help("Discard the resumable scan log and temporary result"),
        )
        .arg(
            Arg::new("yes")
                .short('y')
                .long("yes")
                .action(ArgAction::SetTrue)
                .help("Acknowledge the safety warning without prompting"),
        )
        .arg(
            Arg::new("workspace")
                .long("workspace")
                .value_name("PATH")
                .num_args(1)
                .help("Relative per-GPU scan workspace path"),
        );

    #[cfg(feature = "stressor-bundled")]
    {
        cmd = cmd
            .arg(
                Arg::new("stressor_profile")
                    .long("stressor-profile")
                    .value_name("PROFILE")
                    .num_args(1)
                    .value_parser(["auto", "low-vram", "standard", "40-50"])
                    .help("Bundled CUDA stress profile (default: auto)"),
            )
            .arg(
                Arg::new("stressor_config")
                    .long("stressor-config")
                    .value_name("PATH")
                    .num_args(1)
                    .conflicts_with("stressor_profile")
                    .help("Custom stressor TOML config"),
            );
    }

    #[cfg(all(feature = "stressor-external", not(feature = "stressor-bundled")))]
    {
        cmd = cmd.arg(
            Arg::new("test_exe")
                .long("test-exe")
                .value_name("PATH")
                .default_value("cli-stressor-cuda-rs"),
        );
    }

    #[cfg(all(feature = "stressor-bundled", feature = "stressor-external"))]
    {
        cmd = cmd
            .arg(
                Arg::new("stressor_backend")
                    .long("stressor-backend")
                    .default_value("bundled")
                    .value_parser(["bundled", "external"]),
            )
            .arg(
                Arg::new("test_exe")
                    .long("test-exe")
                    .value_name("PATH")
                    .default_value("cli-stressor-cuda-rs"),
            );
    }

    cmd
}

fn vfp_export_command() -> Command {
    Command::new("export-vfp")
        .about(
            "Export current VFP curve as CSV (dynamic load sampling; static --quick is deprecated \
             — use `nvoc-cli get-public-vftable --output-csv <PATH> --domain <DOMAIN>`)",
        )
        .args(vfp_domain_args("Export"))
        .arg(
            Arg::new("output")
                .value_name("OUTPUT")
                .num_args(1)
                .default_value("-")
                .help("Output file path"),
        )
        .arg(
            Arg::new("quick")
                .short('q')
                .long("quick")
                .action(ArgAction::SetTrue)
                .help(
                    "Deprecated: static export moved to nvoc-cli get-public-vftable --output-csv",
                ),
        )
        .arg(
            Arg::new("nocheck")
                .short('n')
                .long("nocheck")
                .action(ArgAction::SetTrue)
                .help("Skip dynamic result validity check"),
        )
}

fn vfp_export_log_command() -> Command {
    Command::new("export-vfp-log")
        .about("Export VFP points parsed from an autoscan log")
        .arg(
            Arg::new("log")
                .value_name("LOG")
                .short('l')
                .long("log")
                .num_args(1)
                .default_value(default_vfp_log_path())
                .help("Input JSONL scan log file path"),
        )
        .arg(
            Arg::new("initcsv")
                .value_name("INITCSV")
                .short('i')
                .num_args(1)
                .default_value(default_vfp_init_csv_path())
                .help("Reference initial VFP CSV path used when OUTPUT is a file"),
        )
        .arg(
            Arg::new("output")
                .value_name("OUTPUT")
                .num_args(1)
                .default_value("-")
                .help("Output file path"),
        )
}

fn vfp_import_command() -> Command {
    Command::new("import-vfp")
        .about(
            "Internal optimize-workflow driver; users should import via \
             `nvoc-cli set-public-vftable-point-offset --import-csv <PATH> [--domain <DOMAIN>]`",
        )
        .hide(true)
        .args(vfp_domain_args("Import"))
        .arg(
            Arg::new("input")
                .value_name("INPUT")
                .num_args(1)
                .default_value("-")
                .help("Input file path"),
        )
}

fn vfp_fix_result_command() -> Command {
    Command::new("fix-vfp-result")
        .about("Post-process autoscan results")
        .arg(
            Arg::new("delta_ref")
                .value_name("DELTA_REF")
                .short('d')
                .num_args(1)
                .default_value("3")
                .help("Reference delta"),
        )
        .arg(
            Arg::new("tempcsv")
                .value_name("TMPCSV")
                .short('v')
                .num_args(1)
                .default_value(default_vfp_temp_csv_path())
                .help("Temporary VFP CSV path"),
        )
        .arg(
            Arg::new("outputcsv")
                .value_name("OUTPUTCSV")
                .short('o')
                .num_args(1)
                .default_value(default_vfp_csv_path())
                .help("Output VFP CSV path"),
        )
        .arg(
            Arg::new("initcsv")
                .value_name("INITCSV")
                .short('i')
                .num_args(1)
                .default_value(default_vfp_init_csv_path())
                .help("Reference initial VFP CSV path"),
        )
        .arg(
            Arg::new("ultrafast")
                .short('u')
                .long("ultrafast")
                .action(ArgAction::SetTrue)
                .help("Enable ultrafast mode post-processing"),
        )
        .arg(
            Arg::new("vfplog")
                .value_name("VFPLOG")
                .short('l')
                .num_args(1)
                .default_value(default_vfp_log_path())
                .help("VFP JSONL log file path"),
        )
        .arg(
            Arg::new("minus_bin")
                .value_name("MINUS_BIN")
                .short('m')
                .num_args(1)
                .default_value("1")
                .allow_hyphen_values(true)
                .value_parser(clap::value_parser!(i32).range(-50..=50))
                .help("Margin bin adjustment integer"),
        )
}

fn vfp_autoscan_command(legacy: bool) -> Command {
    let mut cmd = if legacy {
        Command::new("autoscan-vfp-legacy")
            .about("Auto-scanner for legacy GPUs using global pstate OC offset")
    } else {
        Command::new("autoscan-vfp").about("Auto-scanner for a new VFP curve")
    }
    .arg(
        Arg::new("log")
            .value_name("LOG")
            .short('l')
            .long("log")
            .num_args(1)
            .default_value(default_vfp_log_path())
            .help("Autoscan JSONL log file path"),
    )
    .arg(
        Arg::new("timeout_loops")
            .short('t')
            .value_name("TIMEOUT_LOOPS")
            .num_args(1)
            .default_value("30")
            .value_parser(clap::value_parser!(u32).range(1..=1_000))
            .help("CLI stress duration/retry loop count"),
    )
    .arg(
        Arg::new("bsod_recovery")
            .short('b')
            .long("recovery_method_switch")
            .num_args(1)
            .value_parser(["aggressive", "traditional"])
            .help("Override recovery method"),
    )
    .arg(
        Arg::new("cuda_device")
            .long("cuda-device")
            .value_name("INDEX")
            .num_args(1)
            .value_parser(clap::value_parser!(u32))
            .help("CUDA device ordinal for the stressor"),
    )
    .arg(
        Arg::new("stressor_extra_args")
            .long("stressor-extra-args")
            .value_name("ARG")
            .num_args(1..)
            .allow_hyphen_values(true)
            .help("Extra arguments appended to each stressor invocation"),
    );

    #[cfg(feature = "stressor-bundled")]
    {
        cmd = cmd
            .arg(
                Arg::new("stressor_profile")
                    .long("stressor-profile")
                    .value_name("PROFILE")
                    .num_args(1)
                    .value_parser(["auto", "low-vram", "standard", "40-50"])
                    .help("Bundled CUDA stress profile; defaults to auto VRAM selection"),
            )
            .arg(
                Arg::new("stressor_config")
                    .long("stressor-config")
                    .value_name("PATH")
                    .num_args(1)
                    .conflicts_with("stressor_profile")
                    .help("Custom stressor TOML config (overrides the embedded profile)"),
            );
    }

    #[cfg(all(feature = "stressor-external", not(feature = "stressor-bundled")))]
    {
        cmd = cmd.arg(
            Arg::new("test_exe")
                .value_name("TEST_EXE")
                .short('w')
                .long("test-exe")
                .num_args(1)
                .default_value("cli-stressor-cuda-rs")
                .help("External cli-stressor-cuda-rs executable path"),
        );
    }

    #[cfg(all(feature = "stressor-bundled", feature = "stressor-external"))]
    {
        cmd = cmd
            .arg(
                Arg::new("stressor_backend")
                    .long("stressor-backend")
                    .value_name("BACKEND")
                    .default_value("bundled")
                    .value_parser(["bundled", "external"])
                    .help("Select bundled worker or external executable backend"),
            )
            .arg(
                Arg::new("test_exe")
                    .value_name("TEST_EXE")
                    .short('w')
                    .long("test-exe")
                    .num_args(1)
                    .default_value("cli-stressor-cuda-rs")
                    .help("External cli-stressor-cuda-rs executable path"),
            );
    }

    if !legacy {
        cmd = cmd
            .arg(
                Arg::new("ultrafast")
                    .short('u')
                    .long("ultrafast")
                    .action(ArgAction::SetTrue)
                    .help("Enable ultrafast mode"),
            )
            .arg(
                Arg::new("point_seq")
                    .value_name("POINT_SEQ")
                    .short('q')
                    .num_args(1)
                    .default_value("-")
                    .help("Point sequence to scan"),
            )
            .arg(
                Arg::new("output")
                    .value_name("OUTPUTCSV")
                    .short('o')
                    .num_args(1)
                    .default_value(default_vfp_temp_csv_path())
                    .help("Autoscan output CSV path"),
            )
            .arg(
                Arg::new("Vmem_scan_switch")
                    .short('m')
                    .long("Vmem_scan_switch")
                    .action(ArgAction::SetTrue)
                    .help("Enable memory voltage scan"),
            )
            .arg(
                Arg::new("initcsv")
                    .value_name("INITCSV")
                    .short('i')
                    .num_args(1)
                    .default_value(default_vfp_init_csv_path())
                    .help("Reference initial VFP CSV path"),
            );
    }

    cmd.args(hidden_vfp_lock_args())
}

fn vfp_reset_command() -> Command {
    Command::new("reset-vfp")
        .about("Reset VFP curve deltas")
        .arg(
            Arg::new("vfp_domain")
                .long("vfp-domain")
                .value_name("VFP_DOMAIN")
                .num_args(1)
                .default_value(VfpResetDomain::All.to_str())
                .value_parser(VfpResetDomain::possible_values().to_vec())
                .help("VFP reset domain: all, core, or memory"),
        )
}

fn vfp_domain_args(action: &'static str) -> Vec<Arg> {
    vec![
        Arg::new("memory")
            .long("memory")
            .action(ArgAction::SetTrue)
            .conflicts_with_all(["processor", "video", "undefined"])
            .help(format!("{action} memory VF table")),
        Arg::new("processor")
            .long("processor")
            .action(ArgAction::SetTrue)
            .conflicts_with_all(["memory", "video", "undefined"])
            .help(format!("{action} processor VF table")),
        Arg::new("video")
            .long("video")
            .action(ArgAction::SetTrue)
            .conflicts_with_all(["memory", "processor", "undefined"])
            .help(format!("{action} video VF table")),
        Arg::new("undefined")
            .long("undefined")
            .action(ArgAction::SetTrue)
            .conflicts_with_all(["memory", "processor", "video"])
            .help(format!("{action} undefined VF table")),
    ]
}

fn hidden_vfp_lock_args() -> Vec<Arg> {
    vec![
        Arg::new("locked_voltage")
            .long("locked-voltage")
            .value_name("POINT_OR_VOLTAGE")
            .num_args(1)
            .hide(true),
        Arg::new("locked_core_clocks")
            .long("locked-core-clocks")
            .value_names(["MIN_MHZ", "MAX_MHZ"])
            .num_args(2)
            .hide(true),
        Arg::new("locked_mem_clocks")
            .long("locked-mem-clocks")
            .value_names(["MIN_MHZ", "MAX_MHZ"])
            .num_args(2)
            .hide(true),
        Arg::new("clock")
            .long("clock")
            .value_names(["UPPER_MHZ", "LOWER_MHZ"])
            .num_args(1..=2)
            .hide(true),
        Arg::new("voltage")
            .long("voltage")
            .action(ArgAction::SetTrue)
            .hide(true),
        Arg::new("point")
            .long("point")
            .value_name("POINT")
            .num_args(1)
            .hide(true),
        Arg::new("domain")
            .long("domain")
            .value_name("DOMAIN")
            .num_args(1)
            .hide(true),
    ]
}

fn collect_long_flags(cmd: &Command, out: &mut Vec<String>) {
    for arg in cmd.get_arguments() {
        if let Some(long) = arg.get_long() {
            out.push(long.to_string());
        }
    }
    for sub in cmd.get_subcommands() {
        collect_long_flags(sub, out);
    }
}

pub fn check_single_dash_args_from<I, S>(
    cmd: &Command,
    args: I,
) -> Result<(), Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut known_longs: Vec<String> = Vec::new();
    collect_long_flags(cmd, &mut known_longs);

    for arg in args {
        let arg = arg.as_ref();
        if !arg.starts_with('-') || arg.starts_with("--") || arg == "-" {
            continue;
        }

        let body = arg.trim_start_matches('-');
        let flag_name = body.split('=').next().unwrap_or(body);

        if known_longs.iter().any(|l| l == flag_name) {
            return Err(format!("invalid option {:?} -- did you mean --{}?", arg, body).into());
        }
    }
    Ok(())
}

pub fn check_single_dash_args(cmd: &Command) -> Result<(), Box<dyn std::error::Error>> {
    check_single_dash_args_from(cmd, std::env::args().skip(1))
}
