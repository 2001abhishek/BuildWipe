//! Everything that differs between Windows, macOS and Linux.
//!
//! Each OS is described as plain data in its own module (`windows.rs`,
//! `macos.rs`, `linux.rs`). Those modules are compiled on every OS, so the
//! rules for all three are unit-tested on any machine. Only `native.rs`, which
//! makes system calls, is compiled per OS.

mod linux;
mod macos;
pub mod native;
mod windows;

use crate::rules::Listing;
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
}

impl Os {
    pub const CURRENT: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    };
}

/// What one OS needs the scanner, disk list and wipe to know. Folder names are lower-case.
pub struct Platform {
    pub os: Os,
    /// Never entered, wherever they appear.
    pub skip_anywhere: &'static [&'static str],
    /// Never entered directly under a filesystem root, such as `C:\Windows` or `/usr`.
    pub skip_at_root: &'static [&'static str],
    /// Like `skip_at_root`, matched by prefix, such as `$WinREAgent`.
    pub skip_at_root_prefixes: &'static [&'static str],
    /// Never entered directly under the user's home folder, such as `~/Library`.
    pub skip_in_home: &'static [&'static str],
    /// Never entered when the name ends with one of these, such as `Xcode.app`.
    pub skip_suffixes: &'static [&'static str],
    /// A file with one of these names marks a folder as installed software.
    pub installed_markers: &'static [&'static str],
    /// Mount points, and everything under them, that aren't user disks.
    pub hidden_mounts: &'static [&'static str],
    /// File systems that aren't user disks, such as snap images.
    pub hidden_file_systems: &'static [&'static str],
    /// File systems on another machine, which are slow to walk.
    pub network_file_systems: &'static [&'static str],
    pub trash_name: &'static str,
    pub file_manager: &'static str,
    /// A plain-language explanation for an OS error code, when there's a better one than the OS's.
    pub explain_error: fn(i32) -> Option<&'static str>,
}

/// Hidden folders are skipped, except virtualenvs, which are artifacts themselves.
const KEEP_HIDDEN: &[&str] = &[".venv", ".virtualenv"];

/// Load-bearing dependencies and toolchains that exist on every OS.
const SHARED_SKIP_ANYWHERE: &[&str] = &["site-packages", "steamapps", "anaconda3", "miniconda3"];

/// `(folder, its parent)` pairs never entered on any OS: an Electron app's
/// bundled code, and Go's module cache, which fails checksum verification if
/// anything inside it is removed.
const SHARED_SKIP_UNDER: &[(&str, &str)] = &[("app", "resources"), ("mod", "pkg")];

const SHARED_INSTALLED_MARKERS: &[&str] = &["app.asar", "conda-meta"];

/// Messages that read the same on every OS.
pub(crate) const IN_USE: &str = "In use by another program. Close it and try again.";
pub(crate) const NOT_EMPTY: &str = "Something added files while it was being deleted. Try again.";
pub(crate) const READ_ONLY_DISK: &str = "The disk is read-only.";
pub(crate) const UNIX_DENIED: &str =
    "Permission denied. It may belong to another user, which is common after a sudo install. Fix its owner or remove it with sudo.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskKind {
    /// A user disk, preselected for scanning.
    Local,
    /// A network share. Listed, but left for the user to choose.
    Network,
    /// Not a user disk at all, so it isn't listed.
    Hidden,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    os: Os,
    trash_name: &'static str,
    file_manager: &'static str,
}

impl Platform {
    pub fn for_os(os: Os) -> &'static Platform {
        match os {
            Os::Windows => &windows::PLATFORM,
            Os::Macos => &macos::PLATFORM,
            Os::Linux => &linux::PLATFORM,
        }
    }

    pub fn current() -> &'static Platform {
        Self::for_os(Os::CURRENT)
    }

    pub fn info(&self) -> PlatformInfo {
        PlatformInfo {
            os: self.os,
            trash_name: self.trash_name,
            file_manager: self.file_manager,
        }
    }

    pub fn disk_kind(&self, mount: &str, file_system: &str) -> DiskKind {
        let file_system = file_system.to_lowercase();
        let under = |prefix: &&str| {
            mount == *prefix
                || mount.strip_prefix(*prefix).is_some_and(|rest| rest.starts_with('/'))
        };

        if self.hidden_file_systems.contains(&file_system.as_str())
            || self.hidden_mounts.iter().any(under)
        {
            DiskKind::Hidden
        } else if self.network_file_systems.contains(&file_system.as_str()) {
            DiskKind::Network
        } else {
            DiskKind::Local
        }
    }

    pub fn describe(&self, error: &io::Error) -> String {
        error
            .raw_os_error()
            .and_then(self.explain_error)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string())
    }
}

/// The scanner's view of what it must not walk into, for one OS and one user.
pub struct Policy {
    pub platform: &'static Platform,
    home: Option<PathBuf>,
}

