//! macOS: the sealed system volume, Library folders, and `.app` bundles.

use super::{CacheDir, IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK, UNIX_DENIED, cache};

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
    caches: CACHES,
    // Apps opened from Finder don't get the shell's PATH.
    container_cli_dirs: &[
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "~/.orbstack/bin",
        "~/.rd/bin",
        "/opt/podman/bin",
        "/Applications/Docker.app/Contents/Resources/bin",
    ],
    container_note: Some(
        "Docker Desktop and Podman keep these inside a virtual disk. Removing them frees space inside it, and macOS may take a while to get that space back.",
    ),
};

static CACHES: &[CacheDir] = &[
    cache("npm cache", ".npm/_cacache"),
    cache("npx packages", ".npm/_npx"),
    cache("Yarn cache", "Library/Caches/Yarn"),
    cache("node-gyp headers", "Library/Caches/node-gyp"),
    cache("Electron downloads", "Library/Caches/electron"),
    cache("electron-builder downloads", "Library/Caches/electron-builder"),
    cache("Cargo registry", ".cargo/registry"),
    cache("Cargo git checkouts", ".cargo/git"),
    cache("pip cache", "Library/Caches/pip"),
    cache("Gradle caches", ".gradle/caches"),
    cache("Gradle wrapper downloads", ".gradle/wrapper/dists"),
    cache("Maven repository", ".m2/repository"),
    cache("NuGet packages", ".nuget/packages"),
    cache("NuGet HTTP cache", ".local/share/NuGet/v3-cache"),
    cache("Go build cache", "Library/Caches/go-build"),
    cache("Composer cache", "Library/Caches/composer"),
    cache("Dart and Flutter packages", ".pub-cache/hosted"),
    cache("CocoaPods cache", "Library/Caches/CocoaPods"),
    cache("Xcode DerivedData", "Library/Developer/Xcode/DerivedData"),
    cache("Homebrew downloads", "Library/Caches/Homebrew"),
    cache("VS Code extension downloads", "Library/Application Support/Code/CachedExtensionVSIXs"),
    cache("VS Code code cache", "Library/Application Support/Code/CachedData"),
];

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
