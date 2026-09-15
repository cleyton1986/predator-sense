use predator_sense_protocol::helper::{
    Action as HelperAction, FanMode as HelperFanMode, PwmControlMode, PERCENT_MAX, PWM_VALUE_MAX,
};

/// Fan control modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FanMode {
    Auto,
    Max,
    Custom(u8, u8), // cpu_percent, gpu_percent
}

/// Set fan mode using the predator-sense-helper (requires pkexec)
/// Auto and Max use firmware modes (safe). Custom is disabled for safety.
pub fn set_fan_mode(mode: FanMode) -> Result<(), String> {
    crate::hardware::applog::info(&format!("fan: mode {mode:?}"));
    use crate::hardware::capabilities::FanPresetStatus;

    let action = match mode {
        FanMode::Auto => HelperAction::FanAuto,
        FanMode::Max => HelperAction::FanMax,
        FanMode::Custom(_, _) => return Err(crate::i18n::t("fan_note").to_string()),
    };
    // The bytes this write sends were only ever hand-verified on a PH315-54
    // (see `get_fan_mode`'s doc comment) - see
    // `capabilities::fan_preset_status_for` for the full per-model picture.
    match crate::hardware::capabilities::get().fan_preset_status {
        // A real report (issue #1) already confirmed this model's EC
        // firmware disagrees with the PH315-54 values - sending them does
        // nothing trustworthy, so refuse instead of pretending it worked.
        // Unless the kernel exposes the predator_v4 PWM path: there the
        // same two presets exist as WMI fan-behavior modes (verified on a
        // PH16-71 - full speed ~6000 RPM, auto back to the EC curve).
        FanPresetStatus::KnownIncompatible => {
            if pwm_available() {
                return match mode {
                    FanMode::Auto => set_pwm_auto(),
                    _ => set_pwm_full_speed(),
                };
            }
            return Err(crate::i18n::t("fan_ec_incompatible").to_string());
        }
        // No report either way. Withholding fan control on every unlisted
        // model would be worse than an unverified write, so still send it -
        // just make the gap visible instead of assuming it behaves the same
        // everywhere.
        FanPresetStatus::Unverified => {
            // The official app never writes EC fan bytes on the predator_v4
            // generation, it only drives the WMI fan-behavior method that the
            // kernel's PWM path wraps. When that path is there, use it: the
            // EC preset bytes were measured to move no air on the PH16-71
            // and only flash the mode key (issue #82 reports the same).
            if pwm_available() {
                return match mode {
                    FanMode::Auto => set_pwm_auto(),
                    _ => set_pwm_full_speed(),
                };
            }
            crate::hardware::applog::info(&format!(
                "fan preset bytes are unverified on this model ({}); sending the \
                 PH315-54 values anyway",
                crate::hardware::capabilities::get().model
            ));
        }
        FanPresetStatus::Verified => {}
    }
    // On this chassis the EC has two internal fan-control states: a static
    // preset (all `FanAuto`'s own write can ever reach on its own - two
    // fixed setpoints, no real curve) and a dynamic one that actually
    // follows load. The only known way to switch it into the dynamic state
    // is a real transition on the WMI `ThermalProfile` index - confirmed by
    // hand, see `PROTOCOLO-HARDWARE.md` §9.2. Only Auto needs this: Max is
    // supposed to sit at its fixed setpoint, not follow a curve.
    if mode == FanMode::Auto {
        wake_dynamic_fan_curve();
    }
    crate::hardware::helper::execute(action, &[])
}

/// Bounces the firmware thermal-profile index off itself through another
/// supported one and back, as a side effect that wakes the EC's real fan
/// curve - see `set_fan_mode`'s doc comment. Round-trips back to the same
/// index so the "Mode" page's firmware profile never actually changes from
/// the user's point of view.
///
/// Best-effort and silent on the common failure paths (unavailable, or only
/// one supported index to begin with - nothing to bounce through) since this
/// is a bonus wake-up, not the fan mode change itself. Logs if the bounce
/// left the profile somewhere other than where it started, since that *is* a
/// real, visible side effect a caller did not ask for.
/// Only reached from the EC-preset path in `set_fan_mode`, which a PH16-71
/// never takes - it is `KnownIncompatible` there, so Auto/Max route through
/// PWM instead. The PWM path's own use of this was removed after the stall it
/// worked around turned out not to exist; that measurement does not cover the
/// EC path on the models that do use it, so this is left alone rather than
/// deleted on a guess.
pub fn wake_dynamic_fan_curve() {
    use crate::hardware::thermal_profile;
    if !thermal_profile::is_available() {
        return;
    }
    let Some(current) = thermal_profile::current() else {
        return;
    };
    let Some(&other) = thermal_profile::supported().iter().find(|&&i| i != current) else {
        return;
    };
    if let Err(e) = thermal_profile::set(other) {
        crate::hardware::applog::info(&format!(
            "fan auto-curve wake skipped: could not set thermal profile {other}: {e}"
        ));
        return;
    }
    if let Err(e) = thermal_profile::set(current) {
        crate::hardware::applog::error(&format!(
            "fan auto-curve wake left the firmware power profile at {other} instead of \
             restoring {current}: {e}"
        ));
    }
}

