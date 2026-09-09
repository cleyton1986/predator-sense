//! GRUB menu splash customization - the "GRUB Splash" tab in Tools.
//!
//! Not a PredatorSense/Acer feature at all: the boot logo picker the real
//! Windows app has (`PSSocket::Set_Custom_BIOS_Logo`, see the reverse
//! engineering notes) only ever stages a file for the *Windows* Boot
//! Manager to render during its own early startup - no EC, ACPI, or WMI
//! involved anywhere in it. That mechanism does not exist on this machine at
//! all once Windows is gone, so there is nothing here to port. This is the
//! closest honest equivalent on Linux: GRUB's own, distro-standard
//! `GRUB_BACKGROUND` menu background, wired up with the same care this
//! project gives anything that touches a system file - a one-time backup,
//! narrow edits, and a verified-or-rolled-back apply.
//!
//! Detection and the "is a custom splash currently active" read are both
//! unprivileged (`/etc/default/grub` is world-readable on every distro this
//! was checked against, and finding a command on `$PATH` needs no
//! privilege either); only `apply`/`reset` cross into the privileged helper
//! (`Action::GrubSplashApply`/`GrubSplashReset`), which re-validates
//! everything checked here on its own side of that boundary.

use predator_sense_protocol::base64_lite;
use predator_sense_protocol::grub_splash::{EXTENSIONS, MARKER, MAX_IMAGE_BYTES};
use predator_sense_protocol::helper::Action;
use std::fs;
use std::path::Path;

const GRUB_DEFAULTS: &str = "/etc/default/grub";
const GRUB_CFG_CANDIDATES: [&str; 2] = ["/boot/grub/grub.cfg", "/boot/grub2/grub.cfg"];
const GENERATORS: [&str; 3] = ["update-grub", "grub2-mkconfig", "grub-mkconfig"];
const EFIBOOTMGR: &str = "efibootmgr";

/// EFI loader filenames that unambiguously identify a *different* boot
/// loader than GRUB - never used the other way around, to positively
/// confirm GRUB, because the common secure-boot chain
/// (shim -> grub, the default on Ubuntu/Fedora/Debian/openSUSE) shows up in
/// `efibootmgr` as `shimx64.efi`, not `grubx64.efi`. Shim's whole job is to
/// load GRUB next, but nothing in this listing says so - so a shim entry is
/// treated as inconclusive (no veto), never as a positive match either.
const OTHER_EFI_LOADER_MARKERS: [&str; 6] = [
    "systemd-boot",
    "gummiboot", // systemd-boot's pre-rename
    "refind",
    "bootmgfw.efi", // Windows Boot Manager, no Linux loader in this chain
    "opencore",
    "clover",
];

/// Whether this system has GRUB at all - a config file to point at *and* a
/// generator to regenerate it with, *and* nothing positively identifying a
/// different loader as the one actually in charge of the next boot. All
/// three matter: a generator alone does not say where `grub.cfg` belongs, a
/// `grub.cfg` alone cannot be regenerated after an edit, and either one can
/// be a leftover from a bootloader this machine no longer actually uses
/// (e.g. `grub2-mkconfig` pulled in as some other package's dependency on a
/// `systemd-boot` system, with a stale `grub.cfg` from an old install).
pub fn detected() -> bool {
    let has_config_and_generator = GRUB_CFG_CANDIDATES
        .iter()
        .any(|path| Path::new(path).is_file())
        && GENERATORS.iter().any(|name| command_exists(name));
    has_config_and_generator && !another_loader_is_definitely_active()
}

/// `true` only when the current EFI boot entry's loader file is positively
/// recognized as belonging to something other than GRUB. `false` covers
/// every inconclusive case on purpose - no `efibootmgr` on `$PATH`, a
/// non-UEFI (legacy BIOS) machine where it always fails, a shim-chainloaded
/// entry, or a label this never learned to recognize - because this exists
/// only to catch a clear false positive, not to demand proof GRUB itself is
/// active. Legacy BIOS boot in particular realistically only ever means
/// GRUB (or nothing this app could touch anyway): `systemd-boot` and
/// `rEFInd` both require UEFI.
fn another_loader_is_definitely_active() -> bool {
    let Ok(output) = std::process::Command::new(EFIBOOTMGR).arg("-v").output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(loader_file) = current_efi_loader_file(&text) else {
        return false;
    };
    let lower = loader_file.to_lowercase();
    OTHER_EFI_LOADER_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Parses `efibootmgr -v`'s own text format: finds `BootCurrent: XXXX`, then
/// that same `BootXXXX` entry's line, then the path inside its `File(...)`.
/// Pure and separately tested against real captured output rather than only
/// exercised through the live binary, which is not present in every
/// environment this runs in (CI included).
fn current_efi_loader_file(efibootmgr_output: &str) -> Option<String> {
    let current_id = efibootmgr_output
        .lines()
        .find_map(|line| line.strip_prefix("BootCurrent:"))
        .map(str::trim)?;
    let prefix = format!("Boot{current_id}");
    let entry_line = efibootmgr_output
        .lines()
        .find(|line| line.starts_with(&prefix))?;
    let after_file = entry_line.split_once("File(")?.1;
    let (path, _) = after_file.split_once(')')?;
    Some(path.to_string())
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|dir| dir.join(name).is_file())
}

/// Whether this app's own splash is the one currently wired into
/// `/etc/default/grub` - never true for a `GRUB_BACKGROUND` the user set to
/// something else, since that is exactly the distinction `MARKER` exists to
/// make (see the privileged side's `strip_managed_background_line`).
pub fn active() -> bool {
    let Ok(content) = fs::read_to_string(GRUB_DEFAULTS) else {
        return false;
    };
    active_in(&content)
}

