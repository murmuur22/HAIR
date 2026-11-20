use hair::local_explorer::Explorer;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Fixture {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("hair-inspect-{tag}-{}", std::process::id()));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("Original.txt"), b"original bytes\n").unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn index(e: &Explorer) -> usize {
    e.current()
        .entries
        .iter()
        .position(|x| x.name == "Original.txt")
        .unwrap()
}
#[test]
fn explicit_inspection_reads_only_bounded_original_regular_bytes() {
    let f = Fixture::new("regular");
    let e = Explorer::new(&f.0).unwrap();
    let i = index(&e);
    assert_eq!(
        e.read_entry(i, 1024)
            .expect("explicit inspect should read selected regular file"),
        b"original bytes\n"
    );
    assert!(
        e.read_entry(i, 2).is_err(),
        "oversized content must not be silently truncated"
    );
    assert!(
        e.read_entry(i, u64::MAX).is_err(),
        "caller cannot remove the hard body-read bound or overflow cap+1"
    );
    assert_eq!(
        fs::read(f.0.join("Original.txt")).unwrap(),
        b"original bytes\n"
    );
}
#[cfg(unix)]
#[test]
fn inspection_uses_retained_directory_not_rebound_display_path() {
    let f = Fixture::new("rebind");
    let old = f.0.join("chosen");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("Original.txt"), b"from retained directory").unwrap();
    let e = Explorer::new(&old).unwrap();
    let i = index(&e);
    fs::rename(&old, f.0.join("moved")).unwrap();
    fs::create_dir(&old).unwrap();
    fs::write(old.join("Original.txt"), b"wrong replacement").unwrap();
    assert_eq!(
        e.read_entry(i, 1024)
            .expect("retained directory should survive rename"),
        b"from retained directory"
    );
}
#[cfg(unix)]
#[test]
fn replacement_symlink_is_not_followed_on_inspect() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("symlink");
    let e = Explorer::new(&f.0).unwrap();
    let i = index(&e);
    fs::remove_file(f.0.join("Original.txt")).unwrap();
    fs::write(f.0.join("Other.txt"), b"not selected").unwrap();
    symlink("Other.txt", f.0.join("Original.txt")).unwrap();
    assert!(
        e.read_entry(i, 1024).is_err(),
        "stale regular-file selection must not follow replacement link"
    );
}
#[cfg(unix)]
#[test]
fn fifo_replacement_inspection_is_bounded_and_reaps_its_helper() {
    use std::{
        ffi::CString,
        os::unix::ffi::OsStrExt,
        process::Command,
        time::{Duration, Instant},
    };
    if std::env::var_os("HAIR_FIFO_CHILD").is_some() {
        let f = Fixture::new("fifo-child");
        let e = Explorer::new(&f.0).unwrap();
        let i = index(&e);
        let p = f.0.join("Original.txt");
        fs::remove_file(&p).unwrap();
        let c = CString::new(p.as_os_str().as_bytes()).unwrap();
        // SAFETY: local synthetic fixture, valid C string and ordinary FIFO permission bits.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(
            e.read_entry(i, 1024).is_err(),
            "FIFO must be refused, not read"
        );
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fifo_replacement_inspection_is_bounded_and_reaps_its_helper",
            "--nocapture",
        ])
        .env("HAIR_FIFO_CHILD", "1")
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if status.is_none() {
        let p = std::env::temp_dir().join(format!("hair-inspect-fifo-child-{}", child.id()));
        if p.exists() {
            fs::remove_dir_all(p).unwrap();
        }
    }
    assert!(
        status.is_some_and(|s| s.success()),
        "inspection hung or failed; owned helper was reaped"
    );
}
