//! Parallel filesystem walk that streams matched artifacts to the UI.

use crate::rules::{self, Category, Listing};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

const WALK_THREADS: usize = 8;
const BATCH_INTERVAL: Duration = Duration::from_millis(150);
const LOOSE_JUNK_GROUP: &str = "Loose OS junk";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    pub name: String,
    pub category: Category,
    pub project_name: String,
    pub project_path: String,
    pub size: u64,
    pub files: u64,
    /// Newest modification time inside the item, in Unix milliseconds.
    pub modified: u64,
    pub mount: String,
    pub note: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ScanEvent {
    Found { items: Vec<Item> },
    Progress { dirs: u64, current: String },
    Finished { cancelled: bool, dirs: u64, unreadable: u64, elapsed_ms: u64 },
}

/// One scan's results. Only paths recorded here can ever be wiped.
#[derive(Default)]
pub struct Session {
    pub cancel: AtomicBool,
    pub found: Mutex<HashMap<PathBuf, Item>>,
}

#[derive(Default)]
pub struct Measure {
    pub size: u64,
    pub files: u64,
    pub modified: u64,
}

impl Measure {
    fn include(&mut self, meta: &fs::Metadata) {
        if !meta.is_dir() {
            self.size += meta.len();
            self.files += 1;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        self.modified = self.modified.max(modified);
    }
}

/// Totals a file or folder without following symlinks or junctions.
pub fn measure(path: &Path, cancel: &AtomicBool) -> Option<Measure> {
    let mut total = Measure::default();
    let meta = fs::symlink_metadata(path).ok()?;
    total.include(&meta);
    if !meta.is_dir() {
        return Some(total);
    }

    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                total.include(&meta);
            }
            if kind.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Some(total)
}

/// The project an artifact belongs to. A git root wins over nested manifests,
/// so a monorepo's packages group under the repository.
#[derive(Clone, Default)]
struct Project {
    path: Option<Arc<PathBuf>>,
    is_git: bool,
}

impl Project {
    fn enter(&self, dir: &Path, listing: &Listing) -> Self {
        if listing.has(".git") {
            return Self { path: Some(Arc::new(dir.to_path_buf())), is_git: true };
        }
        if !self.is_git && rules::is_project_root(listing) {
            return Self { path: Some(Arc::new(dir.to_path_buf())), is_git: false };
        }
        self.clone()
    }
}

#[derive(Default)]
struct Counters {
    dirs: AtomicU64,
    unreadable: AtomicU64,
    current: Mutex<String>,
}

struct Walk<'a> {
    session: &'a Session,
    counters: &'a Counters,
    mounts: &'a [String],
    tx: Sender<Item>,
}

impl Walk<'_> {
    fn mount_of(&self, path: &Path) -> String {
        self.mounts
            .iter()
            .filter(|m| path.starts_with(m))
            .max_by_key(|m| m.len())
            .cloned()
            .unwrap_or_default()
    }

    fn emit(&self, path: &Path, category: Category, note: Option<&'static str>, project: &Project) {
        let Some(measured) = measure(path, &self.session.cancel) else { return };
        let mount = self.mount_of(path);
        let is_loose_junk = category == Category::OsJunk && project.path.is_none();

        let (project_name, project_path) = if is_loose_junk {
            (LOOSE_JUNK_GROUP.to_string(), mount.clone())
        } else {
            let root = project
                .path
                .as_deref()
                .map(PathBuf::as_path)
                .or_else(|| path.parent())
                .unwrap_or(path);
            (display_name(root), root.to_string_lossy().into_owned())
        };

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some("cache") if path.parent().is_some_and(|p| p.ends_with("var")) => "var/cache".into(),
            other => other.unwrap_or_default().to_string(),
        };

        let _ = self.tx.send(Item {
            path: path.to_string_lossy().into_owned(),
            name,
            category,
            project_name,
            project_path,
            size: measured.size,
            files: measured.files,
            modified: measured.modified,
            mount,
            note,
        });
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn visit<'s>(
    scope: &rayon::Scope<'s>,
    walk: &'s Walk<'s>,
    dir: PathBuf,
    project: Project,
    is_scan_root: bool,
) {
    if walk.session.cancel.load(Ordering::Relaxed) {
        return;
    }
    walk.counters.dirs.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut current) = walk.counters.current.try_lock() {
        current.clear();
        current.push_str(&dir.to_string_lossy());
    }

    let Ok(entries) = fs::read_dir(&dir) else {
        walk.counters.unreadable.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let mut children = Vec::new();
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        names.push(name.clone());
        if let Ok(kind) = entry.file_type() {
            children.push((name, kind));
        }
    }
    let listing = Listing::new(names);

    if !is_scan_root {
        if rules::is_virtualenv(&listing) {
            walk.emit(&dir, Category::Python, None, &project);
            return;
        }
        if rules::is_installed_software(&listing) {
            return;
        }
    }

    let project = project.enter(&dir, &listing);
    let at_fs_root = dir.parent().is_none();

    for (name, kind) in children {
        if kind.is_symlink() {
            continue;
        }
        let path = dir.join(&name);

        if kind.is_file() {
            if rules::is_junk_file(&name) {
                walk.emit(&path, Category::OsJunk, None, &project);
            }
            continue;
        }
        if !kind.is_dir() {
            continue;
        }

        if let Some(found) = rules::match_dir(&name, &listing) {
            let target = match found.nested {
                Some(nested) => path.join(nested),
                None => path,
            };
            if target.is_dir() {
                let project = project.clone();
                scope.spawn(move |_| walk.emit(&target, found.category, found.note, &project));
            }
            continue;
        }

        if !rules::should_skip_dir(&dir, &name, at_fs_root) {
            let project = project.clone();
            scope.spawn(move |s| visit(s, walk, path, project, false));
        }
    }
}

