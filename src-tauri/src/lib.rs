mod rules;
mod scanner;
mod wipe;

use scanner::{ScanEvent, Session};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use sysinfo::Disks;
use tauri::ipc::Channel;
use tauri::State;
use wipe::WipeEvent;

#[derive(Default)]
struct AppState {
    session: Mutex<Option<Arc<Session>>>,
    wipe_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskInfo {
    mount: String,
    name: String,
    total: u64,
    available: u64,
    removable: bool,
}

fn read_disks() -> Vec<DiskInfo> {
    let mut disks: Vec<DiskInfo> = Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DiskInfo {
            mount: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().into_owned(),
            total: d.total_space(),
            available: d.available_space(),
            removable: d.is_removable(),
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
        scanner::run(&session, roots, &mounts, |event| {
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
fn start_wipe(
    paths: Vec<String>,
    on_event: Channel<WipeEvent>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .ok()
        .and_then(|current| current.clone())
        .ok_or("Run a scan before wiping.")?;

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
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_disks,
            start_scan,
            cancel_scan,
            start_wipe,
            cancel_wipe
        ])
        .run(tauri::generate_context!())
        .expect("error while running BuildWipe");
}
