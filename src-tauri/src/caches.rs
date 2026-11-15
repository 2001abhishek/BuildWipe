//! Developer tool caches: the fixed folders in `Platform::caches`, such as the
//! npm cache or Gradle's downloads. Each is found by its exact location, never
//! by name, and reported as one item. The tool downloads or rebuilds whatever
//! it needs again.

use crate::platform::{CacheDir, Cleanup, Policy};
use crate::scanner::{measure, Measure};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DAY_MS: u64 = 86_400_000;

pub struct Located {
    pub path: PathBuf,
    pub dir: &'static CacheDir,
}

/// The caches that exist for this user. A cache that is itself a link is left
/// out: removing the link would free nothing.
pub fn locate(policy: &Policy) -> Vec<Located> {
    policy
        .cache_dirs()
        .into_iter()
        .filter(|(path, _)| is_real_dir(path))
        .map(|(path, dir)| Located { path, dir })
        .collect()
}

pub fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
}

/// What removing the cache would free, under its cleanup rule.
pub fn measure_cache(path: &Path, cleanup: Cleanup, cancel: &AtomicBool) -> Option<Measure> {
    match cleanup {
        Cleanup::Whole => measure(path, cancel),
        Cleanup::OlderThanDays(days) => {
            let mut total = Measure::default();
            for (_, measured) in old_entries(path, days, cancel)? {
                total.add(&measured);
            }
            Some(total)
        }
    }
}

/// The entries directly inside `dir` with nothing in them modified for `days`
/// days. Links are left alone.
pub fn old_entries(dir: &Path, days: u64, cancel: &AtomicBool) -> Option<Vec<(PathBuf, Measure)>> {
    let cutoff = now_ms().saturating_sub(days * DAY_MS);
    let mut old = Vec::new();
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        let measured = measure(&path, cancel)?;
        // Zero means the time couldn't be read, which is no proof of age.
        if measured.modified > 0 && measured.modified < cutoff {
            old.push((path, measured));
        }
    }
    Some(old)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::platform::{Os, Platform};

    pub fn touch(root: &Path, relative: &str, bytes: usize) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![0u8; bytes]).unwrap();
        path
    }

    pub fn age(file: &Path, days: u64) {
        let when = SystemTime::now() - Duration::from_secs(days * 86_400);
        fs::File::options().write(true).open(file).unwrap().set_modified(when).unwrap();
    }

    #[test]
    fn finds_only_the_caches_that_exist() {
        let home = tempfile::tempdir().unwrap();
        touch(home.path(), ".npm/_cacache/index-v5/x", 10);
        touch(home.path(), ".npm/_logs/debug.log", 10);
        touch(home.path(), ".cargo/bin/cargo", 10);

        let policy = Policy::new(Platform::for_os(Os::Linux), Some(home.path().to_path_buf()));
        let found: Vec<_> = locate(&policy).into_iter().map(|c| c.dir.label).collect();

        assert_eq!(found, ["npm cache"]);
    }

    #[test]
    fn nothing_is_found_without_a_home() {
        assert!(locate(&Policy::new(Platform::for_os(Os::Linux), None)).is_empty());
    }

    #[test]
    fn age_based_cleanup_counts_only_old_entries() {
        let temp = tempfile::tempdir().unwrap();
        let old = touch(temp.path(), "old.tmp", 100);
        age(&old, 30);
        touch(temp.path(), "new.tmp", 7);
        // A folder counts as new when anything in it, the folder included, is new.
        let inside = touch(temp.path(), "busy/old-inside.tmp", 50);
        age(&inside, 30);

        let never = AtomicBool::new(false);
        let entries = old_entries(temp.path(), 7, &never).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, old);

        let measured = measure_cache(temp.path(), Cleanup::OlderThanDays(7), &never).unwrap();
        assert_eq!(measured.size, 100);
        let whole = measure_cache(temp.path(), Cleanup::Whole, &never).unwrap();
        assert_eq!(whole.size, 157);
    }
}
