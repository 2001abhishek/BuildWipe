//! macOS: the sealed system volume, Library folders, and `.app` bundles.

use super::{IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK, UNIX_DENIED};

pub static PLATFORM: Platform = Platform {
    os: Os::Macos,
    skip_anywhere: &[],
    // `/Volumes` holds other disks, which are listed and scanned as their own roots.
    skip_at_root: &[
        "system",
        "library",
        "applications",
        "private",
        "volumes",
        "cores",
        "bin",
        "sbin",
        "usr",
        "etc",
        "var",
        "tmp",
        "dev",
        "opt",
    ],
    skip_at_root_prefixes: &[],
    skip_in_home: &["library", "applications"],
    // App bundles carry their own node_modules and frameworks.
    skip_suffixes: &[".app", ".framework"],
    installed_markers: &[],
    // The system's own APFS volumes, and Xcode's simulator runtimes.
    hidden_mounts: &["/System/Volumes", "/private/var/vm", "/Library/Developer/CoreSimulator"],
    hidden_file_systems: &["autofs", "devfs", "nullfs"],
    network_file_systems: &["smbfs", "nfs", "afpfs", "webdav"],
    trash_name: "Trash",
    file_manager: "Finder",
    explain_error,
};

fn explain_error(code: i32) -> Option<&'static str> {
    match code {
        // EPERM: usually macOS privacy protection rather than file ownership.
        1 => Some(
            "macOS blocked access. Give BuildWipe Full Disk Access in System Settings › Privacy & Security, then try again.",
        ),
        13 => Some(UNIX_DENIED),  // EACCES
        16 => Some(IN_USE),       // EBUSY
        30 => Some(READ_ONLY_DISK), // EROFS
        66 => Some(NOT_EMPTY),    // ENOTEMPTY
        _ => None,
    }
}
