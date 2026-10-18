use super::*;

pub(super) fn selected(backend: BackendChoice) -> bool {
    backend == BackendChoice::MacosRm
        || (backend == BackendChoice::Auto && cfg!(target_os = "macos"))
}

/// The fan/power/thermal entries whose *read* this backend cannot answer, each
/// paired with the layer it is missing.
///
/// Stage 11 item 5 asks the entry and the backend to state the same thing.
/// These eight used to state the blanket sentence, which ends by explaining a
/// *write* gate: a read refused with a write-gate explanation describes the
/// wrong layer, and on the one family that is otherwise readable it hides that
/// the read answers. Each arm therefore carries core's constant for the layer
/// that is actually missing -- one reason per missing layer, shared with the
/// Python surface -- while the wall for the families that really have no read
/// keeps the terse list.
fn read_layer_refusal(command: Command) -> Option<(&'static str, &'static str)> {
    let reason = match command {
        // The fan-control family: no published command exists at all.
        Command::GetFanCurve | Command::GetFanPolicyInfo => {
            nvoc_core::FAN_CONTROL_UNREADABLE_REASON
        }
        // The policy view around the wall, not the wall itself.
        Command::GetPowerCeiling | Command::GetPowerMode | Command::GetDNotifier => {
            nvoc_core::POWER_POLICY_UNREADABLE_REASON
        }
        // Located, published, measured answering here -- ours to wire.
        Command::GetRatedTdp => nvoc_core::RATED_TDP_UNWIRED_REASON,
        // A thermal field that is not a sensor: the reading answers, the
        // configured limit is a different field with its own reason.
        Command::GetPublicTempLimit => nvoc_core::THERMAL_LIMIT_UNREADABLE_REASON,
        Command::GetTempSim => nvoc_core::THERMAL_SIM_UNREADABLE_REASON,
        _ => return None,
    };
    Some((command.name(), reason))
}

pub(super) fn validate(command: Command) -> CliResult<()> {
    if matches!(
        command,
        Command::GetGpuList
            | Command::GetInfo
            | Command::GetStatus
            | Command::GetSettings
            | Command::GetThrottleReasons
            // The offset read is allowed, but it can still fail *at run time*
            // for one domain: core attaches a name only where a record's own
            // published frequency equals the live clock of the same snapshot,
            // and reports the reason otherwise (see `unnamed_offset_reason`).
            // Refusing the command outright would read as "the backend cannot",
            // which is exactly what it can.
            | Command::GetPstateGlobalFreqOffset
            // The fan read is allowed for the same reason the offset read is:
            // this backend does publish a tachometer reading per cooler, so a
            // wholesale refusal would read as "the backend cannot". The parts of
            // the contract it has no source for (the three percentages) come
            // back null with a reason in `unavailable`, not as a zero.
            | Command::GetFanInfo
            // The power-limit read is allowed for the same reason. This backend
            // reads the enforced wall and the requested limit out of the same
            // snapshot as get-status, so refusing the entry wholesale would read
            // as "the backend has no power limit", which is not true; the two
            // range ends it has no source for come back null with the reason.
            | Command::GetPublicPowerLimit
            // The legacy thermal view is allowed for the same reason as the two
            // above: this backend does publish live sensor readings, so refusing
            // a three-sensor read wholesale would read as "no thermal data on
            // this path". The sensor it has answers; the parts it cannot fill
            // (per-sensor range, controller) come back null with the reason.
            | Command::GetLegacyTempSensor
    ) {
        Ok(())
    } else if command == Command::GetTemperatureThresholds {
        Err(CliError::new(format!(
            // The constant already ends by naming the entry that does answer
            // (get-legacy-temp-sensor), so the wrapper adds nothing after it:
            // the captured text stuttered that sentence twice.
            "get-temp-thresholds is unavailable on macOS RM: {}",
            nvoc_core::THERMAL_THRESHOLDS_UNREADABLE_REASON
        )))
    } else if command == Command::GetPstateFreqRange {
        // Same shape of refusal as the thermal thresholds above: this backend
        // does answer P-state questions (get-status reports the current one),
        // so the list would read as "no P-state here".
        Err(CliError::new(format!(
            "get-pstate-freq-range is unavailable on macOS RM: {} -- on macOS get-status \
             reports the current P-state and get-pstate-global-freq-offset reports the \
             per-domain offsets instead.",
            nvoc_core::PSTATE_FREQ_TABLE_UNREADABLE_REASON
        )))
    } else if let Some((entry, reason)) = read_layer_refusal(command) {
        // The three families' refused reads: the same prefix shape as the two
        // arms above, so the layer text is the only thing that differs between
        // an entry this backend answers and one it cannot.
        Err(CliError::new(format!(
            "{entry} is unavailable on macOS RM: {reason}"
        )))
    } else {
        Err(CliError::new(format!(
            "{} is unavailable on macOS RM; supported commands: get-gpu-list, get-info, get-status, get-settings, get-throttle-reasons, get-pstate-global-freq-offset, get-fan-info, get-public-power-limit, get-legacy-temp-sensor. Performance writes stay gated twice over: the clock SET twin (0x2080D01C, one category byte over from the read: `0x2080901B` + 0x40 on the legacy-non-privileged byte and +1 on the message id) is a 0xC0-0xFF write id, so this unprivileged client is refused with 0x1b before the GSP sees it, and even behind that gate no write is issued until the field identity, the bounded step and the restore path are verified on the device.",
            command.name()
        )))
    }
}

