//! Permanent deletion of items found by the latest scan.
//!
//! Nothing the UI sends is trusted on its own. A path must have been recorded
//! by the scan, and must still be what the scan said it was right now: an
//! artifact that classifies as one on disk, or a folder at exactly one of the
//! tool cache locations. Container items are only ids. What to remove for each
//! was recorded by the scan, and the engine itself refuses anything in use.

use crate::caches;
use crate::containers;
use crate::platform::{native, Cleanup, Platform, Policy};
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
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            native::make_writable(path);
            attempt(path)
        }
        result => result,
    }
}

const NOT_SCANNED: &str = "Not part of the latest scan";
const CHANGED: &str = "Changed since the scan, so it was left alone. Scan again to review it.";

fn forget(session: &Session, path_text: &str) {
    if let Ok(mut found) = session.found.lock() {
        found.remove(Path::new(path_text));
    }
    if let Ok(mut known) = session.containers.lock() {
        known.remove(path_text);
    }
}

fn wipe_one(
    session: &Session,
    policy: &Policy,
    path_text: &str,
    cancel: &AtomicBool,
) -> Result<u64, (String, u64, u64)> {
    let path = PathBuf::from(path_text);
    let refuse = |reason: &str| Err((reason.to_string(), 0, 0));

    if path_text.starts_with(containers::ID_PREFIX) {
        let target = session.containers.lock().ok().and_then(|known| known.get(path_text).cloned());
        let Some(target) = target else { return refuse(NOT_SCANNED) };
        return match containers::remove(&target) {
            Ok(freed) => {
                forget(session, path_text);
                Ok(freed)
            }
            Err(reason) => Err((reason, 0, target.size)),
        };
    }

    let is_known = session.found.lock().is_ok_and(|found| found.contains(&path));
    if !is_known {
        return refuse(NOT_SCANNED);
    }
    match policy.cache_at(&path) {
        Some(_) if !caches::is_real_dir(&path) => return refuse(CHANGED),
        Some(dir) => {
            if let Cleanup::OlderThanDays(days) = dir.cleanup {
                let outcome = remove_old_entries(&path, days, cancel);
                if outcome.is_ok() {
                    forget(session, path_text);
                }
                return outcome;
            }
        }
        None if rules::classify_path(&path).is_none() => return refuse(CHANGED),
        None => {}
    }

    let before = scanner::measure(&path, cancel).map_or(0, |m| m.size);
    match remove(&path) {
        Ok(()) => {
            forget(session, path_text);
            Ok(before)
        }
        Err(error) => {
            // A partial delete still freed space, so report what actually remains.
            let never = AtomicBool::new(false);
            let remaining = scanner::measure(&path, &never).map_or(0, |m| m.size);
            let reason = Platform::current().describe(&error);
            Err((reason, before.saturating_sub(remaining), remaining))
        }
    }
}

