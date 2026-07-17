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
    let action = match mode {
        FanMode::Auto => HelperAction::FanAuto,
        FanMode::Max => HelperAction::FanMax,
        FanMode::Custom(_, _) => return Err(crate::i18n::t("fan_note").to_string()),
    };
    crate::hardware::helper::execute(action, &[])
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
