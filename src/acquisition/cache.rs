//! Descriptor-relative Darwin cache. Not a hostile same-user filesystem sandbox.
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    fs::{File, OpenOptions},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(1);
const DIR: i32 = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
fn err() -> String {
    std::io::Error::last_os_error().to_string()
}
fn name(s: &str) -> Result<CString> {
    if s.is_empty() || s.contains(['/', '\\', ':']) || s == "." || s == ".." {
        return Err("unsafe leaf".into());
    }
    CString::new(s).map_err(|e| e.to_string())
}
fn open_at(parent: &File, s: &str, flags: i32) -> Result<File> {
    let s = name(s)?;
    // SAFETY: owned live fd, single NUL-terminated component; returned fd uniquely owned.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), s.as_ptr(), flags, 0o600 as libc::c_uint) };
    if fd < 0 {
        Err(err())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn mkdir_at(parent: &File, s: &str) -> Result<File> {
    let n = name(s)?;
    // SAFETY: live parent and single validated leaf, private exclusive directory.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), n.as_ptr(), 0o700) } != 0 {
        return Err(err());
    }
    open_at(parent, s, DIR)
}
fn unlink(parent: &File, s: &str, is_dir: bool) -> Result<()> {
    let n = name(s)?;
    // SAFETY: held directory and single component; never follows symlinks.
    if unsafe {
        libc::unlinkat(
            parent.as_raw_fd(),
            n.as_ptr(),
            if is_dir { libc::AT_REMOVEDIR } else { 0 },
        )
    } != 0
    {
        Err(err())
    } else {
        Ok(())
    }
}
fn same(file: &File, path: &Path) -> Result<()> {
    let a = file.metadata().map_err(|e| e.to_string())?;
    let b = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !b.is_dir() || a.dev() != b.dev() || a.ino() != b.ino() {
        return Err("cache directory rebound".into());
    }
    Ok(())
}
fn write_at(dir: &File, n: &str, bytes: &[u8]) -> Result<()> {
    let mut f = open_at(
        dir,
        n,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
    )?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())
}
pub struct Cache {
    pub(super) path: PathBuf,
    held: File,
}
impl Cache {
    /// Parent must exist, be absolute and contain no symlink aliases. Caller supplies
    /// every selected site, pack directory and application Resources boundary.
    pub fn create_new(parent: &Path, n: &str, protected: &[PathBuf]) -> Result<Self> {
        super::validate_path(n)?;
        if n.contains('/') || n.contains('.') {
            return Err("cache name must be one plain component".into());
        }
        let canonical = parent.canonicalize().map_err(|e| e.to_string())?;
        if parent != canonical || !parent.is_absolute() {
            return Err("cache parent alias refused".into());
        }
        let path = canonical.join(n);
        for p in protected {
            let p = p.canonicalize().map_err(|e| e.to_string())?;
            if path.starts_with(&p) || p.starts_with(&path) {
                return Err("cache overlaps selected input/resources".into());
            }
        }
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(DIR)
            .open(canonical)
            .map_err(|e| e.to_string())?;
        let held = mkdir_at(&parent, n)?;
        parent.sync_all().map_err(|e| e.to_string())?;
        same(&held, &path)?;
        Ok(Self { path, held })
    }
    pub(super) fn protect(&self, roots: &[PathBuf]) -> Result<()> {
        for root in roots {
            let p = root.canonicalize().map_err(|e| e.to_string())?;
            if self.path.starts_with(&p) || p.starts_with(&self.path) {
                return Err("cache overlaps selected site/pack/resources".into());
            }
        }
        Ok(())
    }
    #[cfg(test)]
    fn stage(&self, files: &[(String, Vec<u8>)]) -> Result<Generation> {
        self.stage_checked(files, &|| Ok(()))
    }
    pub(super) fn stage_checked(
        &self,
        files: &[(String, Vec<u8>)],
        check: &impl Fn() -> Result<()>,
    ) -> Result<Generation> {
        check()?;
        let mut names = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let mut total = 0usize;
        for (path, bytes) in files {
            super::validate_path(path)?;
            if !names.insert(path.clone()) || bytes.len() > 262144 {
                return Err("staging size/duplicate".into());
            }
            total = total
                .checked_add(bytes.len())
                .ok_or("staging budget overflow")?;
            let parts: Vec<_> = path.split('/').collect();
            for i in 1..parts.len() {
                dirs.insert(parts[..i].join("/"));
            }
        }
        if files.is_empty()
            || names.len() + dirs.len() > 128
            || total > 2_097_152
            || dirs.iter().any(|d| names.contains(d))
        {
            return Err("staging budget/path collision".into());
        }
        same(&self.held, &self.path)?;
        let n = format!(
            "g-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let held = mkdir_at(&self.held, &n)?;
        let mut g = Generation {
            path: self.path.join(&n),
            name: n,
            parent: self.held.try_clone().map_err(|e| e.to_string())?,
            dirs: BTreeMap::from([(String::new(), held)]),
            files: Vec::new(),
            retained: false,
        };
        for d in dirs {
            check()?;
            let (parent, leaf) = d.rsplit_once('/').unwrap_or(("", &d));
            let held = mkdir_at(&g.dirs[parent], leaf)?;
            g.dirs.insert(d, held);
        }
        for (p, bytes) in files {
            check()?;
            let (parent, leaf) = p.rsplit_once('/').unwrap_or(("", p));
            // Remember owned name before write so a partial write is removed on error.
            g.files.push((parent.into(), leaf.into()));
            write_at(&g.dirs[parent], leaf, bytes)?;
        }
        for dir in g.dirs.values() {
            check()?;
            dir.sync_all().map_err(|e| e.to_string())?;
        }
        check()?;
        self.held.sync_all().map_err(|e| e.to_string())?;
        check()?;
        g.check()?;
        Ok(g)
    }
    /// Rename is the commit point. A later sync failure is a durability warning,
    /// never a false failed acquisition after last-good was already replaced.
    pub(super) fn activate_guarded(
        &self,
        g: &mut Generation,
        source: &str,
        inventory_hash: &str,
        commit: impl FnOnce() -> Result<()>,
    ) -> Result<Option<String>> {
        same(&self.held, &self.path)?;
        g.check()?;
        if !g.path.starts_with(&self.path) {
            return Err("foreign generation".into());
        }
        let temp = format!("active-{}", SERIAL.fetch_add(1, Ordering::Relaxed));
        let text = format!(
            "version = 1\ngeneration = '{}'\nsource = '{}'\ninventory_sha256 = '{}'\n",
            g.name, source, inventory_hash
        );
        if let Err(e) = write_at(&self.held, &temp, text.as_bytes()) {
            let _ = unlink(&self.held, &temp, false);
            return Err(e);
        }
        let from = name(&temp)?;
        let to = name("active.toml")?;
        // Claim only after the pointer file has been written and synced. Cancellation
        // or expiry here removes that temporary pointer and leaves last-good intact.
        if let Err(e) = same(&self.held, &self.path)
            .and_then(|_| g.check())
            .and_then(|_| commit())
        {
            let _ = unlink(&self.held, &temp, false);
            return Err(e);
        }
        // SAFETY: both names are leaves inside the same held private cache root.
        if unsafe {
            libc::renameat(
                self.held.as_raw_fd(),
                from.as_ptr(),
                self.held.as_raw_fd(),
                to.as_ptr(),
            )
        } != 0
        {
            let e = err();
            let _ = unlink(&self.held, &temp, false);
            return Err(e);
        }
        g.retained = true;
        Ok(self
            .held
            .sync_all()
            .err()
            .map(|e| format!("activated; directory sync failed: {e}")))
    }
}
pub(super) struct Generation {
    pub(super) path: PathBuf,
    name: String,
    parent: File,
    dirs: BTreeMap<String, File>,
    files: Vec<(String, String)>,
    retained: bool,
}
impl Generation {
    pub(super) fn check(&self) -> Result<()> {
        for (n, f) in &self.dirs {
            same(f, &self.path.join(n))?;
        }
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        if self.retained {
            return Ok(());
        }
        let mut failures = Vec::new();
        for (p, n) in self.files.drain(..).rev() {
            if let Err(e) = unlink(&self.dirs[&p], &n, false) {
                failures.push(e);
            }
        }
        for d in self.dirs.keys().filter(|p| !p.is_empty()).rev() {
            let (p, n) = d.rsplit_once('/').unwrap_or(("", d));
            if let Err(e) = unlink(&self.dirs[p], n, true) {
                failures.push(e);
            }
        }
        if let Err(e) = unlink(&self.parent, &self.name, true) {
            failures.push(e);
        }
        self.retained = true;
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "owned incomplete cache cleanup: {}",
                failures.join("; ")
            ))
        }
    }
}
impl Drop for Generation {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            eprintln!("{e}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn base(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "https-{name}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        p.canonicalize().unwrap()
    }
    #[test]
    fn cache_refuses_overlap_symlink_and_ambiguous_names_before_writes() {
        let p = base("cache-boundaries");
        assert!(Cache::create_new(&p, "inside", std::slice::from_ref(&p)).is_err());
        for n in ["../escape", "A", "a/b", ".hidden", ""] {
            assert!(Cache::create_new(&p, n, &[]).is_err());
        }
        let alias = p.join("alias");
        std::os::unix::fs::symlink(&p, &alias).unwrap();
        assert!(Cache::create_new(&alias, "cache", &[]).is_err());
        let c = Cache::create_new(&p, "owned", &[]).unwrap();
        assert!(Cache::create_new(&p, "owned", &[]).is_err());
        assert!(c.stage(&[("../escape.md".into(), vec![1])]).is_err());
        assert_eq!(std::fs::read_dir(&c.path).unwrap().count(), 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn activation_guard_runs_after_pointer_write_and_cleans_refused_commit() {
        let p = base("commit-guard");
        let c = Cache::create_new(&p, "owned", &[]).unwrap();
        let mut good = c.stage(&[("index.md".into(), b"old".to_vec())]).unwrap();
        c.activate_guarded(&mut good, "https://fixture.example/", "old", || Ok(()))
            .unwrap();
        let before = std::fs::read(c.path.join("active.toml")).unwrap();
        let mut late = c.stage(&[("index.md".into(), b"late".to_vec())]).unwrap();
        let late_path = late.path.clone();
        let cancel = super::super::Cancellation::default();
        assert!(
            c.activate_guarded(&mut late, "https://fixture.example/", "late", || {
                assert!(
                    std::fs::read_dir(&c.path).unwrap().any(|e| e
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with("active-")),
                    "guard must run after pointer sync"
                );
                cancel.cancel();
                cancel.claim_commit(std::time::Instant::now() + std::time::Duration::from_secs(1))
            })
            .is_err()
        );
        drop(late);
        assert!(!late_path.exists());
        assert_eq!(std::fs::read(c.path.join("active.toml")).unwrap(), before);
        assert_eq!(
            std::fs::read_dir(&c.path).unwrap().count(),
            2,
            "last-good and pointer only"
        );
        let mut expired = c
            .stage(&[("index.md".into(), b"expired".to_vec())])
            .unwrap();
        assert!(
            c.activate_guarded(&mut expired, "https://fixture.example/", "expired", || {
                super::super::Cancellation::default().claim_commit(std::time::Instant::now())
            })
            .is_err()
        );
        drop(expired);
        assert_eq!(std::fs::read(c.path.join("active.toml")).unwrap(), before);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn cancellation_during_staging_cleans_partial_generation() {
        let p = base("stage-cancel");
        let c = Cache::create_new(&p, "owned", &[]).unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(
            c.stage_checked(
                &[
                    ("index.md".into(), b"first".to_vec()),
                    ("second.md".into(), b"second".to_vec())
                ],
                &|| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 3 {
                        Err("cancel during write".into())
                    } else {
                        Ok(())
                    }
                }
            )
            .is_err()
        );
        assert_eq!(calls.get(), 3);
        assert_eq!(std::fs::read_dir(&c.path).unwrap().count(), 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn incomplete_generation_cleans_only_owned_files() {
        let p = base("cleanup");
        let c = Cache::create_new(&p, "owned", &[]).unwrap();
        let g = c
            .stage(&[("sub/index.md".into(), b"page".to_vec())])
            .unwrap();
        let path = g.path.clone();
        assert!(path.join("sub/index.md").exists());
        drop(g);
        assert!(!path.exists(), "incomplete stage must clean");
        std::fs::remove_dir_all(p).unwrap();
    }
}
