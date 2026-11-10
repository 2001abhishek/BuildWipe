//! Permanent deletion of items found by the latest scan.
//!
//! Nothing the UI sends is trusted on its own. A path must have been recorded
//! by the scan, and must still classify as an artifact on disk right now.

use crate::rules;
use crate::scanner::{self, Session};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

const WIPE_THREADS: usize = 4;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WipeEvent {
    Removed { path: String, freed: u64 },
    Failed { path: String, error: String, freed: u64, remaining: u64 },
    Finished { cancelled: bool, removed: u64, failed: u64, freed: u64 },
}

fn remove(path: &Path) -> io::Result<()> {
    let is_dir = fs::symlink_metadata(path)?.is_dir();
    let attempt = |p: &Path| if is_dir { fs::remove_dir_all(p) } else { fs::remove_file(p) };

    match attempt(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(windows) => {
            clear_read_only(path);
            attempt(path)
        }
        result => result,
    }
}

/// Git object files and some packages are read-only, which blocks deletion on Windows.
#[cfg(windows)]
fn clear_read_only(path: &Path) {
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(meta) = fs::symlink_metadata(&current) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(&current) {
                pending.extend(entries.flatten().map(|e| e.path()));
            }
        } else if meta.permissions().readonly() {
            let mut permissions = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&current, permissions);
        }
    }
}

#[cfg(not(windows))]
fn clear_read_only(_path: &Path) {}

fn wipe_one(session: &Session, path_text: &str, cancel: &AtomicBool) -> Result<u64, (String, u64, u64)> {
    let path = PathBuf::from(path_text);
    let refuse = |reason: &str| Err((reason.to_string(), 0, 0));

    let is_known = session.found.lock().is_ok_and(|found| found.contains(&path));
    if !is_known {
        return refuse("Not part of the latest scan");
    }
    if rules::classify_path(&path).is_none() {
        return refuse("Changed since the scan, so it was left alone. Scan again to review it.");
    }

    let before = scanner::measure(&path, cancel).map_or(0, |m| m.size);
    match remove(&path) {
        Ok(()) => {
            if let Ok(mut found) = session.found.lock() {
                found.remove(&path);
            }
            Ok(before)
        }
        Err(error) => {
            // A partial delete still freed space, so report what actually remains.
            let never = AtomicBool::new(false);
            let remaining = scanner::measure(&path, &never).map_or(0, |m| m.size);
            Err((friendly(&error), before.saturating_sub(remaining), remaining))
        }
    }
}

fn friendly(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::PermissionDenied => {
            "Access denied. A program may be using it, or it may need administrator rights.".into()
        }
        _ => error.to_string(),
    }
}

