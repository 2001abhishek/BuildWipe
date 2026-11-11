mod platform;
mod rules;
mod scanner;
mod wipe;
mod window;

use platform::{DiskKind, Platform, PlatformInfo, Policy};
use scanner::{ScanEvent, Session};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use sysinfo::Disks;
use tauri::ipc::Channel;
use tauri::{Manager, State};
use wipe::WipeEvent;

#[derive(Default)]
struct AppState {
    session: Mutex<Option<Arc<Session>>>,
    wipe_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    /// Forgets the current scan: stops it if running and frees its allowlist, so
    /// none of its paths can be wiped until they are scanned again.
    fn clear_session(&self) {
        if let Ok(mut current) = self.session.lock() {
            if let Some(session) = current.take() {
                session.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    fn current_session(&self) -> Option<Arc<Session>> {
        self.session.lock().ok().and_then(|current| current.clone())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskInfo {
    mount: String,
    name: String,
    total: u64,
    available: u64,
    removable: bool,
    /// A network share: listed, but not preselected, because walking it is slow.
    network: bool,
}

/// The user's disks, without the OS's own volumes, snap images and pseudo file systems.
fn read_disks() -> Vec<DiskInfo> {
    let platform = Platform::current();
    let mut disks: Vec<DiskInfo> = Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let mount = d.mount_point().to_string_lossy().into_owned();
            let kind = platform.disk_kind(&mount, &d.file_system().to_string_lossy());
            (kind != DiskKind::Hidden).then(|| DiskInfo {
                mount,
                name: d.name().to_string_lossy().into_owned(),
                total: d.total_space(),
                available: d.available_space(),
                removable: d.is_removable(),
                network: kind == DiskKind::Network,
            })
        })
        .collect();
    disks.sort_by(|a, b| a.mount.cmp(&b.mount));
    disks.dedup_by(|a, b| a.mount == b.mount);
    disks
}

#[tauri::command]
fn list_disks() -> Vec<DiskInfo> {
    read_disks()
}

/// Which OS this is, and its names for the trash and file manager.
#[tauri::command]
fn platform_info() -> PlatformInfo {
    Platform::current().info()
}

/// Starts a scan on a background thread. Replaces any earlier session, so only
/// the newest results can be wiped.
#[tauri::command]
fn start_scan(roots: Vec<String>, on_event: Channel<ScanEvent>, state: State<'_, AppState>) {
    let session = Arc::new(Session::default());
    if let Ok(mut current) = state.session.lock() {
        if let Some(previous) = current.replace(session.clone()) {
            previous.cancel.store(true, Ordering::Relaxed);
        }
    }

    std::thread::spawn(move || {
        let mounts: Vec<String> = read_disks().into_iter().map(|d| d.mount).collect();
        scanner::run(&session, roots, &mounts, &Policy::current(), |event| {
            let _ = on_event.send(event);
        });
    });
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) {
    if let Ok(current) = state.session.lock() {
        if let Some(session) = current.as_ref() {
            session.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[tauri::command]
fn clear_scan(state: State<'_, AppState>) {
    state.clear_session();
}

#[tauri::command]
fn start_wipe(
    paths: Vec<String>,
    on_event: Channel<WipeEvent>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session = state.current_session().ok_or("Run a scan before wiping.")?;

    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut current) = state.wipe_cancel.lock() {
        *current = Some(cancel.clone());
    }

    std::thread::spawn(move || {
        wipe::run(&session, paths, &cancel, |event| {
            let _ = on_event.send(event);
        });
    });
    Ok(())
}

#[tauri::command]
fn cancel_wipe(state: State<'_, AppState>) {
    if let Ok(current) = state.wipe_cancel.lock() {
        if let Some(cancel) = current.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // In-app updates from GitHub Releases. See `plugins.updater` in tauri.conf.json.
            #[cfg(desktop)]
            {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_process::init())?;
            }
            if let Some(main) = app.get_webview_window("main") {
                window::show_fitted(&main);
            }
            Ok(())
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_disks,
            platform_info,
            start_scan,
            cancel_scan,
            clear_scan,
            start_wipe,
            cancel_wipe
        ])
        .run(tauri::generate_context!())
        .expect("error while running BuildWipe");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_stops_the_scan_and_leaves_nothing_to_wipe() {
        let state = AppState::default();
        let session = Arc::new(Session::default());
        *state.session.lock().unwrap() = Some(session.clone());

        state.clear_session();

        assert!(session.cancel.load(Ordering::Relaxed));
        assert!(state.current_session().is_none());
    }

    #[test]
    fn clearing_with_no_scan_is_harmless() {
        let state = AppState::default();
        state.clear_session();
        assert!(state.current_session().is_none());
    }
}
