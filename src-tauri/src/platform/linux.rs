//! Linux: the FHS system folders, snaps, AppImages, and pseudo file systems.

use super::{IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK, UNIX_DENIED};

pub static PLATFORM: Platform = Platform {
    os: Os::Linux,
    skip_anywhere: &["lost+found"],
    // `/home`, `/mnt` and `/media` are walked, since that's where projects live.
    skip_at_root: &[
        "bin", "boot", "dev", "etc", "lib", "lib32", "lib64", "libx32", "opt", "proc", "root",
        "run", "sbin", "snap", "srv", "sys", "tmp", "usr", "var",
    ],
    skip_at_root_prefixes: &[],
    // Per-user data for snap packages.
    skip_in_home: &["snap"],
    skip_suffixes: &[],
    // An extracted AppImage.
    installed_markers: &["apprun"],
    hidden_mounts: &["/boot", "/snap", "/run", "/proc", "/sys", "/dev", "/var/lib/docker", "/var/snap"],
    hidden_file_systems: &[
        "squashfs",
        "overlay",
        "tmpfs",
        "devtmpfs",
        "efivarfs",
        "proc",
        "sysfs",
        "ramfs",
        "autofs",
        "nsfs",
        "cgroup",
        "cgroup2",
        "debugfs",
        "tracefs",
        "securityfs",
        "pstore",
        "bpf",
        "configfs",
        "mqueue",
        "hugetlbfs",
        "fusectl",
        "fuse.snapfuse",
        "fuse.portal",
    ],
    network_file_systems: &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "fuse.sshfs",
        "fuse.rclone",
        "ceph",
        "glusterfs",
        "9p",
    ],
    trash_name: "Trash",
    file_manager: "your file manager",
    explain_error,
};

fn explain_error(code: i32) -> Option<&'static str> {
    match code {
        // EPERM
        1 => Some("Not permitted. It may be marked immutable, or belong to another user."),
        13 => Some(UNIX_DENIED),      // EACCES
        16 | 26 => Some(IN_USE),      // EBUSY, ETXTBSY
        30 => Some(READ_ONLY_DISK),   // EROFS
        39 => Some(NOT_EMPTY),        // ENOTEMPTY
        _ => None,
    }
}
