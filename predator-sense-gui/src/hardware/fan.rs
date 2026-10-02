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
}