pub(super) fn execute(invocation: &Invocation, command: Command) -> CliResult<Execution> {
    // Refuse unsupported commands before opening IOKit or allocating RM objects.
    validate(command)?;
    let inventory = discover_targets(BackendSet::MacosRm)?;
    let targets = inventory.targets();
    let selected = select_targets(&targets, &gpu_selector(invocation))?;
    if selected.is_empty() {
        return Err(CliError::new("no GPUs matched the selector"));
    }
    let results: Vec<TargetResult> = selected.into_iter().map(|target| {
        let output = target.macos_rm().map_err(CliError::from).and_then(|device| {
            let mut value = match command {
                Command::GetGpuList => json!({
                    "index": target.index, "gpu_id": target.id.0,
                    "gpu_id_hex": format!("0x{:04X}", target.id.0),
                    "pci_bus": target.id.pci_bus(), "pci_address": device.info().pci_address,
                    "rm_gpu_id": device.info().rm_gpu_id, "name": device.info().name,
                    "backend_nvapi": false, "backend_nvml": false, "backend_macos_rm": true,
                }),
                Command::GetInfo => serde_json::to_value(run(&target, nvoc_core::QueryMacosRmInfo)?.output)?,
                Command::GetStatus => serde_json::to_value(run(&target, nvoc_core::QueryMacosRmStatus)?.output)?,
                Command::GetSettings => {
                    serde_json::to_value(run(&target, nvoc_core::QueryMacosRmSettings)?.output)?
                }
                // Named domains only. Core compares each record's own published
                // frequency with the live clock of the same snapshot and reports
                // a reason when no single record matched, so this arm has no
                // fallback of its own: an uncorroborated number is never printed,
                // and the reason says which comparison failed.
                Command::GetPstateGlobalFreqOffset => {
                    let domain = option_domain(invocation, ClockDomain::Graphics)?;
                    let name = domain_label(domain);
                    let settings = run(&target, nvoc_core::QueryMacosRmSettings)?.output;
                    // The optional dry-run step, as the shared plan document:
                    // the same object, keys and note the optimizer's `--plan`
                    // section publishes, so one question has one answer shape.
                    // Nothing is written.
                    let plan = match option_one(invocation, "plan-delta") {
                        None => serde_json::Value::Null,
                        Some(token) => offset_plan_json(&settings, name, token)?,
                    };
                    offset_entry_json(&settings, name, plan)?
                }
                // The fan-info contract: a cooler count plus, on NVML, three
                // percentages. This backend publishes a tachometer reading per
                // cooler slot and nothing else, so the percentages are null with
                // the reason attached, and the readings carry the driver's own
                // cooler slot -- the position in the published list is a position
                // after the plausibility filter, which is not the cooler index.
                Command::GetFanInfo => {
                    serde_json::to_value(run(&target, nvoc_core::QueryMacosRmStatus)?.output.fan_report())?
                }
                // The power-limit contract (min/current/max watts). Two of the
                // three read out of the same snapshot as get-status -- the
                // enforced wall and the requested limit -- plus the driver's own
                // policy wattage. The two ends have no source on this path, so
                // they come back null with the reason attached rather than as a
                // bound the driver never published; a section that could not be
                // read yields nulls, never 0 W.
                Command::GetPublicPowerLimit => {
                    serde_json::to_value(
                        run(&target, nvoc_core::QueryMacosRmStatus)?.output.power_limit_report(),
                    )?
                }
                // The legacy three-sensor thermal view. The array root and the
                // per-sensor keys are the entry's own; the live readings come
                // from the RUSD temperature sections and the per-sensor reading
                // range from the public thermal read (0x20800513), which this
                // query issues read-only. Every field this backend cannot fill
                // carries its reason, so a null never has to be read as a
                // measured zero and a refused read never has to be read as a
                // value that is simply missing.
                Command::GetLegacyTempSensor => {
                    serde_json::to_value(run(&target, nvoc_core::QueryMacosRmThermalView)?.output)?
                }
                // Decoded by the shared `QueryThrottleReasons` operation (core
                // serves it from the RM snapshot on macOS), so this surface and
                // pynvoc's `query_throttle_reasons` cannot drift apart. NVML
                // also appends per-policy violation counters; the RM backend
                // has no equivalent, so violation stays null like NVML when the
                // device exposes no counters.
                Command::GetThrottleReasons => {
                    let reasons: Vec<serde_json::Value> =
                        run(&target, nvoc_core::QueryThrottleReasons)?
                            .output
                            .into_iter()
                            .map(|reason| json!({"name": reason.name, "active": reason.active}))
                            .collect();
                    json!({ "reasons": reasons, "violation": serde_json::Value::Null })
                }
                _ => unreachable!("validated above"),
            };
            if let Some(object) = value.as_object_mut() {
                // The settings query carries its own capability block; the
                // status/info payloads keep the read-only summary.
                if matches!(
                    command,
                    Command::GetGpuList | Command::GetInfo | Command::GetStatus
                ) {
                    object.insert("capabilities".into(), json!({
                        "device_info": true, "pstate_query": true,
                        "telemetry": true, "performance_writes": false,
                    }));
                }
            }
            Ok(value)
        });
        match output {
            Ok(output) => TargetResult { gpu_id: Some(target.id.0), backend: "macos-rm",
                ok: true, output: Some(output), error: None },
            Err(error) => TargetResult { gpu_id: Some(target.id.0), backend: "macos-rm",
                ok: false, output: None, error: Some(error.to_string()) },
        }
    }).collect();
    let partial = command == Command::GetStatus
        && results.iter().any(|result: &TargetResult| {
            result
                .output
                .as_ref()
                .and_then(|v| v["unavailable"].as_object())
                .is_some_and(|v| !v.is_empty())
        });
    Ok(Execution {
        function: command.name(),
        command,
        backend: "macos-rm".into(),
        warnings: if partial {
            vec!["Partial macOS RM status: unavailable telemetry is null; see unavailable for reasons.".into()]
        } else {
            Vec::new()
        },
        results,
    })
}