/// Reads back the firmware fan mode actually active right now (EC offsets
/// 0x21/0x22, the same ones `set_fan_mode`'s Auto/Max write) - `None` if
/// unreadable or the bytes don't match either known written value. This is
/// what makes the fan-control page trustworthy: the physical Predator key
/// on the keyboard also flips this mode directly at the EC level (through
/// facer.ko, entirely outside this app), so "whatever we last wrote"
/// wouldn't be enough - only reading the EC back catches that too.
/// Verified by hand: writing Auto then reading back gives (0x50, 0x54)
/// exactly; writing Max gives (0x60, 0x58) exactly, both stable.
pub fn get_fan_mode() -> Option<FanMode> {
    match HelperFanMode::parse(&crate::hardware::helper::read(HelperAction::FanModeRead)?)? {
        HelperFanMode::Automatic => Some(FanMode::Auto),
        HelperFanMode::Maximum => Some(FanMode::Max),
    }
}

/// Toggle CoolBoost on/off
pub fn set_coolboost(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::CoolBoost, enabled)
}

/// Read CoolBoost state from EC
pub fn get_coolboost() -> bool {
    crate::hardware::helper::read_switch(HelperAction::CoolBoostRead).unwrap_or(false)
}

/// True if the kernel exposes hwmon PWM control (kernel >= 6.14 + ACER_CAP_PWM model).
/// EXPERIMENTAL — only available on a subset of Predator/Nitro models.
pub fn pwm_available() -> bool {
    crate::hardware::helper::read(HelperAction::PwmAvailable)
        .map(|value| value == "1")
        .unwrap_or(false)
}

/// Set CPU/GPU fan speed as a percentage (0-100). Writes hwmon pwm (0-255).
/// Switches the fan to manual/custom mode first.
pub fn set_pwm_percent(cpu_pct: u8, gpu_pct: u8) -> Result<(), String> {
    crate::hardware::applog::info(&format!(
        "fan: manual pwm cpu={cpu_pct}% gpu={gpu_pct}%"
    ));
    let manual = PwmControlMode::Manual.as_str();
    crate::hardware::helper::execute(HelperAction::PwmCpuEnable, &[manual])?;
    crate::hardware::helper::execute(HelperAction::PwmGpuEnable, &[manual])?;
    let cpu = (u16::from(cpu_pct).min(PERCENT_MAX) * PWM_VALUE_MAX) / PERCENT_MAX;
    let gpu = (u16::from(gpu_pct).min(PERCENT_MAX) * PWM_VALUE_MAX) / PERCENT_MAX;
    crate::hardware::helper::execute(HelperAction::PwmCpu, &[&cpu.to_string()])?;
    crate::hardware::helper::execute(HelperAction::PwmGpu, &[&gpu.to_string()])?;
    Ok(())
}

/// Restore automatic fan control (pwm_enable=2) on both fans.
///
/// This used to be followed by a thermal-profile bounce, on the belief that
/// `pwm_enable = 2` left the EC in a static state reporting 0 RPM until the
/// profile index was transitioned. Re-measured on a PH16-71: that was a
/// misdiagnosis. 0 RPM at idle is the firmware curve working - this chassis
/// runs fanless when cool - and the fans ramp on their own under load with no
/// bounce at all (0 RPM at 42 C idle, unchanged by a bounce; 2913/2902 RPM at
/// 91 C under load). The bounce was a visible mode-key flicker and a chance of
/// being left in the wrong profile, bought nothing, and is gone.
pub fn set_pwm_auto() -> Result<(), String> {
    crate::hardware::applog::info("fan: firmware curve (pwm_enable=2)");
    let automatic = PwmControlMode::Automatic.as_str();
    crate::hardware::helper::execute(HelperAction::PwmCpuEnable, &[automatic])?;
    crate::hardware::helper::execute(HelperAction::PwmGpuEnable, &[automatic])?;
    Ok(())
}

