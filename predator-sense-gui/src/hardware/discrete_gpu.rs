//! MUX-style GPU switch: hybrid/Optimus vs discrete-only.
//!
//! Decoded from the real Windows PredatorSense v3 app
//! (`CommonFunction.cs::Set_System_GPUMode`/`Get_GPUMode_Support`, WMI misc
//! setting index 2), not from mainline `acer-wmi.c` - this control is not
//! part of the upstream kernel driver. `facer.c`'s `discrete_gpu_mode`
//! sysfs attribute wraps the same WMI call the Windows app uses, gated so it
//! only exists on hardware that actually answers it (most Acer laptops this
//! driver binds to have no MUX switch at all).
//!
//! **UNCONFIRMED on real hardware.** Every byte here traces back to
//! decompiled Windows source, not to live traffic or a successful write on
//! real firmware - nobody has tested this attribute against a real MUX
//! switch yet. The official app shows a "restart required" prompt right
//! after toggling it, meaning a successful write is not expected to change
//! anything about the *running* session - the actual GPU routing only takes
//! effect on the next boot, same as when Windows itself changes the
//! setting. Treat a write here the same way: success means the firmware
//! accepted the request, not that anything visibly changed yet.

use predator_sense_protocol::helper::Action as HelperAction;
use std::fs;

const SYSFS_MODE: &str = "/sys/devices/platform/acer-wmi/discrete_gpu_mode";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMode {
    /// Both GPUs available, switchable per-app (the normal, safe default).
    Hybrid,
    /// The iGPU is disabled until switched back. Real hardware consequence.
    DiscreteOnly,
}

impl GpuMode {
    fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Hybrid),
            2 => Some(Self::DiscreteOnly),
            _ => None,
        }
    }

    fn wire_value(self) -> u8 {
        match self {
            Self::Hybrid => 1,
            Self::DiscreteOnly => 2,
        }
    }
}

/// Whether this machine exposes the attribute at all - most do not, since
/// most Acer laptops have no MUX switch. `facer.c` only creates the file
/// after successfully probing the WMI call at module bind, so existence
/// alone is already the capability check; this does not re-read the value.
pub fn is_available() -> bool {
    std::path::Path::new(SYSFS_MODE).exists()
}

/// Current mode, if readable. The attribute is world-readable (only the
/// write needs root, same as `thermal_profile`), so this never goes through
/// the privileged helper.
pub fn get() -> Option<GpuMode> {
    let raw = fs::read_to_string(SYSFS_MODE).ok()?;
    GpuMode::from_wire(raw.trim().parse().ok()?)
}

/// Requests a mode switch. See the module doc: a successful write is the
/// firmware accepting the request, not a confirmation that GPU routing
/// actually changed - that only happens on the next boot.
pub fn set(mode: GpuMode) -> Result<(), String> {
    crate::hardware::helper::execute(
        HelperAction::DiscreteGpuMode,
        &[&mode.wire_value().to_string()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_round_trip() {
        for mode in [GpuMode::Hybrid, GpuMode::DiscreteOnly] {
            assert_eq!(GpuMode::from_wire(mode.wire_value()), Some(mode));
        }
    }

    #[test]
    fn wire_value_matches_the_decompiled_windows_encoding() {
        // CommonFunction.cs: num = 2uL | (ulong)(((!mode) ? 1 : 2) << 8) -
        // mode=false (unchecked, hybrid) -> 1, mode=true (checked, discrete) -> 2.
        assert_eq!(GpuMode::Hybrid.wire_value(), 1);
        assert_eq!(GpuMode::DiscreteOnly.wire_value(), 2);
    }

    #[test]
    fn unknown_wire_values_are_rejected_not_guessed() {
        assert_eq!(GpuMode::from_wire(0), None);
        assert_eq!(GpuMode::from_wire(3), None);
    }
}