/// Deletes `paths`, blocking until done or cancelled.
pub fn run(
    session: &Session,
    paths: Vec<String>,
    cancel: &AtomicBool,
    send: impl Fn(WipeEvent) + Sync,
) {
    let next = AtomicUsize::new(0);
    let totals = Mutex::new((0u64, 0u64, 0u64));

    std::thread::scope(|threads| {
        for _ in 0..WIPE_THREADS {
            threads.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) else {
                    return;
                };
                let outcome = wipe_one(session, path, cancel);
                let mut totals = totals.lock().unwrap_or_else(|e| e.into_inner());
                match outcome {
                    Ok(freed) => {
                        totals.0 += 1;
                        totals.2 += freed;
                        send(WipeEvent::Removed { path: path.clone(), freed });
                    }
                    Err((error, freed, remaining)) => {
                        totals.1 += 1;
                        totals.2 += freed;
                        send(WipeEvent::Failed { path: path.clone(), error, freed, remaining });
                    }
                }
            });
        }
    });

    let (removed, failed, freed) = *totals.lock().unwrap_or_else(|e| e.into_inner());
    send(WipeEvent::Finished {
        cancelled: cancel.load(Ordering::Relaxed),
        removed,
        failed,
        freed,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, relative: &str, bytes: usize) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn scanned(root: &Path) -> Session {
        let session = Session::default();
        scanner::run(&session, vec![root.to_string_lossy().into_owned()], &[], |_| {});
        session
    }

    fn wipe(session: &Session, paths: Vec<String>) -> Vec<WipeEvent> {
        let events = Mutex::new(Vec::new());
        run(session, paths, &AtomicBool::new(false), |e| events.lock().unwrap().push(e));
        events.into_inner().unwrap()
    }

    fn text(path: PathBuf) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn removes_only_the_artifact_and_keeps_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/src/index.js", 5);
        touch(root, "app/node_modules/dep/index.js", 100);
        touch(root, "other/package.json", 5);
        touch(root, "other/node_modules/dep/index.js", 100);

        let session = scanned(root);
        let events = wipe(&session, vec![text(root.join("app/node_modules"))]);

        assert!(!root.join("app/node_modules").exists());
        assert!(root.join("app/package.json").exists());
        assert!(root.join("app/src/index.js").exists());
        assert!(root.join("other/node_modules").exists(), "an unselected item was removed");
        assert!(matches!(
            events.last(),
            Some(WipeEvent::Finished { removed: 1, failed: 0, freed: 100, .. })
        ));
    }

    #[test]
    fn refuses_a_path_the_scan_never_found() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/node_modules/x.js", 5);
        touch(root, "precious/thesis.docx", 5);

        let session = scanned(root);
        let events = wipe(&session, vec![text(root.join("precious"))]);

        assert!(root.join("precious/thesis.docx").exists());
        assert!(matches!(events[0], WipeEvent::Failed { .. }));
    }

    #[test]
    fn refuses_a_real_artifact_once_its_session_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/node_modules/x.js", 5);
        scanned(root);

        // A fresh session is what's left after "New scan": this path has no standing in it.
        let events = wipe(&Session::default(), vec![text(root.join("app/node_modules"))]);

        assert!(root.join("app/node_modules/x.js").exists());
        assert!(matches!(events[0], WipeEvent::Failed { .. }));
    }

    #[test]
    fn refuses_a_found_path_that_no_longer_qualifies() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/Cargo.toml", 5);
        touch(root, "app/target/debug/x", 5);

        let session = scanned(root);
        // The project stopped being a Rust project after the scan.
        fs::remove_file(root.join("app/Cargo.toml")).unwrap();
        let events = wipe(&session, vec![text(root.join("app/target"))]);

        assert!(root.join("app/target/debug/x").exists());
        assert!(matches!(events[0], WipeEvent::Failed { .. }));
    }

    #[test]
    fn removes_read_only_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/node_modules/locked.js", 5);
        let locked = root.join("app/node_modules/locked.js");
        let mut permissions = fs::metadata(&locked).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&locked, permissions).unwrap();

        let session = scanned(root);
        let events = wipe(&session, vec![text(root.join("app/node_modules"))]);

        assert!(!root.join("app/node_modules").exists());
        assert!(matches!(events.last(), Some(WipeEvent::Finished { failed: 0, .. })));
    }

    #[test]
    fn removes_junk_files_and_nested_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "photos/Thumbs.db", 5);
        touch(root, "photos/cat.jpg", 5);
        touch(root, "php/composer.json", 5);
        touch(root, "php/var/cache/x", 5);
        touch(root, "php/var/log/keep.log", 5);

        let session = scanned(root);
        let events = wipe(
            &session,
            vec![text(root.join("photos/Thumbs.db")), text(root.join("php/var/cache"))],
        );

        assert!(!root.join("photos/Thumbs.db").exists());
        assert!(root.join("photos/cat.jpg").exists());
        assert!(!root.join("php/var/cache").exists());
        assert!(root.join("php/var/log/keep.log").exists());
        assert!(matches!(events.last(), Some(WipeEvent::Finished { removed: 2, failed: 0, .. })));
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_inside_an_artifact_does_not_take_its_target_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "outside/keep.txt", 5);
        touch(root, "scan/app/package.json", 5);
        touch(root, "scan/app/node_modules/x.js", 5);
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("scan").join("app").join("node_modules").join("linked"))
            .arg(root.join("outside"))
            .output()
            .unwrap()
            .status;
        assert!(status.success());

        let session = scanned(&root.join("scan"));
        wipe(&session, vec![text(root.join("scan/app/node_modules"))]);

        assert!(!root.join("scan/app/node_modules").exists());
        assert!(root.join("outside/keep.txt").exists(), "deleted through a junction");
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/node_modules/x.js", 5);

        let session = scanned(root);
        let events = Mutex::new(Vec::new());
        run(
            &session,
            vec![text(root.join("app/node_modules"))],
            &AtomicBool::new(true),
            |e| events.lock().unwrap().push(e),
        );

        assert!(root.join("app/node_modules/x.js").exists());
        assert!(matches!(
            events.into_inner().unwrap().last(),
            Some(WipeEvent::Finished { cancelled: true, removed: 0, .. })
        ));
    }
}