/// Firmware "max fan" through the PWM path: fan-behavior mode 0 (turbo /
/// full speed) on both fans - what the physical Turbo key selects.
pub fn set_pwm_full_speed() -> Result<(), String> {
    let full = PwmControlMode::FullSpeed.as_str();
    crate::hardware::helper::execute(HelperAction::PwmCpuEnable, &[full])?;
    crate::hardware::helper::execute(HelperAction::PwmGpuEnable, &[full])?;
    Ok(())
}

/// Which temperature the software curve should answer to.
///
/// The hotter of the two, because the curve drives one speed for both fans and
/// the discrete GPU is the half that was ignored entirely: a machine loading
/// the GPU with an idle CPU got no fan response at all, on every model, which
/// is the dangerous direction of that bug.
///
/// Deliberately not a per-fan curve. Whether the two fans are thermally
/// independent is a property of each chassis's heatsink, and several Acer
/// designs share heatpipes across both dies, where driving the GPU fan from
/// GPU temperature alone would starve the CPU under load. This app runs on
/// hardware that cannot all be measured, so it takes the safe reading rather
/// than assuming a layout.
///
/// A `None` GPU is the common case, not an error: no discrete GPU, no
/// `nvidia-smi`, or an AMD card. A dGPU parked in D3cold can report `0`, which
/// simply loses the comparison.
pub fn curve_input_temp(cpu: Option<f64>, gpu: Option<f64>) -> Option<f64> {
    match (cpu, gpu) {
        (Some(cpu), Some(gpu)) => Some(cpu.max(gpu)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The 6 fixed temperature breakpoints (<45/<55/<65/<75/<85/85+ °C) the
/// software auto-curve steps through. Not user-editable, only the percent
/// each step applies is (see `config::fan_curve_points`, issue #59).
const FAN_CURVE_BREAKPOINTS_C: [f64; 5] = [45.0, 55.0, 65.0, 75.0, 85.0];

/// Original hardcoded curve, kept as the default for anyone who never opens
/// the new per-step editor in Fan Control.
pub const DEFAULT_FAN_CURVE: [u8; 6] = [25, 35, 50, 65, 80, 100];

/// CPU-temperature to fan-speed curve (percent) for the software auto-curve
/// toggle on the Fan Control page, using the given 6 step percentages
/// (`config::fan_curve_points`) against the fixed breakpoints above.
pub fn fan_curve_pct(temp_c: f64, steps: &[u8; 6]) -> u8 {
    for (i, &breakpoint) in FAN_CURVE_BREAKPOINTS_C.iter().enumerate() {
        if temp_c < breakpoint {
            return steps[i];
        }
    }
    steps[5]
}

/// Temperature at which the curve takes manual control back after handing the
/// fans to the firmware, or `None` for a curve that is zero everywhere.
///
/// Deliberately not the boundary of the zero region, and not that boundary
/// plus a couple of degrees. With the fans stopped the temperature climbs past
/// any small buffer on its own, and the first non-zero step then pulls it
/// straight back below the boundary, so the machine alternates between silence
/// and a spinning fan for as long as it sits idle. Measured on a PH16-71 with
/// a 3 C buffer and a `[0, 35, ...]` curve: 0 RPM at 45 C, 2500 RPM at 48 C,
/// back to 0 RPM at 45 C, on a roughly one-minute cycle. A fan that cycles is
/// worse than a fan that holds a low speed.
///
/// So the whole first non-zero band is left to the firmware, whose own ramp is
/// gentler than any step here, and the curve resumes at the top of that band.
/// The result is the wide asymmetric gap that hardware zero-RPM modes use
/// (stop low, start high): with `[0, 35, 50, ...]` the fans are the firmware's
/// below 55 C and the curve's above it, and once the curve has them it keeps
/// them until the temperature falls back under 45 C. At idle the firmware
/// simply holds the temperature inside that band and nothing cycles; under a
/// real load the temperature crosses the top in seconds.
fn curve_resume_c(steps: &[u8; 6]) -> Option<f64> {
    let first_active = steps.iter().position(|&pct| pct != 0)?;
    // A curve whose only non-zero step is the top one resumes at that step's
    // own boundary: there is no band above it to defer to.
    Some(FAN_CURVE_BREAKPOINTS_C[first_active.min(FAN_CURVE_BREAKPOINTS_C.len() - 1)])
}

/// What the software curve should do on this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveAction {
    /// Hand both fans to the firmware curve.
    ///
    /// This is the only way to reach 0 RPM. A manual `pwm` of 0 is not off:
    /// the EC holds a floor, measured at ~1670 RPM on a PH16-71 at idle and
    /// under load alike, so a user who sets a step to 0% gets a spinning fan
    /// unless the firmware takes over. Below its own threshold the firmware
    /// runs the machine genuinely fanless (0 RPM up to ~48 C on that chassis),
    /// which is what 0% is asking for.
    Firmware,
    /// Drive both fans at this percentage.
    Manual(u8),
    /// Write nothing. Either the firmware already has the fans and the curve
    /// still wants 0%, or the temperature has not yet cleared the hysteresis
    /// band, and taking manual control back would undo the silence.
    Hold,
}

/// What the curve last successfully applied to the hardware.
///
/// `Unknown` covers startup and any write that failed. The hardware then holds
/// whatever the firmware, a previous session or another writer left, so the
/// curve has to assert itself rather than trust a value it never managed to
/// apply. That distinction is load-bearing: a flag saying "handed off" while
/// the write actually failed is how the curve and the hardware came to
/// disagree silently, with the app convinced it had done something it had not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanState {
    Unknown,
    Firmware,
    Manual(u8),
}

/// Whether `action` still needs to reach the hardware.
///
/// The curve runs every few seconds and its answer rarely changes, so without
/// this check every tick spent four privileged helper round trips restating
/// the percentage already in force: 84 writes in 6.5 idle minutes, measured.
pub fn needs_write(action: CurveAction, applied: FanState) -> bool {
    match (action, applied) {
        (CurveAction::Hold, _) => false,
        (CurveAction::Firmware, FanState::Firmware) => false,
        (CurveAction::Manual(wanted), FanState::Manual(in_force)) => wanted != in_force,
        _ => true,
    }
}

/// Decides between the firmware curve and a manual percentage, given where the
/// temperature is and whether the firmware currently has control.
///
/// A step of 0% means "stop", which only the firmware can do. Coming back out
/// of that needs the hysteresis buffer, or a temperature hovering at the
/// boundary would hand the fans back and forth every tick.
pub fn curve_action(temp_c: f64, steps: &[u8; 6], handed_off: bool) -> CurveAction {
    if fan_curve_pct(temp_c, steps) == 0 {
        // Nothing to write once the firmware already has it.
        return if handed_off {
            CurveAction::Hold
        } else {
            CurveAction::Firmware
        };
    }
    // Once the firmware has them, it keeps them until the temperature clears
    // the whole first non-zero band. See curve_resume_c for why that is the
    // band and not the boundary.
    //
    // `steps[0] == 0` guards the case where the user edits the zero step away
    // while the firmware holds the fans: there is no longer a band to defer,
    // they have asked for manual control at this temperature, and making them
    // wait for a boundary their curve no longer has would look like the
    // setting being ignored.
    if handed_off
        && steps[0] == 0
        && curve_resume_c(steps).is_none_or(|resume| temp_c < resume)
    {
        return CurveAction::Hold;
    }
    CurveAction::Manual(fan_curve_pct(temp_c, steps))
}

/// Resolves a mode's plan into what the reconciler should do this tick.
///
/// `temp_c` is the hotter die from `curve_input_temp`. `handed_off` is whether
/// the firmware currently holds the fans, which only `Curve` and `Automatic`
/// care about.
pub fn plan_target(
    plan: crate::config::FanPlan,
    temp_c: Option<f64>,
    handed_off: bool,
) -> CurveAction {
    use crate::config::FanPlan;
    match plan {
        FanPlan::Automatic => {
            if handed_off {
                CurveAction::Hold
            } else {
                CurveAction::Firmware
            }
        }
        FanPlan::Max => CurveAction::Manual(100),
        FanPlan::Fixed { percent } => CurveAction::Manual(percent.min(100)),
        FanPlan::Curve { steps } => match temp_c {
            Some(temp_c) => curve_action(temp_c, &steps, handed_off),
            // No sensor: writing a guessed percentage is how fans get pinned.
            None => CurveAction::Hold,
        },
    }
}

/// Read current CPU/GPU fan PWM as percentage (0-100), if available.
pub fn get_pwm_percent() -> Option<(u8, u8)> {
    let cpu: u16 = crate::hardware::helper::read(HelperAction::PwmCpuRead)?
        .parse()
        .ok()?;
    let gpu: u16 = crate::hardware::helper::read(HelperAction::PwmGpuRead)?
        .parse()
        .ok()?;
    Some((
        ((cpu * PERCENT_MAX) / PWM_VALUE_MAX) as u8,
        ((gpu * PERCENT_MAX) / PWM_VALUE_MAX) as u8,
    ))
}

/// The plan bound to a raw mode id, for callers that already have the id.
pub fn plan_for_id(
    plans: &[crate::config::FanBinding],
    mode: &str,
) -> Option<crate::config::FanPlan> {
    plans
        .iter()
        .find(|binding| binding.mode == mode)
        .map(|binding| binding.plan)
}

/// The plan bound to `profile`, or `Automatic` when there is none.
///
/// `Automatic` is the safe fallback in every unknown case: the firmware can
/// always cool the machine, and it is also what an un-migrated config means.
pub fn plan_for(
    profile: Option<crate::hardware::profile::PowerProfile>,
    plans: &[crate::config::FanBinding],
) -> crate::config::FanPlan {
    let Some(profile) = profile else {
        return crate::config::FanPlan::Automatic;
    };
    plan_for_id(plans, profile.to_id()).unwrap_or(crate::config::FanPlan::Automatic)
}

/// Builds the initial per-mode plans from the pre-existing global fan
/// settings, so migrating changes nothing about how the machine behaves until
/// a mode is deliberately given its own plan.
///
/// This is the only remaining reader of `fan_auto_curve_enabled`,
/// `fan_curve_points` and `fan_mode`. Every other consumer uses `fan_plans`,
/// or the two would drift into exactly the disagreement this design removes.
pub fn migrate_plans(cfg: &crate::config::AppConfig) -> Vec<crate::config::FanBinding> {
    use crate::config::{FanBinding, FanPlan};
    use crate::hardware::profile::PowerProfile;

    let plan = if cfg.fan_auto_curve_enabled {
        FanPlan::Curve { steps: cfg.fan_curve_points }
    } else {
        match cfg.fan_mode.as_deref() {
            Some("max") => FanPlan::Max,
            // "auto", anything unrecognised, and never-set all mean the
            // firmware was in charge, which is Automatic.
            _ => FanPlan::Automatic,
        }
    };

    [
        PowerProfile::Eco,
        PowerProfile::Quiet,
        PowerProfile::Balanced,
        PowerProfile::Performance,
        PowerProfile::Turbo,
    ]
    .into_iter()
    .map(|profile| FanBinding {
        mode: profile.to_id().to_string(),
        plan,
    })
    .collect()
}

/// Narrows a plan to what this machine can actually do.
///
/// `Curve` and `Fixed` need per-fan PWM. Without `ACER_CAP_PWM` there is no
/// `pwm1` to write, so they are unreachable through the UI, but they can still
/// arrive in a config copied from a machine that has it. Resolving to
/// `Automatic` keeps the fans governed by something rather than leaving a plan
/// that silently does nothing.
pub fn plan_for_hardware(
    plan: crate::config::FanPlan,
    pwm_available: bool,
) -> crate::config::FanPlan {
    use crate::config::FanPlan;
    match plan {
        FanPlan::Curve { .. } | FanPlan::Fixed { .. } if !pwm_available => FanPlan::Automatic,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_curve_matches_the_original_hardcoded_steps() {
        let steps = DEFAULT_FAN_CURVE;
        assert_eq!(fan_curve_pct(20.0, &steps), 25);
        assert_eq!(fan_curve_pct(44.9, &steps), 25);
        assert_eq!(fan_curve_pct(45.0, &steps), 35);
        assert_eq!(fan_curve_pct(54.9, &steps), 35);
        assert_eq!(fan_curve_pct(55.0, &steps), 50);
        assert_eq!(fan_curve_pct(64.9, &steps), 50);
        assert_eq!(fan_curve_pct(65.0, &steps), 65);
        assert_eq!(fan_curve_pct(74.9, &steps), 65);
        assert_eq!(fan_curve_pct(75.0, &steps), 80);
        assert_eq!(fan_curve_pct(84.9, &steps), 80);
        assert_eq!(fan_curve_pct(85.0, &steps), 100);
        assert_eq!(fan_curve_pct(99.0, &steps), 100);
    }

    #[test]
    fn a_custom_curve_is_honored_at_every_step() {
        // harry42203's complaint (issue #59): quieter at low load, more
        // aggressive at high load than the default.
        let steps = [10, 15, 30, 70, 90, 100];
        assert_eq!(fan_curve_pct(30.0, &steps), 10);
        assert_eq!(fan_curve_pct(50.0, &steps), 15);
        assert_eq!(fan_curve_pct(60.0, &steps), 30);
        assert_eq!(fan_curve_pct(70.0, &steps), 70);
        assert_eq!(fan_curve_pct(80.0, &steps), 90);
        assert_eq!(fan_curve_pct(90.0, &steps), 100);
    }

    #[test]
    fn the_hotter_die_drives_the_curve() {
        assert_eq!(curve_input_temp(Some(50.0), Some(80.0)), Some(80.0));
        assert_eq!(curve_input_temp(Some(85.0), Some(40.0)), Some(85.0));
    }

    #[test]
    fn one_sensor_is_enough() {
        // No discrete GPU, no nvidia-smi, or an AMD card.
        assert_eq!(curve_input_temp(Some(60.0), None), Some(60.0));
        assert_eq!(curve_input_temp(None, Some(60.0)), Some(60.0));
    }

    #[test]
    fn a_parked_gpu_reporting_zero_loses() {
        assert_eq!(curve_input_temp(Some(55.0), Some(0.0)), Some(55.0));
    }

    #[test]
    fn no_reading_means_the_fans_are_left_alone() {
        assert_eq!(curve_input_temp(None, None), None);
    }

    #[test]
    fn the_gpu_half_now_reaches_the_curve() {
        // The bug this fixes: an idle CPU beside a hot GPU asked for 25%.
        let steps = DEFAULT_FAN_CURVE;
        let idle_cpu = Some(40.0);
        let hot_gpu = Some(84.0);
        assert_eq!(
            fan_curve_pct(curve_input_temp(idle_cpu, hot_gpu).unwrap(), &steps),
            80
        );
        assert_eq!(fan_curve_pct(idle_cpu.unwrap(), &steps), 25);
    }
    #[test]
    fn a_zero_step_hands_the_fans_to_the_firmware() {
        let steps = [0, 35, 50, 65, 80, 100];
        // Only the firmware can actually stop them; manual 0 still spins.
        assert_eq!(curve_action(40.0, &steps, false), CurveAction::Firmware);
        // Once it has them, there is nothing to write each tick.
        assert_eq!(curve_action(40.0, &steps, true), CurveAction::Hold);
    }

    #[test]
    fn the_firmware_keeps_the_fans_through_the_whole_first_band() {
        let steps = [0, 35, 50, 65, 80, 100];
        // The measured failure mode: resuming just above 45 C made the fans
        // cycle between 0 and 2500 RPM every minute, because stopped fans
        // warm the machine past the boundary and 35% cools it back under.
        assert_eq!(curve_action(46.0, &steps, true), CurveAction::Hold);
        assert_eq!(curve_action(50.0, &steps, true), CurveAction::Hold);
        assert_eq!(curve_action(54.9, &steps, true), CurveAction::Hold);
        // At the top of that band the curve takes over, at the step for the
        // temperature it actually reached.
        assert_eq!(curve_action(55.0, &steps, true), CurveAction::Manual(50));
        assert_eq!(curve_action(80.0, &steps, true), CurveAction::Manual(80));
    }

    #[test]
    fn once_the_curve_has_them_it_keeps_them_until_the_zero_step() {
        let steps = [0, 35, 50, 65, 80, 100];
        // The other half of the asymmetric gap: manual control is held all
        // the way down to the zero region rather than handing back at 55 C.
        assert_eq!(curve_action(54.0, &steps, false), CurveAction::Manual(35));
        assert_eq!(curve_action(45.0, &steps, false), CurveAction::Manual(35));
        assert_eq!(curve_action(44.0, &steps, false), CurveAction::Firmware);
    }

    #[test]
    fn the_band_follows_whichever_steps_are_zero() {
        // Two zero steps: the firmware owns everything below 65 C.
        let steps = [0, 0, 50, 65, 80, 100];
        assert_eq!(curve_action(50.0, &steps, true), CurveAction::Hold);
        assert_eq!(curve_action(64.0, &steps, true), CurveAction::Hold);
        assert_eq!(curve_action(65.0, &steps, true), CurveAction::Manual(65));
    }

    #[test]
    fn a_curve_that_is_zero_until_the_top_step_resumes_at_its_boundary() {
        let steps = [0, 0, 0, 0, 0, 100];
        assert_eq!(curve_action(80.0, &steps, true), CurveAction::Hold);
        assert_eq!(curve_action(85.0, &steps, true), CurveAction::Manual(100));
    }

    #[test]
    fn a_curve_without_zeros_never_hands_off() {
        let steps = DEFAULT_FAN_CURVE;
        assert_eq!(curve_action(20.0, &steps, false), CurveAction::Manual(25));
        assert_eq!(curve_action(90.0, &steps, false), CurveAction::Manual(100));
    }

    #[test]
    fn editing_the_zero_step_away_takes_the_fans_back_immediately() {
        // The firmware is holding them from a previous `[0, ...]` curve and
        // the user has just set the first step to 25%. Waiting for a band the
        // curve no longer has would read as the setting being ignored.
        let steps = DEFAULT_FAN_CURVE;
        assert_eq!(curve_action(20.0, &steps, true), CurveAction::Manual(25));
    }

    #[test]
    fn an_all_zero_curve_just_means_leave_it_to_the_firmware() {
        let steps = [0; 6];
        assert_eq!(curve_action(95.0, &steps, false), CurveAction::Firmware);
        assert_eq!(curve_action(95.0, &steps, true), CurveAction::Hold);
    }

    #[test]
    fn an_unchanged_manual_percentage_is_not_rewritten() {
        // The defect this fixes: the curve rewrote the same percentage every
        // tick, four privileged helper calls at a time, 84 writes in 6.5
        // minutes of a machine doing nothing.
        assert!(!needs_write(CurveAction::Manual(35), FanState::Manual(35)));
    }

    #[test]
    fn a_changed_manual_percentage_is_written() {
        assert!(needs_write(CurveAction::Manual(50), FanState::Manual(35)));
    }

    #[test]
    fn hold_never_writes() {
        assert!(!needs_write(CurveAction::Hold, FanState::Firmware));
        assert!(!needs_write(CurveAction::Hold, FanState::Manual(35)));
        assert!(!needs_write(CurveAction::Hold, FanState::Unknown));
    }

    #[test]
    fn a_repeated_handoff_is_not_rewritten() {
        assert!(!needs_write(CurveAction::Firmware, FanState::Firmware));
    }

    #[test]
    fn switching_between_firmware_and_manual_is_written() {
        assert!(needs_write(CurveAction::Firmware, FanState::Manual(35)));
        assert!(needs_write(CurveAction::Manual(35), FanState::Firmware));
    }

    #[test]
    fn nothing_is_assumed_before_the_first_successful_write() {
        // Startup, or after a write failed: the hardware is whatever the
        // firmware or a previous session left, so the curve has to assert
        // itself rather than trusting a value it never applied.
        assert!(needs_write(CurveAction::Manual(35), FanState::Unknown));
        assert!(needs_write(CurveAction::Firmware, FanState::Unknown));
    }

    #[test]
    fn automatic_leaves_the_fans_to_the_firmware() {
        use crate::config::FanPlan;
        assert_eq!(
            plan_target(FanPlan::Automatic, Some(70.0), false),
            CurveAction::Firmware
        );
        // Already handed off: nothing to write every tick.
        assert_eq!(
            plan_target(FanPlan::Automatic, Some(70.0), true),
            CurveAction::Hold
        );
    }

    #[test]
    fn fixed_and_max_are_flat_percentages() {
        use crate::config::FanPlan;
        assert_eq!(
            plan_target(FanPlan::Fixed { percent: 40 }, Some(70.0), false),
            CurveAction::Manual(40)
        );
        assert_eq!(
            plan_target(FanPlan::Max, Some(30.0), false),
            CurveAction::Manual(100)
        );
    }

    #[test]
    fn a_curve_plan_uses_the_curve_rules() {
        use crate::config::FanPlan;
        let plan = FanPlan::Curve { steps: [0, 35, 50, 65, 80, 100] };
        // Below the zero step: hand off, exactly as curve_action decides.
        assert_eq!(plan_target(plan, Some(40.0), false), CurveAction::Firmware);
        // Inside the firmware band after a handoff: hold.
        assert_eq!(plan_target(plan, Some(50.0), true), CurveAction::Hold);
        // Above the band: the curve takes over.
        assert_eq!(plan_target(plan, Some(70.0), true), CurveAction::Manual(65));
    }

    #[test]
    fn no_temperature_reading_means_write_nothing() {
        use crate::config::FanPlan;
        // A curve cannot be evaluated without a sensor, and guessing a
        // percentage for hardware whose temperature is unknown is how fans end
        // up pinned. Flat plans do not need one.
        assert_eq!(
            plan_target(FanPlan::Curve { steps: DEFAULT_FAN_CURVE }, None, false),
            CurveAction::Hold
        );
        assert_eq!(
            plan_target(FanPlan::Max, None, false),
            CurveAction::Manual(100)
        );
    }

    #[test]
    fn the_plan_for_the_active_mode_is_used() {
        use crate::config::{FanBinding, FanPlan};
        use crate::hardware::profile::PowerProfile;
        let plans = vec![
            FanBinding { mode: "balanced".into(), plan: FanPlan::Automatic },
            FanBinding { mode: "turbo".into(), plan: FanPlan::Max },
        ];
        assert_eq!(plan_for(Some(PowerProfile::Balanced), &plans), FanPlan::Automatic);
        assert_eq!(plan_for(Some(PowerProfile::Turbo), &plans), FanPlan::Max);
    }

    #[test]
    fn a_mode_with_no_plan_falls_back_to_automatic() {
        use crate::config::{FanBinding, FanPlan};
        use crate::hardware::profile::PowerProfile;
        let plans = vec![FanBinding { mode: "turbo".into(), plan: FanPlan::Max }];
        // Automatic is the safe fallback: the firmware is always able to cool
        // the machine, and it is what an un-migrated config means.
        assert_eq!(plan_for(Some(PowerProfile::Quiet), &plans), FanPlan::Automatic);
        assert_eq!(plan_for(None, &plans), FanPlan::Automatic);
    }

    #[test]
    fn an_unbound_mode_id_has_no_plan() {
        use crate::config::{FanBinding, FanPlan};
        let plans = vec![FanBinding { mode: "turbo".into(), plan: FanPlan::Max }];
        assert_eq!(plan_for_id(&plans, "turbo"), Some(FanPlan::Max));
        assert_eq!(plan_for_id(&plans, "eco"), None);
    }

    #[test]
    fn the_curve_migrates_to_every_mode() {
        use crate::config::{AppConfig, FanPlan};
        let mut cfg = AppConfig::default();
        cfg.fan_auto_curve_enabled = true;
        cfg.fan_curve_points = [0, 35, 50, 65, 80, 100];
        let plans = migrate_plans(&cfg);
        assert_eq!(plans.len(), 5);
        for binding in &plans {
            assert_eq!(binding.plan, FanPlan::Curve { steps: [0, 35, 50, 65, 80, 100] });
        }
    }

    #[test]
    fn a_disabled_curve_migrates_to_the_saved_fan_mode() {
        use crate::config::{AppConfig, FanPlan};
        let mut cfg = AppConfig::default();
        cfg.fan_auto_curve_enabled = false;
        cfg.fan_mode = Some("max".into());
        for binding in migrate_plans(&cfg) {
            assert_eq!(binding.plan, FanPlan::Max);
        }

        let mut cfg = AppConfig::default();
        cfg.fan_auto_curve_enabled = false;
        cfg.fan_mode = Some("auto".into());
        for binding in migrate_plans(&cfg) {
            assert_eq!(binding.plan, FanPlan::Automatic);
        }
    }

    #[test]
    fn nothing_configured_migrates_to_automatic() {
        use crate::config::{AppConfig, FanPlan};
        let cfg = AppConfig::default();
        let plans = migrate_plans(&cfg);
        assert_eq!(plans.len(), 5);
        for binding in &plans {
            assert_eq!(binding.plan, FanPlan::Automatic);
        }
    }

    #[test]
    fn migration_covers_every_mode_exactly_once() {
        use crate::config::AppConfig;
        let plans = migrate_plans(&AppConfig::default());
        let mut modes: Vec<&str> = plans.iter().map(|b| b.mode.as_str()).collect();
        modes.sort_unstable();
        assert_eq!(modes, vec!["balanced", "eco", "performance", "quiet", "turbo"]);
    }

    #[test]
    fn pwm_plans_are_unavailable_without_pwm() {
        use crate::config::FanPlan;
        // Reachable from a config copied off a machine that does have PWM.
        // Resolving to Automatic beats doing nothing silently.
        assert_eq!(
            plan_for_hardware(FanPlan::Curve { steps: DEFAULT_FAN_CURVE }, false),
            FanPlan::Automatic
        );
        assert_eq!(
            plan_for_hardware(FanPlan::Fixed { percent: 40 }, false),
            FanPlan::Automatic
        );
        // These two work through the EC preset path on any model.
        assert_eq!(plan_for_hardware(FanPlan::Max, false), FanPlan::Max);
        assert_eq!(plan_for_hardware(FanPlan::Automatic, false), FanPlan::Automatic);
    }

    #[test]
    fn every_plan_survives_on_hardware_with_pwm() {
        use crate::config::FanPlan;
        for plan in [
            FanPlan::Automatic,
            FanPlan::Max,
            FanPlan::Fixed { percent: 40 },
            FanPlan::Curve { steps: DEFAULT_FAN_CURVE },
        ] {
            assert_eq!(plan_for_hardware(plan, true), plan);
        }
    }
}
