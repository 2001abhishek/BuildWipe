//! The few operations that need OS-specific system calls. Everything else
//! about each OS is plain data in the sibling modules.

use std::fs;
use std::path::Path;

/// Removes what stops the owner from deleting a folder: the read-only
/// attribute on Windows, and missing write permission on Unix folders.
/// Never follows symlinks or junctions.
pub fn make_writable(path: &Path) {
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(meta) = fs::symlink_metadata(&current) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        // Unlock before listing: on Unix a folder without permissions can't be read.
        unlock(&current, &meta);
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(&current) {
                pending.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
}

/// Git objects and some npm packages are read-only, which blocks deletion on Windows.
#[cfg(windows)]
fn unlock(path: &Path, meta: &fs::Metadata) {
    let mut permissions = meta.permissions();
    if permissions.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
}

/// Deleting an entry on Unix needs read, write and search permission on the
/// folder holding it. Go and some build tools leave folders without them.
/// A file's own permissions don't affect deleting it.
#[cfg(unix)]
fn unlock(path: &Path, meta: &fs::Metadata) {
    use std::os::unix::fs::PermissionsExt;
    if !meta.is_dir() {
        return;
    }
    let mode = meta.permissions().mode();
    if mode & 0o700 != 0o700 {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o700));
    }
}
