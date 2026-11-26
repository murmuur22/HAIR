//! Read-only, handle-relative local-folder snapshots and explicit navigation.
use crate::Result;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// Enumeration safety budget, not a limit on eventual visible object slots or paging.
pub const MAX_ENTRIES: usize = 16_384;
/// Back retains the most recent 64 departed directories; older handles are closed.
pub const MAX_HISTORY: usize = 64;
/// Absolute per-inspection encoded-body ceiling; text callers use a lower bound.
pub const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
    Symlink,
    Special,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::File => "file",
            Self::Symlink => "symlink",
            Self::Special => "special",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Original OS name, never reconstructed from an escaped display label.
    pub name: OsString,
    /// Human-facing immediate child path, not an authority for opening this entry.
    pub path: PathBuf,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Actual directory path obtained from its open handle, not a confinement root.
    /// Like any display path, this can become stale after an external rename.
    pub folder: PathBuf,
    pub entries: Vec<Entry>,
}

/// Quoted, control-escaped display only. Keep OS identities for all path operations.
/// OsStr's debug formatting also escapes non-UTF-8 bytes rather than losing identity.
pub fn escape_display(value: &OsStr) -> String {
    format!("{value:?}")
}

/// Enumerate all immediate children or fail, including on overflow or entry errors.
/// The explicitly chosen initial path may resolve aliases. Entries are classified
/// relative to the resulting directory handle without following their links.
/// No file body is opened and no child directory is recursively enumerated.
pub fn snapshot(selected: &Path) -> Result<Snapshot> {
    backend::snapshot(selected)
}

pub use backend::{Destination, Explorer, ReaderFile};

