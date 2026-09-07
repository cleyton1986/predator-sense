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
        FanPresetStatus::KnownIncompatible => {
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
fn wake_dynamic_fan_curve() {
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
pub fn set_pwm_auto() -> Result<(), String> {
    let automatic = PwmControlMode::Automatic.as_str();
    crate::hardware::helper::execute(HelperAction::PwmCpuEnable, &[automatic])?;
    crate::hardware::helper::execute(HelperAction::PwmGpuEnable, &[automatic])?;
    Ok(())
}

/// Simple CPU-temperature to fan-speed curve (percent) for the software
/// auto-curve toggle on the Fan Control page.
pub fn fan_curve_pct(temp_c: f64) -> u8 {
    match temp_c {
        t if t < 45.0 => 25,
        t if t < 55.0 => 35,
        t if t < 65.0 => 50,
        t if t < 75.0 => 65,
        t if t < 85.0 => 80,
        _ => 100,
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