impl Policy {
    pub fn new(platform: &'static Platform, home: Option<PathBuf>) -> Self {
        Self { platform, home }
    }

    pub fn current() -> Self {
        Self::new(Platform::current(), home_dir())
    }

    /// True when the walker must not descend into the unmatched folder `name` inside `parent`.
    ///
    /// `at_fs_root` means `parent` is a drive or filesystem root, where names
    /// like `Windows` or `lib` are the OS itself. Deeper down the same names are
    /// normal project folders, such as the `windows` runner in a Flutter app.
    pub fn skip_dir(&self, parent: &Path, name: &str, at_fs_root: bool) -> bool {
        let lower = name.to_lowercase();
        let platform = self.platform;

        if lower.starts_with('.') {
            return !KEEP_HIDDEN.contains(&lower.as_str());
        }
        if SHARED_SKIP_ANYWHERE.contains(&lower.as_str())
            || platform.skip_anywhere.contains(&lower.as_str())
            || platform.skip_suffixes.iter().any(|s| lower.ends_with(s))
        {
            return true;
        }

        let parent_name = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if SHARED_SKIP_UNDER
            .iter()
            .any(|(child, owner)| lower == *child && parent_name == *owner)
        {
            return true;
        }

        if at_fs_root
            && (platform.skip_at_root.contains(&lower.as_str())
                || platform.skip_at_root_prefixes.iter().any(|p| lower.starts_with(p)))
        {
            return true;
        }

        self.home.as_deref() == Some(parent) && platform.skip_in_home.contains(&lower.as_str())
    }

    /// True when a folder holds an installed application or toolchain. Its
    /// dependencies are load-bearing, so nothing inside may be offered for removal.
    pub fn is_installed_software(&self, listing: &Listing) -> bool {
        SHARED_INSTALLED_MARKERS
            .iter()
            .chain(self.platform.installed_markers)
            .any(|marker| listing.has(marker))
    }
}