/// Drops roots that sit inside another selected root, so nothing is counted twice.
fn distinct_roots(roots: Vec<String>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
    roots.sort();
    roots.dedup();
    let all = roots.clone();
    roots.retain(|r| !all.iter().any(|other| other != r && r.starts_with(other)));
    roots
}

/// Records incoming items and forwards them to the UI in timed batches.
fn collect(
    session: &Session,
    rx: Receiver<Item>,
    counters: &Counters,
    send: &mut (impl FnMut(ScanEvent) + Send),
) {
    let mut batch = Vec::new();
    let mut last_flush = Instant::now();
    loop {
        let received = rx.recv_timeout(BATCH_INTERVAL);
        let disconnected = matches!(received, Err(RecvTimeoutError::Disconnected));
        if let Ok(item) = received {
            if let Ok(mut found) = session.found.lock() {
                found.insert(PathBuf::from(&item.path), item.clone());
            }
            batch.push(item);
        }
        if disconnected || last_flush.elapsed() >= BATCH_INTERVAL {
            last_flush = Instant::now();
            if !batch.is_empty() {
                send(ScanEvent::Found { items: std::mem::take(&mut batch) });
            }
            send(ScanEvent::Progress {
                dirs: counters.dirs.load(Ordering::Relaxed),
                current: counters.current.lock().map(|c| c.clone()).unwrap_or_default(),
            });
        }
        if disconnected {
            return;
        }
    }
}