/// Removes what has been untouched for `days` days from a folder like Temp.
/// Entries a program still holds open are skipped, which is expected there,
/// so this only fails when nothing at all could be removed.
fn remove_old_entries(dir: &Path, days: u64, cancel: &AtomicBool) -> Result<u64, (String, u64, u64)> {
    let entries = caches::old_entries(dir, days, cancel).unwrap_or_default();
    let mut freed = 0;
    let mut first_error = None;
    let mut removed_any = false;
    for (entry, measured) in &entries {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match remove(entry) {
            Ok(()) => {
                removed_any = true;
                freed += measured.size;
            }
            Err(error) => {
                // A partly removed folder still freed some space.
                let never = AtomicBool::new(false);
                let left = scanner::measure(entry, &never).map_or(0, |m| m.size);
                freed += measured.size.saturating_sub(left);
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) if !removed_any && freed == 0 => {
            let remaining = entries.iter().map(|(_, m)| m.size).sum();
            Err((Platform::current().describe(&error), 0, remaining))
        }
        _ => Ok(freed),
    }
}

/// Deletes `paths`, blocking until done or cancelled.
pub fn run(
    session: &Session,
    policy: &Policy,
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
                let outcome = wipe_one(session, policy, path, cancel);
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
    use crate::platform::Os;

    fn touch(root: &Path, relative: &str, bytes: usize) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn no_home() -> Policy {
        Policy::new(Platform::current(), None)
    }

    fn scanned(root: &Path) -> Session {
        let session = Session::default();
        let roots = vec![root.to_string_lossy().into_owned()];
        let request = scanner::Request { roots, caches: false, containers: false };
        scanner::run(&session, request, &[], &no_home(), |_| {});
        session
    }

    fn wipe(session: &Session, paths: Vec<String>) -> Vec<WipeEvent> {
        wipe_as(session, &no_home(), paths)
    }

    fn wipe_as(session: &Session, policy: &Policy, paths: Vec<String>) -> Vec<WipeEvent> {
        let events = Mutex::new(Vec::new());
        run(session, policy, paths, &AtomicBool::new(false), |e| events.lock().unwrap().push(e));
        events.into_inner().unwrap()
    }

    /// A scan of only the tool caches under a throwaway home folder.
    fn scanned_caches(policy: &Policy) -> Session {
        let session = Session::default();
        let request = scanner::Request { roots: Vec::new(), caches: true, containers: false };
        scanner::run(&session, request, &[], policy, |_| {});
        session
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

    #[test]
    fn a_link_inside_an_artifact_does_not_take_its_target_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "outside/keep.txt", 5);
        touch(root, "scan/app/package.json", 5);
        touch(root, "scan/app/node_modules/x.js", 5);
        // A junction on Windows, a symlink on macOS and Linux.
        scanner::tests::link_dir(
            &root.join("outside"),
            &root.join("scan").join("app").join("node_modules").join("linked"),
        );

        let session = scanned(&root.join("scan"));
        let events = wipe(&session, vec![text(root.join("scan/app/node_modules"))]);

        assert!(!root.join("scan/app/node_modules").exists());
        assert!(root.join("outside/keep.txt").exists(), "deleted through a link");
        assert!(matches!(events.last(), Some(WipeEvent::Finished { removed: 1, failed: 0, .. })));
    }

    /// Go and some build tools leave folders without write permission, which
    /// blocks deleting anything inside them on macOS and Linux.
    #[cfg(unix)]
    #[test]
    fn removes_folders_without_write_permission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 5);
        touch(root, "app/node_modules/pkg/sub/x.js", 5);
        let locked = root.join("app/node_modules/pkg");
        fs::set_permissions(locked.join("sub"), fs::Permissions::from_mode(0o000)).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

        let session = scanned(root);
        let events = wipe(&session, vec![text(root.join("app/node_modules"))]);

        assert!(!root.join("app/node_modules").exists(), "{events:?}");
        assert!(root.join("app/package.json").exists());
        assert!(matches!(events.last(), Some(WipeEvent::Finished { removed: 1, failed: 0, .. })));
    }

    #[test]
    fn a_failure_is_explained_in_the_current_os_terms() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        let code = if cfg!(windows) { 5 } else { 13 };
        let explained = Platform::current().describe(&io::Error::from_raw_os_error(code));
        assert_ne!(explained, denied.to_string());
        assert!(explained.len() > 20, "{explained}");
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
            &no_home(),
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

    #[test]
    fn removes_a_whole_tool_cache_and_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        touch(home, ".npm/_cacache/index-v5/a", 100);
        touch(home, ".npm/_logs/keep.log", 5);
        touch(home, ".npm/_npx/abc/package.json", 5);
        let policy = Policy::new(Platform::for_os(Os::Linux), Some(home.to_path_buf()));

        let session = scanned_caches(&policy);
        let events = wipe_as(&session, &policy, vec![text(home.join(".npm").join("_cacache"))]);

        assert!(!home.join(".npm/_cacache").exists());
        assert!(home.join(".npm/_logs/keep.log").exists());
        assert!(home.join(".npm/_npx/abc/package.json").exists(), "an unselected cache was removed");
        assert!(matches!(events.last(), Some(WipeEvent::Finished { removed: 1, failed: 0, freed: 100, .. })));
    }

    #[test]
    fn refuses_a_cache_folder_the_scan_never_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let policy = Policy::new(Platform::for_os(Os::Linux), Some(home.to_path_buf()));
        let session = scanned_caches(&policy);
        // Created after the scan, so the session has no record of it.
        touch(home, ".npm/_cacache/index-v5/a", 100);

        let events = wipe_as(&session, &policy, vec![text(home.join(".npm").join("_cacache"))]);

        assert!(home.join(".npm/_cacache/index-v5/a").exists());
        assert!(matches!(events[0], WipeEvent::Failed { .. }));
    }

    #[test]
    fn a_cache_replaced_by_a_link_after_the_scan_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        touch(&home, ".npm/_cacache/index-v5/a", 100);
        touch(dir.path(), "precious/keep.txt", 5);
        let policy = Policy::new(Platform::for_os(Os::Linux), Some(home.clone()));
        let session = scanned_caches(&policy);

        let cache = home.join(".npm").join("_cacache");
        fs::remove_dir_all(&cache).unwrap();
        scanner::tests::link_dir(&dir.path().join("precious"), &cache);
        let events = wipe_as(&session, &policy, vec![text(cache)]);

        assert!(dir.path().join("precious/keep.txt").exists());
        assert!(matches!(events[0], WipeEvent::Failed { .. }));
    }

    #[test]
    fn temp_cleanup_removes_only_what_is_old() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let old = caches::tests::touch(home, "AppData/Local/Temp/old-installer.tmp", 100);
        caches::tests::age(&old, 30);
        let fresh = caches::tests::touch(home, "AppData/Local/Temp/in-use-now.tmp", 7);
        let policy = Policy::new(Platform::for_os(Os::Windows), Some(home.to_path_buf()));
        let temp = home.join("AppData").join("Local").join("Temp");

        let session = scanned_caches(&policy);
        let events = wipe_as(&session, &policy, vec![text(temp.clone())]);

        assert!(!old.exists());
        assert!(fresh.exists(), "a recent temporary file was removed");
        assert!(temp.is_dir(), "the Temp folder itself was removed");
        assert!(matches!(events.last(), Some(WipeEvent::Finished { removed: 1, failed: 0, freed: 100, .. })));
    }

    #[test]
    fn refuses_a_container_id_the_scan_never_found() {
        let events = wipe(&Session::default(), vec!["container://docker/image/sha256:abc".to_string()]);
        assert!(matches!(&events[0], WipeEvent::Failed { error, .. } if error == NOT_SCANNED));
    }
}