/// Independent random-access budget; the whole-body API and its ceiling are unchanged.
pub const READER_CHUNK_BYTES: usize = 4096;
#[derive(Debug)]
pub struct ReaderChunk {
    pub bytes: [u8; READER_CHUNK_BYTES],
    pub used: usize,
    pub start: u64,
    pub end: u64,
    pub observed_len: u64,
    pub eof: bool,
}
impl ReaderChunk {
    pub fn body(&self) -> &[u8] {
        &self.bytes[..self.used]
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod backend {
    use super::*;
    use std::{
        collections::VecDeque,
        ffi::{CStr, CString},
        fs::{File, Metadata},
        io,
        mem::{MaybeUninit, offset_of, size_of},
        os::{
            fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
            unix::{ffi::OsStrExt, ffi::OsStringExt, fs::MetadataExt},
        },
        ptr::{self, NonNull},
    };

    // Use target-specific constants from the existing pinned libc dependency.
    const DIRECTORY_FLAGS: libc::c_int =
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

    /// One current snapshot and handle; history owns handles, never cached snapshots.
    /// Parent is explicitly allowed outside the initial folder. This is not a jail.
    /// Back reloads visited contents and consumes a history item only on success.
    #[derive(Clone)]
    pub struct Destination(std::sync::Arc<Directory>);
    pub struct Explorer {
        directory: std::sync::Arc<Directory>,
        current: Snapshot,
        history: VecDeque<std::sync::Arc<Directory>>,
    }

    // atime is intentionally excluded: our own reads can change it. This is
    // mutation detection, not an immutable snapshot or a universal I/O timeout.
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Stamp {
        dev: u64,
        ino: u64,
        len: u64,
        mtime: (i64, i64),
        ctime: (i64, i64),
    }
    fn reader_stamp(file: &File) -> Result<Stamp> {
        let m = file.metadata().map_err(|e| format!("reader fstat: {e}"))?;
        if !m.is_file() {
            return Err("reader handle is not a regular file".into());
        }
        Ok(Stamp {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        })
    }
    /// Authorized regular-file owner. Navigation never clones this capability.
    /// Explicit image staging duplicates only the descriptor and its baseline.
    pub struct ReaderFile {
        file: File,
        baseline: Stamp,
        #[cfg(test)]
        chunk_read_limit: Option<(u64, usize)>,
    }
    impl ReaderFile {
        /// Explicit image-only staging of the SAME authorized open description.
        /// Positional reads do not share a cursor. Baseline is copied, never reset.
        pub fn try_clone(&self) -> Result<Self> {
            Ok(Self {
                file: self
                    .file
                    .try_clone()
                    .map_err(|e| format!("stage image handle: {e}"))?,
                baseline: self.baseline,
                #[cfg(test)]
                chunk_read_limit: self.chunk_read_limit,
            })
        }
        /// Worker-only bounded immutable image acquisition. Raw4096 and read_entry
        /// retain their original independent guards. No caller-chosen length.
        pub fn read_image_bytes(
            &self,
            cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            use std::os::unix::fs::FileExt;
            use std::sync::atomic::Ordering;
            const CAP: u64 = 4 * 1024 * 1024;
            if cancelled.load(Ordering::Relaxed) {
                return Err("image acquisition cancelled".into());
            }
            let before = reader_stamp(&self.file)?;
            if before != self.baseline || before.len > CAP {
                return Err(
                    "image source changed or exceeds 4 MiB encoded cap; C retries same handle"
                        .into(),
                );
            }
            let len = usize::try_from(before.len).map_err(|_| "image length overflow")?;
            let mut bytes = vec![0; len];
            let mut used = 0_usize;
            // <=64 KiB per syscall, <=128 attempts including EINTR/short reads.
            for _ in 0..128 {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("image acquisition cancelled".into());
                }
                if used == len {
                    break;
                }
                let end = used
                    .checked_add(64 * 1024)
                    .ok_or("image range overflow")?
                    .min(len);
                match self.file.read_at(&mut bytes[used..end], used as u64) {
                    Ok(0) => break,
                    Ok(n) => used = used.checked_add(n).ok_or("image count overflow")?,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(format!("image read_at: {e}")),
                }
            }
            if reader_stamp(&self.file)? != before || used != len {
                return Err("image changed/incomplete; acquired bytes discarded".into());
            }
            Ok(bytes)
        }
        /// Video-only acquisition: reject >1 MiB BEFORE allocation/read. Reuses
        /// held baseline and finite positional acquisition; never a native path.
        pub fn read_video_still_bytes(
            &self,
            cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            let before = reader_stamp(&self.file)?;
            if before != self.baseline || before.len == 0 || before.len > 1024 * 1024 {
                return Err("MP4 still source changed, empty or exceeds 1 MiB".into());
            }
            // read_image_bytes checks THIS original baseline before and after;
            // a growth race cannot allocate beyond the already admitted baseline.
            self.read_image_bytes(cancelled)
                .map_err(|e| format!("held MP4 still acquisition: {e}"))
        }
        /// Worker-only static-model acquisition: identical held baseline, 4 MiB,
        /// 64 KiB/syscall and 128-attempt policy to images; never a display path.
        pub fn read_model_bytes(
            &self,
            cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            self.read_image_bytes(cancelled)
                .map_err(|e| format!("held model acquisition: {e}"))
        }
        /// Explicit recovery accepts a new baseline on THIS handle, never a path.
        pub fn retry_from_zero(&mut self) -> Result<ReaderChunk> {
            self.baseline = reader_stamp(&self.file)?;
            self.read_chunk(0)
        }
        /// Test-only syscall sizing, not returned-data/error injection: every byte
        /// still comes from read_at on this authorized held file and original stamp.
        #[cfg(test)]
        pub(crate) fn limit_chunk_reads_from(&mut self, offset: u64, bytes: usize) {
            assert!(bytes > 0);
            self.chunk_read_limit = Some((offset, bytes));
        }
        pub fn read_chunk(&self, offset: u64) -> Result<ReaderChunk> {
            use std::os::unix::fs::FileExt;
            let end_limit = offset
                .checked_add(READER_CHUNK_BYTES as u64)
                .ok_or("reader offset overflow")?;
            if end_limit > libc::off_t::MAX as u64 {
                return Err("reader range exceeds OS offset limit".into());
            }
            let before = reader_stamp(&self.file)?;
            if before != self.baseline {
                return Err(
                    "opened file changed since baseline; C retries same handle from zero".into(),
                );
            }
            let wanted = before
                .len
                .saturating_sub(offset)
                .min(READER_CHUNK_BYTES as u64) as usize;
            let mut chunk = ReaderChunk {
                bytes: [0; READER_CHUNK_BYTES],
                used: 0,
                start: offset,
                end: offset,
                observed_len: before.len,
                eof: offset >= before.len,
            };
            // Finite syscall count even for repeated EINTR/short reads. No cursor,
            // allocation by length, lookahead, or cache of previously visited pages.
            for _ in 0..8 {
                if chunk.used == wanted {
                    break;
                }
                let at = offset
                    .checked_add(chunk.used as u64)
                    .ok_or("reader offset overflow")?;
                #[cfg(not(test))]
                let read_end = wanted;
                #[cfg(test)]
                let read_end = match self.chunk_read_limit {
                    Some((from, cap)) if offset >= from => wanted.min(chunk.used.saturating_add(cap)),
                    _ => wanted,
                };
                match self.file.read_at(&mut chunk.bytes[chunk.used..read_end], at) {
                    Ok(0) => break,
                    Ok(n) => chunk.used += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(format!("reader read_at: {e}")),
                }
            }
            let after = reader_stamp(&self.file)?;
            if after != before {
                return Err("opened file changed while reading; staged bytes discarded".into());
            }
            if chunk.used != wanted {
                return Err(
                    "incomplete reader chunk (8-attempt budget or unexpected short EOF)".into(),
                );
            }
            chunk.end = offset
                .checked_add(chunk.used as u64)
                .ok_or("reader range overflow")?;
            chunk.eof = chunk.end >= after.len;
            Ok(chunk)
        }
    }

    impl Explorer {
        #[cfg(target_os = "macos")]
        pub(crate) fn placement_directory(&self) -> Result<File> {
            self.directory
                .0
                .try_clone()
                .map_err(|e| format!("retain placement directory: {e}"))
        }
        /// Open exactly once, relative to the held directory and original component.
        /// A raced special object may be opened transiently; it is never body-read.
        pub fn open_reader(&self, index: usize) -> Result<ReaderFile> {
            let entry = self
                .current
                .entries
                .get(index)
                .ok_or("entry no longer selected")?;
            if entry.kind != Kind::File {
                return Err("reader opens only regular files, not links or special files".into());
            }
            let name = component(&entry.name)?;
            // SAFETY: live directory, validated component; no creation or writes.
            let fd = unsafe {
                libc::openat(
                    self.directory.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_CLOEXEC
                        | libc::O_NOCTTY,
                )
            };
            if fd < 0 {
                return Err(format!("open reader: {}", io::Error::last_os_error()));
            }
            // SAFETY: newly acquired descriptor, immediately given one owner.
            let file = unsafe { File::from_raw_fd(fd) };
            let baseline = reader_stamp(&file)?;
            Ok(ReaderFile {
                file, baseline,
                #[cfg(test)]
                chunk_read_limit: None,
            })
        }
        pub fn new(path: &Path) -> Result<Self> {
            let result = (|| {
                let directory = Directory::initial(path)?;
                let current = load(&directory)?;
                Ok(Self {
                    directory: std::sync::Arc::new(directory),
                    current,
                    history: VecDeque::with_capacity(MAX_HISTORY),
                })
            })();
            result.map_err(|error: String| {
                format!(
                    "local folder {}: {error}; retry or choose another directory",
                    escape_display(path.as_os_str())
                )
            })
        }

        /// Share immutable directory authority. Enumeration always opens a fresh
        /// '.' description, so neither clones nor bookmarks share stream offsets.
        pub fn try_clone(&self) -> Result<Self> {
            Ok(Self {
                directory: self.directory.clone(),
                current: self.current.clone(),
                history: self.history.clone(),
            })
        }
        pub fn destination(&self) -> Destination {
            Destination(self.directory.clone())
        }
        pub fn jump(&mut self, destination: &Destination) -> Result<()> {
            let current = load(&destination.0).map_err(|e| retained("Map travel", e))?;
            self.commit(destination.0.clone(), current);
            Ok(())
        }
        /// Rebuild the bounded Back stack from retained route authorities, never paths.
        pub fn restore_route(&mut self, route: &[Destination]) -> Result<()> {
            if route.is_empty() || route.len() > MAX_HISTORY {
                return Err("Invalid retained route length".into());
            }
            let target = route.last().unwrap();
            let current = load(&target.0).map_err(|e| retained("Route return", e))?;
            self.directory = target.0.clone();
            self.current = current;
            self.history = route[..route.len() - 1]
                .iter()
                .map(|d| d.0.clone())
                .collect();
            Ok(())
        }

        pub fn current(&self) -> &Snapshot {
            &self.current
        }

        pub fn map_identity(&self) -> Result<[u64; 2]> {
            let m = self.directory.metadata()?;
            Ok([m.dev(), m.ino()])
        }
        /// Compare acquired directories, never a display path or symlink target.
        /// Ascend only no-follow '..' handles; a bounded/failed walk refuses.
        pub fn is_within_directory(&self, ancestor: &File) -> Result<bool> {
            directory_is_within(&self.directory.0, ancestor)
        }
        pub fn contains_directory(&self, child: &File) -> Result<bool> {
            directory_is_within(child, &self.directory.0)
        }
        pub fn is_within_explorer(&self, ancestor: &Self) -> Result<bool> {
            self.is_within_directory(&ancestor.directory.0)
        }
        /// Refuse any acquired candidate inside room-owned layout metadata,
        /// including aliases and late reparenting. Never initialize to inspect.
        pub fn check_cache_directory(candidate: &File) -> Result<()> {
            let mut current = Directory(candidate.try_clone().map_err(|e| e.to_string())?);
            for _ in 0..1024 {
                let here = current.metadata()?;
                let parent = current.open_at(c"..")?;
                let above = parent.metadata()?;
                let mut layout = MaybeUninit::<libc::stat>::uninit();
                // SAFETY: held parent, constant component, writable stat storage.
                let found = unsafe { libc::fstatat(parent.0.as_raw_fd(), c".hair-layout".as_ptr(), layout.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
                if found == 0 {
                    // SAFETY: successful fstatat initialized the entire stat.
                    let layout = unsafe { layout.assume_init() };
                    if (here.dev(), here.ino()) == (layout.st_dev as u64, layout.st_ino) {
                        return Err("Cache cannot initialize room-owned .hair-layout or its descendants".into());
                    }
                } else {
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::NotFound { return Err(format!("Cache layout-boundary inspection: {error}")); }
                }
                if (here.dev(), here.ino()) == (above.dev(), above.ino()) { return Ok(()); }
                current = parent;
            }
            Err("Cache layout ancestor inspection budget exceeded; nothing initialized".into())
        }
        pub fn has_visited_directory(&self, candidate: &File) -> Result<bool> {
            let target = candidate.metadata().map_err(|e| e.to_string())?;
            for directory in std::iter::once(&self.directory).chain(self.history.iter()) {
                let m = directory.metadata()?;
                if (m.dev(), m.ino()) == (target.dev(), target.ino()) { return Ok(true); }
            }
            Ok(false)
        }
        pub fn map_back_identity(&self) -> Result<Option<[u64; 2]>> {
            self.history
                .back()
                .map(|d| d.metadata().map(|m| [m.dev(), m.ino()]))
                .transpose()
        }
        /// Compare live, pinned directory handles without reopening display paths.
        /// The index refers to the existing bounded Back deque, oldest first.
        /// Observing a match neither consumes history nor changes either explorer.
        pub fn history_matches_current(&self, index: usize, other: &Self) -> Result<bool> {
            let Some(directory) = self.history.get(index) else {
                return Ok(false);
            };
            let saved = directory.metadata()?;
            let current = other.directory.metadata()?;
            Ok(saved.dev() == current.dev() && saved.ino() == current.ino())
        }

        /// Read only on explicit inspect, from the retained directory and raw name.
        /// Nonblocking/no-follow open then handle validation prevents FIFO/link swaps.
        /// It does not promise a timeout for arbitrary kernel/network filesystem I/O.
        pub fn read_entry(&self, index: usize, limit: u64) -> Result<Vec<u8>> {
            use std::io::Read;
            if limit > MAX_BODY_BYTES {
                return Err(format!(
                    "inspection limit must be <= {MAX_BODY_BYTES} bytes"
                ));
            }
            let entry = self
                .current
                .entries
                .get(index)
                .ok_or("entry no longer selected")?;
            if entry.kind != Kind::File {
                return Err(
                    "inspect bodies only for regular files; links/special files are not opened"
                        .into(),
                );
            }
            let name = component(&entry.name)?;
            // SAFETY: retained directory, validated single component and read-only flags.
            let fd = unsafe {
                libc::openat(
                    self.directory.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC
                        | libc::O_NONBLOCK
                        | libc::O_NOCTTY,
                )
            };
            if fd < 0 {
                return Err(format!(
                    "inspect {}: {}",
                    escape_display(&entry.name),
                    io::Error::last_os_error()
                ));
            }
            // SAFETY: successful fresh descriptor, exactly one owner from this point.
            let file = unsafe { File::from_raw_fd(fd) };
            let metadata = file
                .metadata()
                .map_err(|e| format!("inspect metadata: {e}"))?;
            if !metadata.is_file() || metadata.len() > limit {
                return Err(format!(
                    "inspect {}: must be regular and <= {limit} bytes",
                    escape_display(&entry.name)
                ));
            }
            let mut bytes = Vec::new();
            file.take(limit + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("inspect read: {e}"))?;
            if bytes.len() as u64 > limit {
                return Err("file grew beyond inspection budget; original file unchanged".into());
            }
            Ok(bytes)
        }

        /// Enter one raw child name from this snapshot, never an escaped label/path.
        /// A replacement symlink is rejected by openat even if its old kind was Folder.
        pub fn enter(&mut self, index: usize) -> Result<()> {
            let result = (|| {
                let entry =
                    self.current.entries.get(index).ok_or_else(|| {
                        format!("entry index {index} is not in the current snapshot")
                    })?;
                if entry.kind != Kind::Folder {
                    return Err(format!(
                        "entry {} is {}; only directories can be entered and entry links are not followed",
                        escape_display(&entry.name),
                        entry.kind.label()
                    ));
                }
                let name = component(&entry.name)?;
                let directory = self
                    .directory
                    .open_at(&name)
                    .map_err(|error| format!("entry {}: {error}", escape_display(&entry.name)))?;
                let current = load(&directory)?;
                Ok((directory, current))
            })();
            let (directory, current) = result.map_err(|error| retained("Enter", error))?;
            self.commit(std::sync::Arc::new(directory), current);
            Ok(())
        }

        /// One explicit handle-relative Parent step. Equal opened dev/inode means root.
        /// A root no-op does not enumerate again or add a Back item.
        pub fn up(&mut self) -> Result<bool> {
            let result = (|| {
                let parent = self.directory.open_at(c"..")?;
                let here = self.directory.metadata()?;
                let there = parent.metadata()?;
                if here.dev() == there.dev() && here.ino() == there.ino() {
                    return Ok(None);
                }
                let current = load(&parent)?;
                Ok(Some((parent, current)))
            })();
            match result.map_err(|error| retained("Parent", error))? {
                None => Ok(false),
                Some((directory, current)) => {
                    self.commit(std::sync::Arc::new(directory), current);
                    Ok(true)
                }
            }
        }

        pub fn back(&mut self) -> Result<bool> {
            let Some(directory) = self.history.back() else {
                return Ok(false);
            };
            let current = load(directory).map_err(|error| retained("Back", error))?;
            // No history mutation occurred during load. Only now consume the handle.
            if let Some(directory) = self.history.pop_back() {
                self.directory = directory;
                self.current = current;
                Ok(true)
            } else {
                Ok(false)
            }
        }

        /// Explicit immediate re-enumeration; no background watcher or recursive scan.
        pub fn refresh(&mut self) -> Result<()> {
            let current = load(&self.directory).map_err(|error| retained("Refresh", error))?;
            self.current = current;
            Ok(())
        }

        fn commit(&mut self, directory: std::sync::Arc<Directory>, current: Snapshot) {
            let previous = std::mem::replace(&mut self.directory, directory);
            self.current = current;
            if self.history.len() == MAX_HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(previous);
        }
    }

    pub(super) fn snapshot(selected: &Path) -> Result<Snapshot> {
        Explorer::new(selected).map(|explorer| explorer.current)
    }

    fn retained(action: &str, error: String) -> String {
        format!(
            "{action}: {error}; current snapshot and Back history kept; try Refresh, Back, or Parent"
        )
    }

    fn component(name: &OsStr) -> Result<CString> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
            return Err(format!("invalid child component {}", escape_display(name)));
        }
        CString::new(bytes)
            .map_err(|_| format!("child component {} contains NUL", escape_display(name)))
    }

    fn directory_is_within(child: &File, ancestor: &File) -> Result<bool> {
        let target = ancestor.metadata().map_err(|e| e.to_string())?;
        if !target.is_dir() { return Err("Ancestor authority is not a directory".into()); }
        let mut current = Directory(child.try_clone().map_err(|e| e.to_string())?);
        for _ in 0..1024 {
            let here = current.metadata()?;
            if !here.is_dir() { return Err("Child authority is not a directory".into()); }
            if (here.dev(), here.ino()) == (target.dev(), target.ino()) { return Ok(true); }
            let parent = current.open_at(c"..")?;
            let above = parent.metadata()?;
            if (here.dev(), here.ino()) == (above.dev(), above.ino()) { return Ok(false); }
            current = parent;
        }
        Err("Directory ancestor inspection budget exceeded; no placement/adoption".into())
    }

    struct Directory(File);

    impl Directory {
        fn initial(path: &Path) -> Result<Self> {
            let path = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| "chosen directory path contains NUL".to_string())?;
            // Initial selection is the only pathname-based directory open. Aliases
            // are explicitly allowed here, but the result must be a directory.
            // SAFETY: path is NUL-terminated; flags do not request a mode argument.
            let fd = unsafe {
                libc::openat(
                    libc::AT_FDCWD,
                    path.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            Self::opened(fd)
        }

        fn open_at(&self, name: &CStr) -> Result<Self> {
            // Callers pass a validated child component or the explicit "."/"..".
            // SAFETY: the retained FD is live and name is NUL-terminated.
            let fd = unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), DIRECTORY_FLAGS) };
            Self::opened(fd)
        }

