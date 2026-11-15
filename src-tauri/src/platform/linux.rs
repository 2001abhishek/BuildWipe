//! Linux: the FHS system folders, snaps, AppImages, and pseudo file systems.

use super::{CacheDir, IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK, UNIX_DENIED, cache};

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
    caches: CACHES,
    container_cli_dirs: &["/usr/local/bin", "/usr/bin", "~/.rd/bin"],
    // Engines on Linux store their data on the disk itself, so freed space shows up at once.
    container_note: None,
};

static CACHES: &[CacheDir] = &[
    cache("npm cache", ".npm/_cacache"),
    cache("npx packages", ".npm/_npx"),
    cache("Yarn cache", ".cache/yarn"),
    cache("node-gyp headers", ".cache/node-gyp"),
    cache("Electron downloads", ".cache/electron"),
    cache("electron-builder downloads", ".cache/electron-builder"),
    cache("Cargo registry", ".cargo/registry"),
    cache("Cargo git checkouts", ".cargo/git"),
    cache("pip cache", ".cache/pip"),
    cache("Gradle caches", ".gradle/caches"),
    cache("Gradle wrapper downloads", ".gradle/wrapper/dists"),
    cache("Maven repository", ".m2/repository"),
    cache("NuGet packages", ".nuget/packages"),
    cache("NuGet HTTP cache", ".local/share/NuGet/v3-cache"),
    cache("Go build cache", ".cache/go-build"),
    cache("Composer cache", ".cache/composer"),
    cache("Dart and Flutter packages", ".pub-cache/hosted"),
    cache("VS Code extension downloads", ".config/Code/CachedExtensionVSIXs"),
    cache("VS Code code cache", ".config/Code/CachedData"),
];

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