fn active_in(content: &str) -> bool {
    content.lines().any(|line| {
        let unindented = line.trim_start();
        !unindented.starts_with('#')
            && unindented.starts_with("GRUB_BACKGROUND=")
            && line.contains(MARKER)
    })
}

/// `None` for an unsupported extension - the same four GRUB's own loader
/// modules understand, checked here so a bad pick fails instantly in the
/// GUI instead of only after a `pkexec` prompt and a round trip to the
/// helper, which enforces this identical list again on its own.
fn recognized_extension(image_path: &Path) -> Option<&'static str> {
    let extension = image_path.extension()?.to_str()?.to_lowercase();
    EXTENSIONS
        .iter()
        .find(|candidate| **candidate == extension)
        .copied()
}

pub fn apply(image_path: &Path) -> Result<(), String> {
    let extension = recognized_extension(image_path)
        .ok_or_else(|| crate::i18n::t("grub_splash_error_extension").to_string())?;
    let data = fs::read(image_path)
        .map_err(|error| format!("{}: {error}", crate::i18n::t("grub_splash_error_read")))?;
    if data.is_empty() || data.len() > MAX_IMAGE_BYTES {
        return Err(crate::i18n::t("grub_splash_error_size").to_string());
    }
    let payload = base64_lite::encode(&data);
    crate::hardware::helper::execute(Action::GrubSplashApply, &[extension, &payload])
}

pub fn reset() -> Result<(), String> {
    crate::hardware::helper::execute(Action::GrubSplashReset, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_in_recognizes_only_this_apps_own_line() {
        assert!(!active_in("GRUB_TIMEOUT=5\n"));
        assert!(!active_in(
            "GRUB_BACKGROUND=\"/home/user/my-own-wallpaper.png\"\n"
        ));
        assert!(active_in(
            "GRUB_TIMEOUT=5\nGRUB_BACKGROUND=\"/boot/grub/predator-sense-splash.png\"\n"
        ));
    }

    #[test]
    fn active_in_ignores_a_commented_out_line() {
        assert!(!active_in(
            "#GRUB_BACKGROUND=\"/boot/grub/predator-sense-splash.png\"\n"
        ));
    }

    #[test]
    fn recognized_extension_is_case_insensitive_and_exact() {
        assert_eq!(recognized_extension(Path::new("a.PNG")), Some("png"));
        assert_eq!(recognized_extension(Path::new("a.jpeg")), Some("jpeg"));
        assert_eq!(recognized_extension(Path::new("a.bmp")), None);
        assert_eq!(recognized_extension(Path::new("no-extension")), None);
    }

    /// Real `efibootmgr -v` output, captured on a live PH315-54 running
    /// GRUB through shim (secure boot) - the exact shape that must resolve
    /// to "not a different loader" rather than a false-positive veto, since
    /// this is the default secure-boot chain on Ubuntu/Fedora/Debian and
    /// would otherwise disable this feature for most real users. Trimmed of
    /// the `dp`/`data` hex lines, which the parser never looks at.
    const SHIM_CHAINLOAD_SAMPLE: &str = "\
BootCurrent: 0002
Timeout: 0 seconds
BootOrder: 0002,0001,0003,2001,2002,2003
Boot0001* Windows Boot Manager\tHD(1,GPT,79f9,0x800,0x82000)/File(\\EFI\\Microsoft\\Boot\\bootmgfw.efi)RC
Boot0002* ubuntu\tHD(1,GPT,79f9,0x800,0x82000)/File(\\EFI\\ubuntu\\shimx64.efi)
Boot0003* Windows Boot Manager\tHD(1,GPT,79f9,0x800,0x82000)/File(\\EFI\\ubuntu\\grubx64.efi)
Boot2001* EFI USB Device\tRC
";

    #[test]
    fn current_efi_loader_file_follows_boot_current_not_boot_order() {
        // The current entry (Boot0002, shim) is neither first in BootOrder
        // nor the one literally named "grubx64.efi" (Boot0003) - only
        // BootCurrent says which one is real.
        assert_eq!(
            current_efi_loader_file(SHIM_CHAINLOAD_SAMPLE).as_deref(),
            Some("\\EFI\\ubuntu\\shimx64.efi")
        );
    }

    #[test]
    fn current_efi_loader_file_is_none_without_a_recognizable_boot_current() {
        assert_eq!(current_efi_loader_file("Timeout: 0 seconds\n"), None);
    }

    #[test]
    fn a_shim_secure_boot_chain_is_never_treated_as_another_loader() {
        // Shim's whole job is to load GRUB next; nothing in efibootmgr's own
        // listing says so, but it must never read as "systemd-boot" or any
        // other marker either - this is the default shape on most real
        // installs, and a false veto here would silently disable the
        // feature for the common case, not just an edge case.
        let loader = current_efi_loader_file(SHIM_CHAINLOAD_SAMPLE).unwrap();
        let lower = loader.to_lowercase();
        assert!(!OTHER_EFI_LOADER_MARKERS
            .iter()
            .any(|marker| lower.contains(marker)));
    }

    #[test]
    fn a_systemd_boot_entry_is_recognized_as_another_loader() {
        let sample = "BootCurrent: 0001\nBoot0001* Linux Boot Manager\tHD(1,GPT)/File(\\EFI\\systemd\\systemd-bootx64.efi)\n";
        let loader = current_efi_loader_file(sample).unwrap();
        assert!(OTHER_EFI_LOADER_MARKERS
            .iter()
            .any(|marker| loader.to_lowercase().contains(marker)));
    }
}