fn home_dir() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Os; 3] = [Os::Windows, Os::Macos, Os::Linux];

    fn policy(os: Os) -> Policy {
        Policy::new(Platform::for_os(os), Some(PathBuf::from("/home/me")))
    }

    fn skips(os: Os, parent: &str, name: &str, at_fs_root: bool) -> bool {
        policy(os).skip_dir(Path::new(parent), name, at_fs_root)
    }

    #[test]
    fn current_os_is_detected() {
        let expected = if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Linux
        };
        assert_eq!(Platform::current().os, expected);
    }

    #[test]
    fn every_os_skips_hidden_folders_but_not_virtualenvs() {
        for os in ALL {
            assert!(skips(os, "/code", ".cargo", false), "{os:?}");
            assert!(skips(os, "/code", ".git", false), "{os:?}");
            assert!(!skips(os, "/code/app", ".venv", false), "{os:?}");
        }
    }

    #[test]
    fn every_os_protects_shared_toolchains() {
        for os in ALL {
            assert!(skips(os, "/apps/editor/resources", "app", false), "{os:?} Electron app");
            assert!(skips(os, "/home/me/go/pkg", "mod", false), "{os:?} Go module cache");
            assert!(skips(os, "/code/env/lib/python3.12", "site-packages", false), "{os:?}");
            assert!(skips(os, "/games", "steamapps", false), "{os:?}");
        }
    }

    #[test]
    fn every_os_walks_ordinary_project_folders() {
        for os in ALL {
            for name in ["src", "app", "mod", "lib", "windows", "linux", "Library", "bin"] {
                assert!(!skips(os, "/code/project", name, false), "{os:?} skipped {name}");
            }
        }
    }

    #[test]
    fn windows_protects_system_and_program_folders() {
        let os = Os::Windows;
        assert!(skips(os, "C:\\", "Windows", true));
        assert!(skips(os, "C:\\", "$Recycle.Bin", true));
        assert!(skips(os, "C:\\", "$WinREAgent", true));
        assert!(skips(os, "/code", "AppData", false));
        assert!(skips(os, "/code", "Program Files (x86)", false));
        // A Flutter project's `windows` runner is not the OS.
        assert!(!skips(os, "/code/app", "windows", false));
    }

    #[test]
    fn macos_protects_system_library_and_app_bundles() {
        let os = Os::Macos;
        for name in ["System", "Library", "Applications", "Volumes", "private", "opt"] {
            assert!(skips(os, "/", name, true), "/{name}");
        }
        assert!(skips(os, "/home/me", "Library", false), "~/Library");
        assert!(skips(os, "/home/me", "Applications", false), "~/Applications");
        assert!(skips(os, "/code", "Xcode.app", false));
        // Library is only protected in the home folder, not in every project.
        assert!(!skips(os, "/home/me/code/app", "Library", false));
    }

    #[test]
    fn linux_protects_system_folders_and_snaps() {
        let os = Os::Linux;
        for name in ["usr", "etc", "proc", "sys", "snap", "opt", "var", "lib", "root"] {
            assert!(skips(os, "/", name, true), "/{name}");
        }
        assert!(skips(os, "/home/me", "snap", false), "~/snap");
        assert!(skips(os, "/mnt/data", "lost+found", false));
        // The same names deeper down are project folders.
        assert!(!skips(os, "/home/me/code/app", "lib", false));
        assert!(!skips(os, "/home/me/code", "snap", false));
    }

    #[test]
    fn home_rules_need_a_known_home() {
        let without_home = Policy::new(Platform::for_os(Os::Macos), None);
        assert!(!without_home.skip_dir(Path::new("/home/me"), "Library", false));
    }

    #[test]
    fn installed_software_is_recognized_per_os() {
        let listing = |names: &[&str]| Listing::new(names.iter().map(|n| n.to_string()));
        for os in ALL {
            assert!(policy(os).is_installed_software(&listing(&["app.asar"])), "{os:?}");
            assert!(!policy(os).is_installed_software(&listing(&["package.json"])), "{os:?}");
        }
        assert!(policy(Os::Windows).is_installed_software(&listing(&["unins000.exe"])));
        assert!(policy(Os::Windows).is_installed_software(&listing(&["python.exe"])));
        assert!(policy(Os::Linux).is_installed_software(&listing(&["AppRun"])));
    }

    #[test]
    fn linux_lists_only_real_disks() {
        let linux = Platform::for_os(Os::Linux);
        assert_eq!(linux.disk_kind("/", "ext4"), DiskKind::Local);
        assert_eq!(linux.disk_kind("/home", "btrfs"), DiskKind::Local);
        assert_eq!(linux.disk_kind("/media/me/USB", "vfat"), DiskKind::Local);
        assert_eq!(linux.disk_kind("/snap/core22/1380", "squashfs"), DiskKind::Hidden);
        assert_eq!(linux.disk_kind("/boot/efi", "vfat"), DiskKind::Hidden);
        assert_eq!(linux.disk_kind("/run/user/1000", "tmpfs"), DiskKind::Hidden);
        assert_eq!(linux.disk_kind("/mnt/nas", "nfs4"), DiskKind::Network);
        // A prefix only counts at a folder boundary.
        assert_eq!(linux.disk_kind("/bootstrap", "ext4"), DiskKind::Local);
    }

    #[test]
    fn macos_hides_its_system_volumes() {
        let macos = Platform::for_os(Os::Macos);
        assert_eq!(macos.disk_kind("/", "apfs"), DiskKind::Local);
        assert_eq!(macos.disk_kind("/Volumes/Backup", "apfs"), DiskKind::Local);
        assert_eq!(macos.disk_kind("/System/Volumes/Data", "apfs"), DiskKind::Hidden);
        assert_eq!(macos.disk_kind("/System/Volumes/VM", "apfs"), DiskKind::Hidden);
        assert_eq!(macos.disk_kind("/Volumes/share", "smbfs"), DiskKind::Network);
    }

    #[test]
    fn windows_lists_every_drive() {
        let windows = Platform::for_os(Os::Windows);
        assert_eq!(windows.disk_kind("C:\\", "NTFS"), DiskKind::Local);
        assert_eq!(windows.disk_kind("E:\\", "exFAT"), DiskKind::Local);
    }

    #[test]
    fn errors_are_explained_in_each_os_terms() {
        let explain = |os: Os, code: i32| (Platform::for_os(os).explain_error)(code);
        assert!(explain(Os::Windows, 5).unwrap().contains("administrator"));
        assert_eq!(explain(Os::Windows, 32), Some(IN_USE));
        assert_eq!(explain(Os::Linux, 13), Some(UNIX_DENIED));
        assert_eq!(explain(Os::Macos, 13), Some(UNIX_DENIED));
        assert!(explain(Os::Macos, 1).unwrap().contains("Full Disk Access"));
        assert_eq!(explain(Os::Linux, 39), Some(NOT_EMPTY));
        assert_eq!(explain(Os::Macos, 66), Some(NOT_EMPTY));
        for os in ALL {
            assert_eq!(explain(os, 99_999), None, "{os:?}");
        }
    }

    #[test]
    fn unknown_errors_fall_back_to_the_os_message() {
        let error = io::Error::other("disk on fire");
        assert_eq!(Platform::current().describe(&error), "disk on fire");
    }

    #[test]
    fn each_os_names_its_trash_and_file_manager() {
        assert_eq!(Platform::for_os(Os::Windows).trash_name, "Recycle Bin");
        assert_eq!(Platform::for_os(Os::Macos).file_manager, "Finder");
        assert_eq!(Platform::for_os(Os::Linux).trash_name, "Trash");
    }
}
