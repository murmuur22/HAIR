use super::*;
#[cfg(test)]
thread_local! { static BODY_READS: std::cell::RefCell<Vec<(u64, u64, usize)>> = const { std::cell::RefCell::new(Vec::new()) }; }
#[cfg(test)]
thread_local! { static CANCEL_AFTER_READ: std::cell::RefCell<Option<std::sync::Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn cancel_after_body_read(cancelled: std::sync::Arc<AtomicBool>) {
    CANCEL_AFTER_READ.with(|c| *c.borrow_mut() = Some(cancelled));
}
#[cfg(test)]
pub(super) fn record_body_read(file: &File, bytes: usize) {
    let m = file.metadata().unwrap();
    BODY_READS.with(|r| r.borrow_mut().push((m.dev(), m.ino(), bytes)));
    CANCEL_AFTER_READ.with(|c| {
        if let Some(cancelled) = c.borrow_mut().take() { cancelled.store(true, Ordering::Relaxed); }
    });
}
#[cfg(test)]
pub(super) fn take_body_reads() -> Vec<(u64, u64, usize)> { BODY_READS.with(|r| std::mem::take(&mut *r.borrow_mut())) }
#[cfg(test)]
thread_local! { static CANCEL_AFTER_METADATA: std::cell::RefCell<Option<std::sync::Arc<AtomicBool>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(super) fn cancel_after_metadata_check(cancelled: std::sync::Arc<AtomicBool>) {
    CANCEL_AFTER_METADATA.with(|c| *c.borrow_mut() = Some(cancelled));
}
#[cfg(test)]
pub(super) fn record_metadata_check() {
    CANCEL_AFTER_METADATA.with(|c| {
        if let Some(cancelled) = c.borrow_mut().take() { cancelled.store(true, Ordering::Relaxed); }
    });
}
const DIR: i32 = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
const READ: i32 = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
fn component(name: &OsStr) -> Result<CString> {
    valid_name(name)?;
    CString::new(name.as_bytes()).map_err(|_| "NUL in collection component".into())
}
pub(super) fn valid_name(name: &OsStr) -> Result<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 255 || bytes.contains(&0) || bytes.contains(&b'/') || bytes == b"." || bytes == b".." {
        return Err("Invalid collection filename component; paths are not accepted".into());
    }
    Ok(())
}
fn open_at(parent: &File, name: &OsStr, flags: i32) -> Result<File> {
    let name = component(name)?;
    // SAFETY: live directory and validated NUL-terminated component. New FD has one owner.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags, 0o600) };
    if fd < 0 { return Err(format!("Collection open: {}", std::io::Error::last_os_error())); }
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(super) fn acquire_root(path: &Path) -> Result<(File, OsString, File)> {
    // Reject '..' rather than normalizing authority; never follow any component.
    if path.as_os_str().as_bytes().split(|b| *b == b'/').any(|s| s == b"..") {
        return Err("Collection root cannot contain parent traversal".into());
    }
    let mut parts = path.components().peekable();
    let absolute = path.is_absolute();
    let mut parent = OpenOptions::new().read(true).custom_flags(DIR).open(if absolute { "/" } else { "." }).map_err(|e| e.to_string())?;
    while let Some(part) = parts.next() {
        match part {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => {
                let child = open_at(&parent, name, DIR)?;
                verify_named(&parent, name, &child)?;
                if parts.peek().is_none() {
                    owned(&child, true)?;
                    return Ok((parent, name.to_os_string(), child));
                }
                parent = child;
            }
            _ => return Err("Invalid collection root component".into()),
        }
    }
    Err("Collection requires an explicit owned child directory, not / or .".into())
}
pub(super) fn owned(file: &File, directory: bool) -> Result<Metadata> {
    let meta = file.metadata().map_err(|e| e.to_string())?;
    // No shared writable storage or hard-linked payload/manifest/lock authority.
    if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
        || if directory { !meta.is_dir() } else { !meta.is_file() || meta.nlink() != 1 } {
        return Err("Collection storage must be owned/private (directory 0700, single-link file 0600)".into());
    }
    Ok(meta)
}
pub(super) fn regular(parent: &File, name: &str) -> Result<File> {
    let file = open_at(parent, OsStr::new(name), READ)?;
    owned(&file, false)?;
    verify_named(parent, OsStr::new(name), &file)?;
    Ok(file)
}
pub(super) const FINDER: &str = ".DS_Store";
// Exact inert-name exception for Finder's usual 0644 mode. No body reads.
// Owned, single-link regular files only, at most 1 MiB logical length.
pub(super) fn finder_identity(root: &File, names: &[OsString]) -> Result<Option<Stamp>> {
    if !names.iter().any(|n| n == FINDER) { return Ok(None); }
    let file = open_at(root, OsStr::new(FINDER), READ)?;
    let m = file.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.nlink() != 1
        || !matches!(m.mode() & 0o7777, 0o600 | 0o644) || m.len() > 1024 * 1024 {
        return Err("Unsafe .DS_Store metadata; preserve and inspect root".into());
    }
    verify_named(root, OsStr::new(FINDER), &file)?;
    Ok(Some(metadata_stamp(&m)))
}
pub(super) fn create(parent: &File, name: &str) -> Result<File> {
    let file = open_at(parent, OsStr::new(name), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC)?;
    owned(&file, false)?;
    verify_named(parent, OsStr::new(name), &file)?;
    Ok(file)
}
pub(super) fn verify_named(parent: &File, name: &OsStr, held: &File) -> Result<()> {
    let name = component(name)?;
    let mut raw = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: initialized on success, no-follow stat of one component relative to live FD.
    if unsafe { libc::fstatat(parent.as_raw_fd(), name.as_ptr(), raw.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) } != 0 {
        return Err(format!("Collection namespace check: {}", std::io::Error::last_os_error()));
    }
    let raw = unsafe { raw.assume_init() };
    let meta = held.metadata().map_err(|e| e.to_string())?;
    if raw.st_dev as u64 != meta.dev() || raw.st_ino != meta.ino() || raw.st_mode as u32 & libc::S_IFMT as u32 != meta.mode() & libc::S_IFMT as u32 {
        return Err("Collection storage identity/name rebound; reopen explicitly".into());
    }
    Ok(())
}
pub(super) fn rename(root: &File, from: &str, to: &str, exclusive: bool) -> Result<()> {
    let from = component(OsStr::new(from))?;
    let to = component(OsStr::new(to))?;
    // SAFETY: two validated relative names in the held directory. EXCL prevents payload overwrite.
    let result = unsafe { libc::renameatx_np(root.as_raw_fd(), from.as_ptr(), root.as_raw_fd(), to.as_ptr(), if exclusive { libc::RENAME_EXCL } else { 0 }) };
    if result != 0 { Err(format!("Collection rename: {}", std::io::Error::last_os_error())) } else { Ok(()) }
}
pub(super) fn unlink(root: &File, name: &str) -> Result<()> {
    let name = component(OsStr::new(name))?;
    // SAFETY: one verified generated name relative to held owned root; never recursive.
    if unsafe { libc::unlinkat(root.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        Err(format!("Collection payload removal: {}", std::io::Error::last_os_error()))
    } else { Ok(()) }
}
#[derive(PartialEq, Eq)]
pub(super) struct Stamp { pub len: u64, pub dev: u64, pub ino: u64, pub mtime: (i64,i64), pub ctime: (i64,i64) }
pub(super) fn stamp(file: &File) -> Result<Stamp> {
    let m = owned(file, false)?;
    Ok(metadata_stamp(&m))
}
fn metadata_stamp(m: &Metadata) -> Stamp {
    Stamp { len: m.len(), dev: m.dev(), ino: m.ino(), mtime: (m.mtime(), m.mtime_nsec()), ctime: (m.ctime(), m.ctime_nsec()) }
}
pub(super) fn verify_payload_cancelled(file: &File, item: &Item, cancelled: &AtomicBool) -> Result<()> {
    use std::os::unix::fs::FileExt;
    check_cancel(cancelled)?;
    let before = stamp(file)?;
    if before.len != item.bytes { return Err("Collection payload length corrupt/incomplete".into()); }
    let mut offset = 0;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    while offset < item.bytes {
        check_cancel(cancelled)?;
        let wanted = (item.bytes - offset).min(buf.len() as u64) as usize;
        // Finite attempts including interruption/short reads, same policy as ReaderFile.
        let mut used = 0;
        for _ in 0..8 {
            check_cancel(cancelled)?;
            if used == wanted { break; }
            match file.read_at(&mut buf[used..wanted], offset + used as u64) {
                Ok(0) => break,
                Ok(n) => {
                    #[cfg(test)]
                    record_body_read(file, n);
                    used += n;
                },
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {},
                Err(e) => return Err(format!("Collection payload read failed: {e}")),
            }
        }
        if used != wanted { return Err("Collection payload partial read refused".into()); }
        hash.update(&buf[..used]);
        offset += used as u64;
    }
    check_cancel(cancelled)?;
    if <[u8;32]>::from(hash.finalize()) != item.sha256 || stamp(file)? != before {
        return Err("Collection payload digest corrupt or changed".into());
    }
    Ok(())
}