        fn opened(fd: RawFd) -> Result<Self> {
            if fd < 0 {
                return Err(format!("open directory: {}", io::Error::last_os_error()));
            }
            // SAFETY: successful openat returned a new, exclusively owned FD.
            // Establish ownership before any subsequent fallible validation.
            let directory = Self(unsafe { File::from_raw_fd(fd) });
            if !directory.metadata()?.is_dir() {
                return Err("opened handle is not a directory".into());
            }
            Ok(directory)
        }

        fn metadata(&self) -> Result<Metadata> {
            self.0
                .metadata()
                .map_err(|error| format!("directory handle metadata: {error}"))
        }

        fn kind(&self, name: &CStr) -> Result<Kind> {
            let mut stat = MaybeUninit::<libc::stat>::uninit();
            // SAFETY: stat is writable storage and both FD and C string are live.
            // AT_SYMLINK_NOFOLLOW classifies a link itself, including dangling links.
            let status = unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if status != 0 {
                return Err(format!("entry metadata: {}", io::Error::last_os_error()));
            }
            // SAFETY: successful fstatat initialized the stat structure.
            let mode = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
            Ok(match mode {
                libc::S_IFDIR => Kind::Folder,
                libc::S_IFREG => Kind::File,
                libc::S_IFLNK => Kind::Symlink,
                _ => Kind::Special,
            })
        }

