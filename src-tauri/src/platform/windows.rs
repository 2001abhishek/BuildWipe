//! Windows: drive letters, Program Files, AppData, and installer-managed apps.

use super::{IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK};

pub static PLATFORM: Platform = Platform {
    os: Os::Windows,
    skip_anywhere: &[
        "$recycle.bin",
        "system volume information",
        "program files",
        "program files (x86)",
        "programdata",
        "appdata",
        "windowsapps",
    ],
    skip_at_root: &["windows", "recovery", "perflogs", "msocache", "config.msi", "boot"],
    // $Recycle.Bin, $WinREAgent, $Windows.~BT and other system folders at a drive root.
    skip_at_root_prefixes: &["$"],
    skip_in_home: &[],
    skip_suffixes: &[],
    // Inno Setup and NSIS uninstallers, and standalone Python installs.
    installed_markers: &["unins000.exe", "uninstall.exe", "python.exe"],
    hidden_mounts: &[],
    hidden_file_systems: &[],
    network_file_systems: &[],
    trash_name: "Recycle Bin",
    file_manager: "File Explorer",
    explain_error,
};

fn explain_error(code: i32) -> Option<&'static str> {
    match code {
        // ERROR_ACCESS_DENIED
        5 => Some("Access denied. A program may be using it, or it may need administrator rights."),
        // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
        32 | 33 => Some(IN_USE),
        // ERROR_WRITE_PROTECT
        19 => Some(READ_ONLY_DISK),
        // ERROR_DIR_NOT_EMPTY
        145 => Some(NOT_EMPTY),
        _ => None,
    }
}
