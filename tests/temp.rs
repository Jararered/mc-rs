//! Scratch folders for tests that write files.

use std::fs;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

/// A new, empty folder under Cargo's scratch space for this test target,
/// removed when this is dropped. A test that panics keeps its folder, and
/// says where it is, so what it wrote can be looked at.
///
/// Declare it before whatever writes into it (an app, a server), so that is
/// dropped first: Windows will not delete a file that is still open.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "{label}-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A folder left by a failed run of a process with the same ID.
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!("kept {} from the failed test", self.0.display());
            return;
        }
        // A writer thread may still be closing its last file, which Windows
        // refuses to delete for that moment.
        for _ in 0..50 {
            if fs::remove_dir_all(&self.0).is_ok() || !self.0.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        eprintln!("could not remove {}", self.0.display());
    }
}