/// Walks `roots`, blocking until the walk finishes or is cancelled.
pub fn run(
    session: &Session,
    roots: Vec<String>,
    mounts: &[String],
    mut send: impl FnMut(ScanEvent) + Send,
) {
    let started = Instant::now();
    let counters = Counters::default();
    let (tx, rx) = mpsc::channel();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(WALK_THREADS)
        .build()
        .expect("failed to start scan threads");

    std::thread::scope(|threads| {
        let (counters_ref, send_ref) = (&counters, &mut send);
        let collector = threads.spawn(move || collect(session, rx, counters_ref, send_ref));

        let walk = Walk { session, counters: &counters, mounts, tx };
        let walk_ref = &walk;
        pool.scope(|scope| {
            for root in distinct_roots(roots) {
                if root.is_dir() {
                    scope.spawn(move |s| visit(s, walk_ref, root, Project::default(), true));
                } else {
                    walk_ref.counters.unreadable.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        // Dropping the sender is what tells the collector the walk is over.
        drop(walk);
        let _ = collector.join();
    });

    send(ScanEvent::Finished {
        cancelled: session.cancel.load(Ordering::Relaxed),
        dirs: counters.dirs.load(Ordering::Relaxed),
        unreadable: counters.unreadable.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn touch(root: &Path, relative: &str, bytes: usize) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn scan(root: &Path) -> (BTreeMap<String, Item>, Vec<ScanEvent>) {
        let session = Session::default();
        let mut events = Vec::new();
        run(&session, vec![root.to_string_lossy().into_owned()], &[], |e| events.push(e));
        let found = session.found.into_inner().unwrap();
        let items = found
            .into_iter()
            .map(|(path, item)| {
                let relative = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                (relative, item)
            })
            .collect();
        (items, events)
    }

    #[test]
    fn finds_each_ecosystem_and_reports_size() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "web/package.json", 2);
        touch(root, "web/node_modules/left-pad/index.js", 100);
        touch(root, "web/node_modules/left-pad/package.json", 20);
        touch(root, "game/Cargo.toml", 2);
        touch(root, "game/target/debug/game.exe", 500);
        touch(root, "api/.venv/pyvenv.cfg", 10);
        touch(root, "api/.venv/lib/site.py", 90);
        touch(root, "api/src/__pycache__/a.pyc", 5);
        touch(root, "svc/App.csproj", 2);
        touch(root, "svc/bin/App.dll", 50);
        touch(root, "svc/obj/App.pdb", 50);
        touch(root, "php/composer.json", 2);
        touch(root, "php/var/cache/prod/x.php", 7);
        touch(root, "php/var/log/keep.log", 7);
        touch(root, "photos/Thumbs.db", 3);

        let (items, events) = scan(root);
        let paths: Vec<&str> = items.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            [
                "api/.venv",
                "api/src/__pycache__",
                "game/target",
                "photos/Thumbs.db",
                "php/var/cache",
                "svc/bin",
                "svc/obj",
                "web/node_modules",
            ]
        );
        assert_eq!(items["web/node_modules"].size, 120);
        assert_eq!(items["web/node_modules"].files, 2);
        assert_eq!(items["api/.venv"].category, Category::Python);
        assert_eq!(items["php/var/cache"].name, "var/cache");
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished { cancelled: false, unreadable: 0, .. })
        ));
    }

    #[test]
    fn leaves_lookalikes_and_protected_folders_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "docs/target/report.pdf", 10);
        touch(root, "docs/build/notes.txt", 10);
        touch(root, "docs/bin/tool.exe", 10);
        touch(root, "docs/Library/book.epub", 10);
        touch(root, "code/AppData/Local/app/package.json", 1);
        touch(root, "code/AppData/Local/app/node_modules/x.js", 10);
        touch(root, "installed/editor/resources/app/package.json", 1);
        touch(root, "installed/editor/resources/app/node_modules/x.js", 10);
        touch(root, "installed/tool/unins000.exe", 1);
        touch(root, "installed/tool/package.json", 1);
        touch(root, "installed/tool/node_modules/x.js", 10);
        touch(root, "home/.cargo/registry/src/crate/Cargo.toml", 1);
        touch(root, "home/.cargo/registry/src/crate/target/x", 10);
        touch(root, "Python314/python.exe", 1);
        touch(root, "Python314/Lib/email/__pycache__/a.pyc", 10);

        let (items, _) = scan(root);
        assert!(items.is_empty(), "unexpected matches: {:?}", items.keys());
    }

    #[test]
    fn does_not_report_artifacts_nested_inside_a_match() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "app/package.json", 1);
        touch(root, "app/node_modules/dep/package.json", 1);
        touch(root, "app/node_modules/dep/node_modules/inner/x.js", 10);

        let (items, _) = scan(root);
        assert_eq!(items.keys().collect::<Vec<_>>(), ["app/node_modules"]);
        assert_eq!(items["app/node_modules"].size, 11);
    }

    #[test]
    fn groups_a_monorepo_under_its_git_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("mono/.git")).unwrap();
        touch(root, "mono/packages/a/package.json", 1);
        touch(root, "mono/packages/a/node_modules/x.js", 1);
        touch(root, "solo/package.json", 1);
        touch(root, "solo/node_modules/x.js", 1);

        let (items, _) = scan(root);
        assert_eq!(items["mono/packages/a/node_modules"].project_name, "mono");
        assert_eq!(items["solo/node_modules"].project_name, "solo");
    }

    #[test]
    fn groups_junk_outside_any_project_together() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "a/.DS_Store", 1);
        touch(root, "b/thumbs.db", 1);
        touch(root, "proj/package.json", 1);
        touch(root, "proj/.DS_Store", 1);

        let (items, _) = scan(root);
        assert_eq!(items["a/.DS_Store"].project_name, LOOSE_JUNK_GROUP);
        assert_eq!(items["b/thumbs.db"].project_name, LOOSE_JUNK_GROUP);
        assert_eq!(items["proj/.DS_Store"].project_name, "proj");
    }

    #[test]
    fn nested_roots_are_not_counted_twice() {
        let roots = distinct_roots(vec![
            "D:\\code\\app".into(),
            "D:\\code".into(),
            "D:\\code".into(),
            "E:\\other".into(),
        ]);
        assert_eq!(roots, [PathBuf::from("D:\\code"), PathBuf::from("E:\\other")]);
    }

    #[test]
    fn a_cancelled_scan_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "app/package.json", 1);
        touch(dir.path(), "app/node_modules/x.js", 1);

        let session = Session::default();
        session.cancel.store(true, Ordering::Relaxed);
        let mut events = Vec::new();
        run(&session, vec![dir.path().to_string_lossy().into_owned()], &[], |e| events.push(e));

        assert!(session.found.lock().unwrap().is_empty());
        assert!(matches!(events.last(), Some(ScanEvent::Finished { cancelled: true, .. })));
    }

    #[cfg(windows)]
    #[test]
    fn does_not_follow_junctions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "real/package.json", 1);
        touch(root, "real/node_modules/x.js", 10);
        fs::create_dir_all(root.join("scan")).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("scan").join("link"))
            .arg(root.join("real"))
            .output()
            .unwrap()
            .status;
        assert!(status.success());

        let (items, _) = scan(&root.join("scan"));
        assert!(items.is_empty(), "followed a junction: {:?}", items.keys());
    }
}