        fn display_path(&self) -> Result<PathBuf> {
            #[cfg(target_os = "macos")]
            let path = {
                let mut bytes = vec![0_u8; libc::PATH_MAX as usize];
                // SAFETY: F_GETPATH expects a writable PATH_MAX-sized buffer.
                let status =
                    unsafe { libc::fcntl(self.0.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) };
                if status != 0 {
                    return Err(format!(
                        "directory handle path: {}",
                        io::Error::last_os_error()
                    ));
                }
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .ok_or_else(|| "directory handle path is not NUL-terminated".to_string())?;
                bytes.truncate(end);
                PathBuf::from(OsString::from_vec(bytes))
            };
            #[cfg(target_os = "linux")]
            let path = std::fs::read_link(format!("/proc/self/fd/{}", self.0.as_raw_fd()))
                .map_err(|error| {
                    format!("directory handle path (/proc/self/fd required): {error}")
                })?;

            if !path.is_absolute() || self.metadata()?.nlink() == 0 {
                return Err("directory handle has no live absolute display path".into());
            }
            Ok(path)
        }
    }

    // fdopendir owns a fresh open-file-description, not dup(currentfd). A dup
    // shares its directory offset and can make a later refresh incorrectly see EOF.
    struct DirectoryStream(Option<NonNull<libc::DIR>>);

    impl DirectoryStream {
        fn new(directory: &Directory) -> Result<Self> {
            let fresh = directory.open_at(c".")?;
            let descriptor: OwnedFd = fresh.0.into();
            // SAFETY: descriptor is an owned, readable directory FD. On failure
            // fdopendir does not consume it, so OwnedFd still closes it on return.
            let stream = unsafe { libc::fdopendir(descriptor.as_raw_fd()) };
            let stream = NonNull::new(stream)
                .ok_or_else(|| format!("start enumeration: {}", io::Error::last_os_error()))?;
            // Transfer ownership only after fdopendir succeeds. closedir now owns it.
            let _ = descriptor.into_raw_fd();
            Ok(Self(Some(stream)))
        }

        fn next_name(&mut self) -> Result<Option<OsString>> {
            let stream = self
                .0
                .ok_or_else(|| "enumeration stream already closed".to_string())?;
            let errno = errno_address();
            // SAFETY: errno is thread-local writable storage; stream remains live.
            let (entry, error) = unsafe {
                *errno = 0;
                let entry = libc::readdir(stream.as_ptr());
                (entry, *errno)
            };
            if entry.is_null() {
                return if error == 0 {
                    Ok(None)
                } else {
                    Err(format!(
                        "enumerate directory: {}",
                        io::Error::from_raw_os_error(error)
                    ))
                };
            }
            // SAFETY: readdir supplied a record valid until the next call on this
            // stream. Copy its bounded name now; retain no pointers into that record.
            unsafe { dirent_name(entry) }.map(Some)
        }

        fn close(mut self) -> Result<()> {
            if let Some(stream) = self.0.take() {
                // SAFETY: sole ownership is consumed here, even on reported failure.
                if unsafe { libc::closedir(stream.as_ptr()) } != 0 {
                    return Err(format!(
                        "finish enumeration: {}",
                        io::Error::last_os_error()
                    ));
                }
            }
            Ok(())
        }
    }

    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            if let Some(stream) = self.0.take() {
                // SAFETY: this is the sole remaining owner, including early errors.
                // Never separately close its descriptor or retry a failed close.
                unsafe { libc::closedir(stream.as_ptr()) };
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn errno_address() -> *mut libc::c_int {
        // SAFETY: libc returns this thread's errno location.
        unsafe { libc::__error() }
    }

    #[cfg(target_os = "linux")]
    fn errno_address() -> *mut libc::c_int {
        // SAFETY: libc returns this thread's errno location.
        unsafe { libc::__errno_location() }
    }

    unsafe fn dirent_name(entry: *const libc::dirent) -> Result<OsString> {
        // Do not create &dirent or read the entire d_name array: readdir records
        // can be shorter than sizeof(dirent). Read the ABI header, then only the
        // NUL-terminated name within d_reclen. Reject oversized records explicitly.
        // SAFETY: readdir guarantees a valid platform dirent header.
        let record_len = unsafe { ptr::addr_of!((*entry).d_reclen).read_unaligned() } as usize;
        let offset = offset_of!(libc::dirent, d_name);
        if record_len <= offset || record_len > size_of::<libc::dirent>() {
            return Err(format!(
                "unsupported directory record length {record_len}; no partial snapshot returned"
            ));
        }
        // Capacities of d_name in the pinned libc platform bindings. Do not
        // mistake struct tail padding for additional filename bytes.
        #[cfg(target_os = "macos")]
        let name_capacity = 1024;
        #[cfg(target_os = "linux")]
        let name_capacity = 256;
        let mut bytes = Vec::new();
        for index in 0..(record_len - offset).min(name_capacity) {
            // SAFETY: bounded by this record's length, stopping at its first NUL.
            let byte = unsafe { entry.cast::<u8>().add(offset + index).read() };
            if byte == 0 {
                #[cfg(target_os = "macos")]
                {
                    // SAFETY: d_namlen is part of the verified platform header.
                    let length = unsafe { ptr::addr_of!((*entry).d_namlen).read_unaligned() };
                    if usize::from(length) != bytes.len() {
                        return Err("directory record name length mismatch".into());
                    }
                }
                return Ok(OsString::from_vec(bytes));
            }
            bytes.push(byte);
        }
        Err("directory record has no bounded NUL-terminated name".into())
    }

    fn load(directory: &Directory) -> Result<Snapshot> {
        let mut stream = DirectoryStream::new(directory)?;
        let mut entries = Vec::new();
        while let Some(name) = stream.next_name()? {
            if name.as_bytes() == b"." || name.as_bytes() == b".." {
                continue;
            }
            if entries.len() == MAX_ENTRIES {
                return Err(format!(
                    "directory exceeds enumeration cap of {MAX_ENTRIES} immediate entries; no partial snapshot returned"
                ));
            }
            let component = component(&name)?;
            let kind = directory
                .kind(&component)
                .map_err(|error| format!("entry {}: {error}", escape_display(&name)))?;
            entries.push(Entry {
                name,
                path: PathBuf::new(),
                kind,
            });
        }
        stream.close()?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for pair in entries.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(format!(
                    "duplicate directory name {}; retry enumeration; no partial snapshot returned",
                    escape_display(&pair[0].name)
                ));
            }
        }
        // Resolve the display path from the opened object, never canonicalize a
        // mutable pathname. All metadata/open operations above use the retained FD.
        let folder = directory.display_path()?;
        for entry in &mut entries {
            entry.path = folder.join(&entry.name);
        }
        Ok(Snapshot { folder, entries })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod backend {
    use super::*;

    const UNSUPPORTED: &str = "handle-relative local exploration is supported only on macOS and Linux; no pathname fallback is used";

    /// Other platforms have no navigation backend; construction returns an error.
    pub struct Explorer {
        current: Snapshot,
    }
    #[derive(Clone)]
    pub struct Destination;
    pub struct ReaderFile;
    impl ReaderFile {
        pub fn try_clone(&self) -> Result<Self> {
            Err(UNSUPPORTED.into())
        }
        pub fn read_image_bytes(
            &self,
            _cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            Err(UNSUPPORTED.into())
        }
        pub fn read_video_still_bytes(
            &self,
            _cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            Err("MP4 still decoding requires macOS; no pathname fallback".into())
        }
        pub fn read_model_bytes(
            &self,
            _cancelled: &std::sync::atomic::AtomicBool,
        ) -> Result<Vec<u8>> {
            Err(UNSUPPORTED.into())
        }
        pub fn read_chunk(&self, _offset: u64) -> Result<ReaderChunk> {
            Err(UNSUPPORTED.into())
        }
        pub fn retry_from_zero(&mut self) -> Result<ReaderChunk> {
            Err(UNSUPPORTED.into())
        }
    }
    impl Explorer {
        pub fn open_reader(&self, _index: usize) -> Result<ReaderFile> {
            Err(UNSUPPORTED.into())
        }
        pub fn new(_path: &Path) -> Result<Self> {
            Err(UNSUPPORTED.into())
        }
        pub fn destination(&self) -> Destination {
            Destination
        }
        pub fn jump(&mut self, _destination: &Destination) -> Result<()> {
            Err(UNSUPPORTED.into())
        }
        pub fn restore_route(&mut self, _route: &[Destination]) -> Result<()> {
            Err(UNSUPPORTED.into())
        }
        pub fn current(&self) -> &Snapshot {
            &self.current
        }
        pub fn try_clone(&self) -> Result<Self> {
            Err(UNSUPPORTED.into())
        }
        pub fn map_identity(&self) -> Result<[u64; 2]> {
            Err(UNSUPPORTED.into())
        }
        pub fn map_back_identity(&self) -> Result<Option<[u64; 2]>> {
            Err(UNSUPPORTED.into())
        }
        pub fn history_matches_current(&self, _index: usize, _other: &Self) -> Result<bool> {
            Err(UNSUPPORTED.into())
        }
        pub fn read_entry(&self, _index: usize, _limit: u64) -> Result<Vec<u8>> {
            Err(UNSUPPORTED.into())
        }
        pub fn enter(&mut self, _index: usize) -> Result<()> {
            Err(UNSUPPORTED.into())
        }
        pub fn up(&mut self) -> Result<bool> {
            Err(UNSUPPORTED.into())
        }
        pub fn back(&mut self) -> Result<bool> {
            Err(UNSUPPORTED.into())
        }
        pub fn refresh(&mut self) -> Result<()> {
            Err(UNSUPPORTED.into())
        }
    }
    pub(super) fn snapshot(selected: &Path) -> Result<Snapshot> {
        Explorer::new(selected).map(|explorer| explorer.current)
    }
}
