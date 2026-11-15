//! Windows: drive letters, Program Files, AppData, and installer-managed apps.

use super::{CacheDir, Cleanup, IN_USE, NOT_EMPTY, Os, Platform, READ_ONLY_DISK, cache};

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
    caches: CACHES,
    container_cli_dirs: &[
        "C:/Program Files/Docker/Docker/resources/bin",
        "C:/Program Files/RedHat/Podman",
    ],
    container_note: Some(
        "Docker Desktop and Podman keep these inside a virtual disk. Removing them frees space inside it, but your drive may only get that space back once the virtual disk is compacted.",
    ),
};

// The default locations. %LOCALAPPDATA% and %APPDATA% are AppData/Local and AppData/Roaming.
static CACHES: &[CacheDir] = &[
    cache("npm cache", "AppData/Local/npm-cache/_cacache"),
    cache("npx packages", "AppData/Local/npm-cache/_npx"),
    cache("Yarn cache", "AppData/Local/Yarn/Cache"),
    cache("node-gyp headers", "AppData/Local/node-gyp/Cache"),
    cache("Electron downloads", "AppData/Local/electron/Cache"),
    cache("electron-builder downloads", "AppData/Local/electron-builder/Cache"),
    cache("Cargo registry", ".cargo/registry"),
    cache("Cargo git checkouts", ".cargo/git"),
    cache("pip cache", "AppData/Local/pip/Cache"),
    cache("Gradle caches", ".gradle/caches"),
    cache("Gradle wrapper downloads", ".gradle/wrapper/dists"),
    cache("Maven repository", ".m2/repository"),
    cache("NuGet packages", ".nuget/packages"),
    cache("NuGet HTTP cache", "AppData/Local/NuGet/v3-cache"),
    cache("Go build cache", "AppData/Local/go-build"),
    cache("Composer cache", "AppData/Local/Composer"),
    cache("Dart and Flutter packages", "AppData/Local/Pub/Cache/hosted"),
    cache("VS Code extension downloads", "AppData/Roaming/Code/CachedExtensionVSIXs"),
    cache("VS Code code cache", "AppData/Roaming/Code/CachedData"),
    cache("Crash dumps", "AppData/Local/CrashDumps"),
    CacheDir {
        label: "Temporary files older than a week",
        path: "AppData/Local/Temp",
        cleanup: Cleanup::OlderThanDays(7),
    },
];

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