/// The `--plan-delta` half of the offset entry: one request, published as the
/// shared plan document the optimizer and the Python bindings publish.
fn offset_plan_json(
    settings: &nvoc_core::DeviceSettings,
    name: &str,
    token: &str,
) -> Result<serde_json::Value, CliError> {
    let delta_khz = nvoc_core::parse_offset_delta(token).map_err(CliError::new)?;
    let request = nvoc_core::PlanRequest {
        target: nvoc_core::OffsetStepTarget::Domain(name.to_string()),
        delta_khz,
    };
    Ok(serde_json::to_value(nvoc_core::plan_document(
        settings,
        &[request],
    ))?)
}

/// The `get-pstate-global-freq-offset` body, built from one read.
///
/// Kept as a function of the projection so the shape of this entry can be tested
/// without a device: the numbers come from [`nvoc_core::DeviceSettings`] (one
/// shared view, so this entry cannot drift from the Python bindings), and the
/// only thing added here is the public vocabulary.
fn offset_entry_json(
    settings: &nvoc_core::DeviceSettings,
    name: &str,
    plan: serde_json::Value,
) -> Result<serde_json::Value, CliError> {
    let view = settings.clock_offset_view(name).map_err(CliError::new)?;
    Ok(json!({
        "domain": view.domain,
        // The backend's own P-state reading; this command's --pstate option is
        // not part of the offset records (they are per domain, not per P-state),
        // so it is reported rather than matched.
        "pstate": settings.pstate,
        "domain_bit": format!("0x{:x}", view.domain_bit),
        "offset_mhz": f64::from(view.offset_khz) / 1000.0,
        "offset_khz": view.offset_khz,
        "frequency_khz": view.frequency_khz,
        "corroboration": view.corroboration,
        // The span the driver's range block declares, in kHz, and the record
        // count it was read from. Null is "not read", never a span of zero;
        // window_reason then carries that block's own outcome.
        "window_min_khz": view.window_min_khz,
        "window_max_khz": view.window_max_khz,
        "window_records": view.window_records,
        "window_reason": view.window_reason,
        // The value a restore would have to write back: the offset this same read
        // found. Printed before the write gate opens, so the target is not
        // invented later.
        "restore_khz": view.restore_khz,
        "plan": plan,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The offset entry's shape without a device: what the read found plus the
    /// two things the write gate needs before it opens.
    fn named_graphics_offset() -> nvoc_core::NamedClockOffset {
        nvoc_core::NamedClockOffset {
            name: "graphics".to_string(),
            domain_bit: 1,
            frequency_khz: 1_575_000,
            offset_khz: -1,
            corroboration: "matched the live graphics clock of this snapshot".to_string(),
        }
    }

    #[test]
    fn the_offset_entry_publishes_the_driver_span_and_the_restore_target() {
        let settings = nvoc_core::DeviceSettings {
            clock_offsets_named: Some(vec![named_graphics_offset()]),
            clock_offset_limits_khz: Some(nvoc_core::ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 3,
            }),
            pstate: Some(0),
            ..Default::default()
        };

        let entry = offset_entry_json(&settings, "graphics", serde_json::Value::Null).unwrap();
        assert_eq!(entry["domain"], "graphics");
        assert_eq!(entry["domain_bit"], "0x1");
        assert_eq!(entry["offset_khz"], -1);
        assert_eq!(entry["offset_mhz"], -0.001);
        assert_eq!(entry["window_min_khz"], -18_750);
        assert_eq!(entry["window_max_khz"], 18_750);
        assert_eq!(entry["window_records"], 3);
        assert!(entry["window_reason"].is_null(), "{entry}");
        assert_eq!(
            entry["restore_khz"], -1,
            "the restore target is the read value"
        );
        assert!(
            entry["plan"].is_null(),
            "no --plan-delta means no plan object"
        );
    }

    /// No span read: the keys stay null and the reason says why, so a missing
    /// window can never be read as a span of zero, and a domain the comparison
    /// did not corroborate is refused rather than reported as a zero offset.
    #[test]
    fn an_unread_window_reaches_the_entry_as_null_with_a_reason() {
        let settings = nvoc_core::DeviceSettings {
            clock_offsets_named: Some(vec![named_graphics_offset()]),
            notes: vec![
                "offset channel: range refused 0x00000062, no window is reported for this reason"
                    .to_string(),
            ],
            ..Default::default()
        };

        let entry = offset_entry_json(&settings, "graphics", serde_json::Value::Null).unwrap();
        assert!(entry["window_min_khz"].is_null(), "{entry}");
        assert!(entry["window_max_khz"].is_null(), "{entry}");
        assert!(entry["window_records"].is_null(), "{entry}");
        assert!(
            entry["window_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("no window is reported"),
            "{entry}"
        );
        assert_eq!(entry["offset_khz"], -1, "the offset is still reported");

        assert!(offset_entry_json(&settings, "video", serde_json::Value::Null).is_err());
    }

    /// The plan section this entry publishes is the shared document: same keys,
    /// same note, same write flag as the optimizer's, pinned here because a
    /// second shape for one question is exactly what a reader cannot see a
    /// reason for.
    #[test]
    fn the_offset_entry_publishes_the_shared_plan_document() {
        let settings = nvoc_core::DeviceSettings {
            clock_offset_limits_khz: Some(nvoc_core::ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 1,
            }),
            clock_offsets: Some(vec![nvoc_core::ClockOffsetReading {
                domain_bit: 0x1,
                frequency_khz: 1_575_000,
                offset_khz: -126,
            }]),
            clock_offsets_named: Some(vec![nvoc_core::NamedClockOffset {
                name: "graphics".to_string(),
                domain_bit: 0x1,
                frequency_khz: 1_575_000,
                offset_khz: -126,
                corroboration: "matched the live graphics clock of this snapshot".to_string(),
            }]),
            ..Default::default()
        };

        let plan = offset_plan_json(&settings, "graphics", "+8mhz").expect("parses and serialises");
        assert_eq!(plan["applied"], serde_json::json!(false));
        assert_eq!(plan["settings_write"], serde_json::json!(false));
        assert_eq!(plan["requests"], serde_json::json!(1));
        assert_eq!(plan["steps"][0]["verdict"], serde_json::json!("ready"));
        assert_eq!(plan["steps"][0]["domain_bit"], serde_json::json!(1));
        assert!(
            plan["note"]
                .as_str()
                .unwrap_or_default()
                .contains("nothing was written"),
            "{plan}"
        );
        // This entry spells the target with --domain, so --plan-delta carries a
        // step size only; the shared grammar still owns what a step size is, and
        // a token that is not one is refused with the grammar's words rather than
        // read as zero.
        let refused = offset_plan_json(&settings, "graphics", "8mhzz").expect_err("not a step");
        let message = format!("{refused}");
        assert!(
            message.contains("khz") && message.contains("mhz"),
            "{message}"
        );
    }

    #[test]
    fn backend_flag_is_global_and_conflicts_with_other_backends() {
        for args in [["--macos-rm", "get-info"], ["get-info", "--macos-rm"]] {
            assert_eq!(parse_args(args).unwrap().backend, BackendChoice::MacosRm);
        }
        for other in ["--nvapi", "--nvml"] {
            assert!(parse_args(["get-info", "--macos-rm", other]).is_err());
        }
    }

    #[test]
    fn unsupported_commands_are_rejected_before_driver_access() {
        let invocation = parse_args(["set-power-limit", "100", "--macos-rm"]);
        assert!(
            invocation
                .unwrap_err()
                .to_string()
                .contains("unavailable on macOS RM")
        );
        // Also check callers constructing Invocations without the parser.
        let mut invocation = parse_args(["get-info", "--macos-rm"]).unwrap();
        invocation.command = Some(Command::SetPowerLimit);
        let error = run_invocation(&invocation).unwrap_err().to_string();
        assert!(error.contains("unavailable on macOS RM"));
        assert!(!error.contains("driver loaded"));
    }

    #[test]
    fn settings_is_readable_but_writes_stay_refused() {
        assert!(validate(Command::GetSettings).is_ok());
        // Stage-8 read face: throttle reasons are RM-backed now, so the
        // dedicated command joins the macOS allow-list.
        assert!(validate(Command::GetThrottleReasons).is_ok());
        for command in [
            Command::SetPowerLimit,
            Command::SetPstateGlobalFreqOffset,
            Command::SetTempLimit,
            Command::SetFanSpeed,
        ] {
            let error = validate(command).unwrap_err().to_string();
            assert!(error.contains("unavailable on macOS RM"), "{error}");
        }
    }

    #[test]
    fn the_offset_read_is_allowed_and_reports_a_named_or_uncorroborated_domain() {
        // Stage 11 item 5, second pass. The read used to be refused with a
        // message that named the missing layer; core now *supplies* that layer
        // -- it names a domain only when a record's own published frequency
        // equals the live clock of the same snapshot, and reports a reason
        // otherwise -- so the command is allowed and the layer explanation moved
        // to the run-time path (`nvoc_core::unnamed_offset_reason`), where an
        // uncorroborated domain is refused instead of an uncorroborated number
        // being printed.
        assert!(validate(Command::GetPstateGlobalFreqOffset).is_ok());
        // The write half of the same pair keeps the terse capability refusal:
        // there the driver really does refuse, so no layer needs naming.
        let write = validate(Command::SetPstateGlobalFreqOffset)
            .unwrap_err()
            .to_string();
        assert!(write.contains("unavailable on macOS RM"), "{write}");
        assert!(write.contains("0x2080D01C"), "{write}");
        assert!(!write.contains("named"), "{write}");
    }

    #[test]
    fn the_fan_read_is_allowed_and_its_unfillable_fields_say_so() {
        // Stage 11 item 5, fan family. This backend does publish a tachometer
        // reading per cooler (the same snapshot `get-status` reports), so
        // refusing fan-info wholesale would read as "this backend has no fan
        // data", which is not true. The arm answers from the core projection,
        // so the entry and the backend cannot state different things; the three
        // percentage fields of the contract come back null with the reason
        // attached rather than as a zero.
        assert!(validate(Command::GetFanInfo).is_ok());
        // The write half of the family keeps the terse capability refusal: no
        // fan SET exists on this path at all, so no layer needs naming.
        let write = validate(Command::SetFanSpeed).unwrap_err().to_string();
        assert!(write.contains("unavailable on macOS RM"), "{write}");
        assert!(!write.contains("rpm"), "{write}");
    }

    #[test]
    fn the_power_limit_read_is_allowed_while_its_write_half_stays_refused() {
        // Stage 11 item 5, power family. The enforced wall and the requested
        // limit are in the snapshot this backend already publishes, so the entry
        // answers from them; the two range ends come back null with the reason,
        // and no power SET is issued on any macOS path.
        assert!(validate(Command::GetPublicPowerLimit).is_ok());
        for command in [
            Command::SetPowerLimit,
            Command::ResetPowerLimit,
            Command::SetPublicTgpPercent,
            Command::GetRatedTdp,
        ] {
            let error = validate(command).unwrap_err().to_string();
            assert!(error.contains("unavailable on macOS RM"), "{error}");
        }
        // The rated-TDP read stays refused for a different reason than a
        // missing field: its route is the NVAPI private trio
        // (0xED2BEA09/0x87BD35EF/0xFCBDF642), which this path never issues, so
        // the number is not read -- it is not that the driver lacks it. The
        // reason is the shared constant, and it carries the measurement that
        // backs that claim (our own capture answered NV_OK), which is what
        // keeps "not wired in" from being read as "the driver has no value".
        let rated = validate(Command::GetRatedTdp).unwrap_err().to_string();
        assert!(rated.contains("unavailable on macOS RM"), "{rated}");
        assert!(
            rated.contains(nvoc_core::RATED_TDP_UNWIRED_REASON),
            "{rated}"
        );
        assert!(rated.contains("0x2080206e"), "{rated}");
        assert!(!rated.contains("get-public-power-limit"), "{rated}");
    }

    #[test]
    fn the_legacy_thermal_view_is_allowed_while_the_threshold_entries_stay_refused() {
        // Stage 11 item 5, temperature family. The backend publishes live
        // sensor readings, so the legacy thermal *view* answers from them; the
        // entries that ask for something this backend never reads stay refused,
        // with the reason living in the allow-list rather than in a fabricated
        // number.
        assert!(validate(Command::GetLegacyTempSensor).is_ok());
        for command in [
            // Thresholds (slowdown/shutdown/max) and the temp-limit range
            // (min/default/max + curve) are policy reads: the RUSD section
            // carries the reading, not the policy.
            Command::GetTemperatureThresholds,
            Command::GetPublicTempLimit,
            Command::GetTempSim,
            Command::SetTempLimit,
            Command::ResetTempLimit,
        ] {
            let error = validate(command).unwrap_err().to_string();
            assert!(error.contains("unavailable on macOS RM"), "{error}");
        }
        let allowed = validate(Command::GetTemperatureThresholds)
            .unwrap_err()
            .to_string();
        assert!(allowed.contains("get-legacy-temp-sensor"), "{allowed}");
        // The threshold entry is refused for a reason of its own, not with the
        // list: this backend does publish sensor readings (the legacy view is
        // allowed just above), so the list alone would read as "no thermal data
        // on this path". The reason is the shared core constant the Python
        // entry quotes, so the two cannot drift apart.
        assert!(
            allowed.contains(nvoc_core::THERMAL_THRESHOLDS_UNREADABLE_REASON),
            "{allowed}"
        );
        let pstate = validate(Command::GetPstateFreqRange)
            .unwrap_err()
            .to_string();
        assert!(
            pstate.contains(nvoc_core::PSTATE_FREQ_TABLE_UNREADABLE_REASON),
            "{pstate}"
        );
        assert!(pstate.contains("get-pstate-global-freq-offset"), "{pstate}");
        // Both refusals open with the entry name, then the shared constant, then
        // the entry that does answer -- and nothing that restates the shape the
        // reason already names: the first version stuttered ("it reads the
        // per-pstate clock table, and the per-pstate clock table has no source
        // here"). This prefix check is what keeps that from coming back.
        let thresholds_prefix = "get-temp-thresholds is unavailable on macOS RM: ";
        assert!(allowed.starts_with(thresholds_prefix), "{allowed}");
        assert!(
            allowed[thresholds_prefix.len()..]
                .starts_with(nvoc_core::THERMAL_THRESHOLDS_UNREADABLE_REASON),
            "{allowed}"
        );
        let pstate_prefix = "get-pstate-freq-range is unavailable on macOS RM: ";
        assert!(pstate.starts_with(pstate_prefix), "{pstate}");
        assert!(
            pstate[pstate_prefix.len()..]
                .starts_with(nvoc_core::PSTATE_FREQ_TABLE_UNREADABLE_REASON),
            "{pstate}"
        );
        // An entry with no read source *in* this backend still keeps the
        // list-shaped refusal: `get-temp-sim` used to be the example here, and
        // no longer is -- it now names the layer it is missing (see
        // `every_refused_read_of_the_three_families_names_its_layer`), because
        // the list ends by explaining a write gate, which is the wrong layer
        // for a read.
        let list_shaped = validate(Command::SetPowerLimit).unwrap_err().to_string();
        assert!(list_shaped.contains("supported commands:"), "{list_shaped}");
    }

    /// Stage 11 item 5, third pass: the eight read entries of the three families
    /// that stay refused now each name the layer they are missing, from core's
    /// one-reason-per-layer constants, and none of them is refused with the
    /// write-gate sentence.
    ///
    /// The pairing is asserted mechanically rather than by reading the text: a
    /// table that omits a command is invisible to every other check, so every
    /// entry in the family lists is walked.
    #[test]
    fn every_refused_read_of_the_three_families_names_its_layer() {
        let expected: [(&str, Command, &str); 8] = [
            ("get-fan-curve", Command::GetFanCurve, "33-line"),
            (
                "get-legacy-fan-policy",
                Command::GetFanPolicyInfo,
                "33-line",
            ),
            (
                "get-power-ceiling",
                Command::GetPowerCeiling,
                "get-public-power-limit",
            ),
            (
                "get-power-mode",
                Command::GetPowerMode,
                "get-public-power-limit",
            ),
            (
                "get-dnotifier",
                Command::GetDNotifier,
                "get-public-power-limit",
            ),
            ("get-rated-tdp", Command::GetRatedTdp, "NV_OK"),
            (
                "get-public-temp-limit",
                Command::GetPublicTempLimit,
                "readable",
            ),
            ("get-temp-sim", Command::GetTempSim, "尚未定位"),
        ];
        for (name, command, layer_word) in expected {
            assert_eq!(command.name(), name, "the table and the CLI disagree");
            let error = validate(command).unwrap_err().to_string();
            assert!(error.contains("unavailable on macOS RM"), "{error}");
            assert!(
                error.contains(layer_word),
                "{name} does not name its layer ({layer_word}): {error}"
            );
            // The negative that makes this test worth having: the write-gate
            // sentence must not be what a *read* is told.
            assert!(
                !error.contains("Performance writes stay gated"),
                "{name} still gets the write-gate sentence: {error}"
            );
            // And the refusal must not be the bare list either: every one of
            // these families has a read that answers, so the list alone would
            // read as "no data of this kind on this path".
            assert!(!error.contains("supported commands:"), "{name}: {error}");
        }
        // The two families whose read really does answer name that entry, so a
        // user is not sent to a command that is refused as well.
        assert!(
            validate(Command::GetPowerCeiling)
                .unwrap_err()
                .to_string()
                .contains("get-public-power-limit")
        );
        assert!(
            validate(Command::GetPublicTempLimit)
                .unwrap_err()
                .to_string()
                .contains("get-legacy-temp-sensor")
        );
    }

    #[test]
    fn auto_uses_platform_backend_without_changing_explicit_choices() {
        assert_eq!(selected(BackendChoice::Auto), cfg!(target_os = "macos"));
        assert!(selected(BackendChoice::MacosRm));
        assert!(!selected(BackendChoice::Nvapi));
        assert!(!selected(BackendChoice::Nvml));
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[test]
    fn explicit_backend_on_other_platforms_reports_requirement() {
        let invocation = parse_args(["get-info", "--macos-rm"]).unwrap();
        assert!(
            run_invocation(&invocation)
                .unwrap_err()
                .to_string()
                .contains("requires x86_64 macOS")
        );
    }
}

// ---------------------------------------------------------------------------
// Continuous macOS RM monitoring (`get-status --interval [--count]`).
// ---------------------------------------------------------------------------

use nvoc_core::MacosRmSession;
use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// SIGINT flag shared with the async-signal-safe handler below.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

#[cfg(unix)]
extern "C" fn request_stop(_signum: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Installs the SIGINT handler for one continuous run and restores the previous
/// disposition on drop, so a library caller (or a second run in the same
/// process) is not left with a modified signal table. The handler only stores
/// an atomic flag, which keeps it async-signal-safe. Nothing here (or anywhere
/// in the Rust layer) can run for SIGKILL or a hard crash.
struct InterruptGuard {
    #[cfg(unix)]
    previous: usize,
}

#[cfg(unix)]
impl InterruptGuard {
    fn install() -> Self {
        const SIGINT: i32 = 2;
        // A new run must not inherit a stop request from an earlier one.
        INTERRUPTED.store(false, Ordering::SeqCst);
        let handler: extern "C" fn(i32) = request_stop;
        // SAFETY: signal() swaps one disposition for a handler that performs a
        // single atomic store; the previous disposition is restored on drop.
        let previous = unsafe { signal(SIGINT, handler as usize) };
        Self { previous }
    }
}

#[cfg(unix)]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        const SIGINT: i32 = 2;
        // SAFETY: restores exactly the disposition signal() reported.
        unsafe {
            signal(SIGINT, self.previous);
        }
    }
}

#[cfg(not(unix))]
impl InterruptGuard {
    fn install() -> Self {
        INTERRUPTED.store(false, Ordering::SeqCst);
        Self {}
    }
}

fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Sleep, returning early when Ctrl-C arrives.
fn sleep_interruptible(duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if interrupted() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    interrupted()
}

#[derive(Debug)]
pub(super) struct ContinuousOptions {
    interval: Duration,
    /// 0 = run until interrupted.
    count: u64,
}

/// Sampling cadence: one sample, then `interval` of rest, so the observed
/// period is the interval plus the sampling cost (measured: `--interval 1`
/// gives 1.000-1.011 s gaps, `--interval 2` gives 2.001-2.011 s).
///
/// Intervals below 1 ms are refused: they would busy-loop the driver instead of
/// sampling, and `Duration::from_secs_f64` rounds sub-nanosecond values to
/// zero. Values above a day are refused as typos; the same bound keeps
/// `Duration::from_secs_f64` from panicking on overflow, which in a release
/// build (`panic = "abort"`) would abort the process and skip session cleanup.
const MIN_INTERVAL_SECS: f64 = 0.001;
const MAX_INTERVAL_SECS: f64 = 86_400.0;

/// Parse `--interval`/`--count`. `Ok(None)` keeps the one-shot path.
pub(super) fn continuous_options(invocation: &Invocation) -> CliResult<Option<ContinuousOptions>> {
    let interval = option_one(invocation, "interval");
    let count = option_one(invocation, "count");
    if interval.is_none() && count.is_none() {
        return Ok(None);
    }
    let raw = interval.ok_or_else(|| {
        CliError::new("--count requires --interval (continuous macOS RM monitoring)")
    })?;
    let seconds: f64 = raw
        .parse()
        .map_err(|_| CliError::new(format!("invalid --interval {raw:?}: expected seconds")))?;
    if !seconds.is_finite() || !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&seconds) {
        return Err(CliError::new(format!(
            "--interval must be between {MIN_INTERVAL_SECS} and {MAX_INTERVAL_SECS} seconds, got {raw:?}"
        )));
    }
    let interval = Duration::from_secs_f64(seconds);
    if interval < Duration::from_millis(1) {
        return Err(CliError::new(format!(
            "--interval {raw:?} rounds to zero; use at least 1 ms"
        )));
    }
    let count = match count {
        None => 0,
        Some(raw) => raw.parse::<u64>().map_err(|_| {
            CliError::new(format!(
                "invalid --count {raw:?}: expected a non-negative integer"
            ))
        })?,
    };
    Ok(Some(ContinuousOptions { interval, count }))
}

/// (PCI address, device id, RM GPU id) — the identity a reopen must reproduce.
type Identity = (String, u32, u32);

/// Bounded reopen schedule: four attempts with 0/0.5/1/2 s of backoff, and a
/// hard cap on how many times in a row a lost session is rebuilt. A device that
/// keeps disappearing ends the run with an error instead of reopening the
/// driver forever.
const REOPEN_BACKOFF_MS: [u64; 4] = [0, 500, 1_000, 2_000];
const MAX_CONSECUTIVE_REOPENS: u32 = 8;

enum OpenOutcome {
    Opened(Box<MacosRmSession>),
    /// A stop request arrived while waiting out the backoff.
    Cancelled,
}

fn open_monitored_session(
    expected: &Identity,
    cancelled: &dyn Fn() -> bool,
) -> CliResult<OpenOutcome> {
    let mut last = String::from("no attempt made");
    for delay in REOPEN_BACKOFF_MS {
        if delay > 0 && sleep_interruptible(Duration::from_millis(delay)) {
            return Ok(OpenOutcome::Cancelled);
        }
        let opened = MacosRmSession::open_with(
            nvoc_core::MONITOR_STALE_AFTER,
            Duration::from_millis(1_500),
            cancelled,
        );
        match opened {
            Ok(session) => {
                let identity = session.identity();
                if &identity == expected {
                    return Ok(OpenOutcome::Opened(Box::new(session)));
                }
                let error = CliError::new(format!(
                    "device identity changed: expected {} device {:04x} rm {}, got {} device {:04x} rm {}",
                    expected.0, expected.1, expected.2, identity.0, identity.1, identity.2
                ));
                // Release the unexpected device before retrying.
                if let Err(close) = session.close() {
                    last = format!("{error}; release of the unexpected session failed: {close}");
                } else {
                    last = error.to_string();
                }
            }
            Err(error) => last = error.to_string(),
        }
        if cancelled() {
            return Ok(OpenOutcome::Cancelled);
        }
    }
    Err(CliError::new(format!(
        "could not open a macOS RM session after {} attempts: {last}",
        REOPEN_BACKOFF_MS.len()
    )))
}

/// Tell the user which sections had not published a new value when the session
/// opened. Those fields are reported as `never_refreshed` until the driver
/// publishes, instead of being presented as fresh data. Sections the driver
/// does not publish at all are not "pending": they settle during the open and
/// are reported per sample as `unsupported` with a reason, so they are not
/// listed here.
fn warn_missing_initial_freshness(session: &MacosRmSession) {
    let settled = session.initial_settled();
    let waiting: Vec<&str> = nvoc_core::MONITOR_SECTIONS
        .iter()
        .enumerate()
        .filter(|(index, _)| !settled[*index])
        .map(|(_, name)| *name)
        .collect();
    if !waiting.is_empty() {
        eprintln!(
            "nvoc-cli: warning: {} RUSD section(s) had not published a new value when the session opened ({}); they are reported as never_refreshed until they do",
            waiting.len(),
            waiting.join(", ")
        );
    }
}

enum SampleOutput {
    Written,
    /// The consumer closed stdout (e.g. `| head`): stop cleanly.
    ConsumerGone,
}

fn emit_sample(
    json: bool,
    sequence: u64,
    host_time: SystemTime,
    duration: Duration,
    gpu_id: u32,
    info: &nvoc_core::DeviceInfo,
    sample: &nvoc_core::DeviceSample,
) -> CliResult<SampleOutput> {
    let lines = if json {
        vec![super::output::macos_rm_sample_json(
            sequence, host_time, duration, gpu_id, info, sample,
        )?]
    } else {
        super::output::format_macos_rm_sample_human(
            sequence, host_time, duration, gpu_id, info, sample,
        )
    };
    // Stream line by line so a long run is observable and a Ctrl-C mid-run
    // still leaves every completed sample on disk. Writing through `writeln!`
    // instead of `println!` keeps a closed pipe an error to handle rather than
    // a panic: a release build aborts on panic and would skip session cleanup.
    let mut stdout = std::io::stdout().lock();
    for line in &lines {
        if let Err(error) = writeln!(stdout, "{line}") {
            if error.kind() == ErrorKind::BrokenPipe {
                return Ok(SampleOutput::ConsumerGone);
            }
            return Err(CliError::new(format!("cannot write sample: {error}")));
        }
    }
    if let Err(error) = stdout.flush() {
        if error.kind() == ErrorKind::BrokenPipe {
            return Ok(SampleOutput::ConsumerGone);
        }
        return Err(CliError::new(format!("cannot flush sample: {error}")));
    }
    Ok(SampleOutput::Written)
}

/// Build the successful "we stopped" result: continuous runs that end on a stop
/// request, a sample count or a closed consumer are not failures.
fn stopped(sequence: u64, reason: &str) -> RunOutput {
    eprintln!("nvoc-cli: stopped after {sequence} samples ({reason})");
    RunOutput {
        rendered: String::new(),
        streamed: true,
        exit_code: 0,
    }
}

pub(super) fn execute_continuous(
    invocation: &Invocation,
    options: ContinuousOptions,
) -> CliResult<RunOutput> {
    if !selected(invocation.backend) {
        return Err(CliError::new(
            "--interval/--count continuous monitoring requires the macOS RM backend",
        ));
    }
    let _interrupt = InterruptGuard::install();
    let json = invocation.output == OutputFormat::Json;
    let inventory = discover_targets(BackendSet::MacosRm)?;
    let targets = inventory.targets();
    if targets.is_empty() {
        return Err(CliError::new("no macOS RM GPU found"));
    }
    let selected_targets = select_targets(&targets, &gpu_selector(invocation))?;
    let target = match selected_targets.as_slice() {
        [target] => *target,
        _ => {
            return Err(CliError::new(
                "continuous monitoring needs exactly one target; narrow it with --gpu",
            ));
        }
    };
    let info = target.macos_rm().map_err(CliError::from)?.info().clone();
    let expected: Identity = (info.pci_address.clone(), info.device_id, info.rm_gpu_id);
    let cancelled = || interrupted();

    let mut session = match open_monitored_session(&expected, &cancelled)? {
        OpenOutcome::Opened(session) => Some(*session),
        OpenOutcome::Cancelled => return Ok(stopped(0, "Ctrl-C")),
    };
    if let Some(session) = session.as_ref() {
        warn_missing_initial_freshness(session);
    }

    let mut sequence: u64 = 0;
    let mut consecutive_reopens: u32 = 0;
    loop {
        if interrupted() {
            break;
        }
        let live = session
            .as_mut()
            .expect("a live session is always present inside the loop");
        let started = Instant::now();
        let sample = match live.sample() {
            Ok(sample) => {
                consecutive_reopens = 0;
                sample
            }
            Err(error) => {
                if consecutive_reopens >= MAX_CONSECUTIVE_REOPENS {
                    return Err(CliError::new(format!(
                        "macOS RM session lost ({error}) and {consecutive_reopens} consecutive reconnects did not help"
                    )));
                }
                consecutive_reopens += 1;
                eprintln!(
                    "nvoc-cli: macOS RM session lost ({error}); reconnecting ({consecutive_reopens}/{MAX_CONSECUTIVE_REOPENS})"
                );
                // Release the old session *before* opening a new one: the RM
                // transport and the driver's poller are process-global, so two
                // live chains would compete for the same resources.
                let old = session
                    .take()
                    .expect("a live session is always present inside the loop");
                let release = old.close().err();
                if cancelled() {
                    break;
                }
                match open_monitored_session(&expected, &cancelled)? {
                    OpenOutcome::Opened(new) => {
                        if let Some(error) = release {
                            eprintln!(
                                "nvoc-cli: warning: the previous session reported a release failure: {error}"
                            );
                        }
                        warn_missing_initial_freshness(&new);
                        session = Some(*new);
                    }
                    OpenOutcome::Cancelled => break,
                }
                continue;
            }
        };
        let duration = started.elapsed();
        sequence += 1;
        match emit_sample(
            json,
            sequence,
            SystemTime::now(),
            duration,
            target.id.0,
            &info,
            &sample,
        )? {
            SampleOutput::Written => {}
            SampleOutput::ConsumerGone => return Ok(stopped(sequence, "stdout closed")),
        }
        if options.count != 0 && sequence >= options.count {
            break;
        }
        if sleep_interruptible(options.interval) {
            break;
        }
    }
    // Explicit release: a mapping that cannot be unmapped is reported instead
    // of being swallowed, and the caller still gets a clean exit afterwards.
    if let Some(session) = session.take()
        && let Err(error) = session.close()
    {
        return Err(CliError::new(format!("macOS RM session release: {error}")));
    }
    let reason = if interrupted() {
        "Ctrl-C"
    } else {
        "sample count"
    };
    Ok(stopped(sequence, reason))
}

#[cfg(test)]
mod continuous_tests {
    use super::*;

    fn options(args: &[&str]) -> CliResult<Option<ContinuousOptions>> {
        let invocation = parse_args(args.iter().copied()).unwrap();
        continuous_options(&invocation)
    }

    #[test]
    fn single_shot_commands_never_enter_the_continuous_path() {
        assert!(options(&["get-status", "--macos-rm"]).unwrap().is_none());
        assert!(
            options(&["get-status", "--macos-rm", "--verbose"])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn interval_accepts_fractional_seconds_and_defaults_count_to_unbounded() {
        let parsed = options(&["get-status", "--macos-rm", "--interval", "0.5"])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.interval, Duration::from_millis(500));
        assert_eq!(parsed.count, 0);
        let parsed = options(&[
            "get-status",
            "--macos-rm",
            "--interval",
            "1",
            "--count",
            "3",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.interval, Duration::from_secs(1));
        assert_eq!(parsed.count, 3);
    }

    #[test]
    fn interval_boundaries_are_refused_instead_of_rounding_or_panicking() {
        for bad in ["0", "0.0001", "nan", "inf", "abc", "1e30", "86401"] {
            let error = options(&["get-status", "--macos-rm", "--interval", bad])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("--interval must be between")
                    || error.contains("rounds to zero")
                    || error.contains("expected seconds"),
                "{bad}: {error}"
            );
        }
        // A leading "-" is an option to the parser, so negatives never reach
        // the interval logic at all.
        for bad in ["-1", "-inf"] {
            assert!(
                parse_args(["get-status", "--macos-rm", "--interval", bad]).is_err(),
                "{bad} must be rejected as an option"
            );
        }
        // One millisecond and one day are both accepted.
        assert_eq!(
            options(&["get-status", "--macos-rm", "--interval", "0.001"])
                .unwrap()
                .unwrap()
                .interval,
            Duration::from_millis(1)
        );
        assert_eq!(
            options(&["get-status", "--macos-rm", "--interval", "86400"])
                .unwrap()
                .unwrap()
                .interval,
            Duration::from_secs(86_400)
        );
    }

    #[test]
    fn count_requires_interval_and_rejects_non_integers() {
        let error = options(&["get-status", "--macos-rm", "--count", "3"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("--count requires --interval"), "{error}");
        let error = options(&[
            "get-status",
            "--macos-rm",
            "--interval",
            "1",
            "--count",
            "abc",
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid --count"), "{error}");
        // A negative count is an option to the parser, never a value we parse.
        assert!(
            parse_args([
                "get-status",
                "--macos-rm",
                "--interval",
                "1",
                "--count",
                "-1"
            ])
            .is_err()
        );
        // Explicit zero means "until stopped".
        assert_eq!(
            options(&[
                "get-status",
                "--macos-rm",
                "--interval",
                "1",
                "--count",
                "0"
            ])
            .unwrap()
            .unwrap()
            .count,
            0
        );
    }
}
