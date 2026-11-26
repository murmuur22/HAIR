// Controlled posix_spawn file actions hold a child before exec using two owned
// FIFOs. Rust's ordinary Command uses posix_spawnp here (not pthread_atfork).
use std::os::fd::AsRawFd;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

struct ParkedSpawn {
    release: std::path::PathBuf,
    child: Option<std::thread::JoinHandle<()>>,
    watchdog: Option<std::thread::JoinHandle<bool>>,
    cancel: std::sync::mpsc::Sender<()>,
}
impl ParkedSpawn {
    fn new(base: &std::path::Path) -> Self {
        let ready = base.join("spawn-ready");
        let release = base.join("spawn-release");
        for p in [&ready, &release] {
            let p = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(p.as_ptr(), 0o600) }, 0);
        }
        // The watchdog makes failed rendezvous bounded, never a passing test.
        // It does not retry a storage operation or change a storage result.
        let (cancel, receiver) = std::sync::mpsc::channel();
        let paths = [ready.clone(), release.clone()];
        let watchdog = std::thread::spawn(move || {
            let timed_out = receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .is_err();
            if timed_out {
                let _held = paths.map(|p| {
                    std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(p)
                        .unwrap()
                });
            }
            timed_out
        });
        let paths = [ready.clone(), release.clone()];
        // Reserve distinct destinations, never fixed descriptor numbers that
        // could overwrite an unrelated parallel test's inherited lock fd.
        let destinations = [
            std::fs::File::open(base).unwrap(),
            std::fs::File::open(base).unwrap(),
        ];
        let child = std::thread::spawn(move || unsafe {
            let mut actions = std::mem::MaybeUninit::uninit();
            assert_eq!(libc::posix_spawn_file_actions_init(actions.as_mut_ptr()), 0);
            let mut actions = actions.assume_init();
            for (i, p) in paths.iter().enumerate() {
                let p = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
                assert_eq!(
                    libc::posix_spawn_file_actions_addopen(
                        &mut actions,
                        destinations[i].as_raw_fd(),
                        p.as_ptr(),
                        libc::O_RDONLY,
                        0,
                    ),
                    0
                );
            }
            let mut pid = 0;
            let argv = [c"true".as_ptr().cast_mut(), std::ptr::null_mut()];
            let envp = [std::ptr::null_mut()];
            let rc = libc::posix_spawnp(
                &mut pid,
                c"/usr/bin/true".as_ptr(),
                &actions,
                std::ptr::null(),
                argv.as_ptr(),
                envp.as_ptr(),
            );
            assert_eq!(libc::posix_spawn_file_actions_destroy(&mut actions), 0);
            assert_eq!(rc, 0);
            let mut status = 0;
            assert_eq!(libc::waitpid(pid, &mut status, 0), pid);
            assert_eq!(status, 0);
        });
        // Rendezvous with the first child file action; its second open cannot
        // finish until release below. No sleeps or storage retries.
        let _ready = std::fs::OpenOptions::new()
            .write(true)
            .open(&ready)
            .unwrap();
        Self {
            release,
            child: Some(child),
            watchdog: Some(watchdog),
            cancel,
        }
    }
}
impl Drop for ParkedSpawn {
    fn drop(&mut self) {
        let _release = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.release)
            .unwrap();
        self.child.take().unwrap().join().unwrap();
        let _ = self.cancel.send(());
        assert!(
            !self.watchdog.take().unwrap().join().unwrap(),
            "spawn rendezvous timed out; not a passing synchronization"
        );
    }
}

#[test]
fn slice4b3_scoped_release_does_not_wait_for_spawn_exec() {
    for mode in [
        "manual-drop",
        "fixture-drop",
        "publication-return",
        "publication-error",
        "publication-unwind",
    ] {
        let f = Fixture::new();
        let e = Explorer::new(&f.0).unwrap();
        let root = f.0.join(".hair-layout");
        std::fs::create_dir(&root).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(root.join("lock"))
            .unwrap();
        let m = lock.metadata().unwrap();
        assert_ne!(
            unsafe { libc::fcntl(lock.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        let mut child = None;
        if mode == "manual-drop" {
            assert_eq!(
                unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
            child = Some(ParkedSpawn::new(&f.0));
            drop(lock);
        } else if mode == "fixture-drop" {
            let lock = mac::HeldFlock::acquire(lock, libc::LOCK_EX).unwrap();
            child = Some(ParkedSpawn::new(&f.0));
            assert!(positions(&e).is_err());
            drop(lock);
        } else {
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                mac::publish(
                    &e,
                    OsStr::new("First"),
                    b"first",
                    &pos(),
                    &AtomicBool::new(false),
                    |stage| {
                        if stage == 3 {
                            child = Some(ParkedSpawn::new(&f.0));
                            let read = positions(&e);
                            let write = publish(
                                &e,
                                OsStr::new("During"),
                                b"no",
                                &pos(),
                                &AtomicBool::new(false),
                            );
                            assert!(read.is_err(), "live writer must refuse reads: {read:?}");
                            assert!(
                                matches!(write, Outcome::NotCommitted(_)),
                                "live writer must refuse writes: {write:?}"
                            );
                            if mode == "publication-error" {
                                return Err("controlled precommit refusal".into());
                            }
                            assert_ne!(mode, "publication-unwind", "controlled validator unwind");
                        }
                        Ok(())
                    },
                )
            }));
            match mode {
                "publication-error" => assert_eq!(
                    out.unwrap(),
                    Outcome::NotCommitted("controlled precommit refusal".into())
                ),
                "publication-unwind" => assert!(out.is_err()),
                _ => assert!(matches!(out.unwrap(), Outcome::Committed { .. })),
            }
            if mode != "publication-return" {
                assert!(!f.0.join("First").exists());
            }
            drop(lock);
        }
        let named = std::fs::metadata(root.join("lock")).unwrap();
        assert_eq!((m.dev(), m.ino()), (named.dev(), named.ino()));
        let read = positions(&e);
        let write = publish(
            &e,
            OsStr::new(mode),
            b"later",
            &pos(),
            &AtomicBool::new(false),
        );
        println!(
            "{mode}: fixture={:?} dev={} ino={} before exec: read={read:?} write={write:?}",
            f.0,
            m.dev(),
            m.ino()
        );
        if mode == "manual-drop" {
            // OS mechanism control: close alone does not release an inherited
            // open description. This remains true after the production repair.
            assert!(
                read.as_ref().is_err_and(|s| s.contains("Layout is busy")),
                "{read:?}"
            );
            assert!(
                matches!(&write, Outcome::NotCommitted(s) if s.contains("Layout is busy")),
                "{write:?}"
            );
        } else {
            assert!(read.is_ok(), "scope ended but child still pins flock: {read:?}");
            assert!(matches!(write, Outcome::Committed { .. }), "{write:?}");
        }
        drop(child);
        let read = positions(&e);
        let write = publish(
            &e,
            OsStr::new("AfterExec"),
            b"later",
            &pos(),
            &AtomicBool::new(false),
        );
        println!("{mode}: after exec/exit: read={read:?} write={write:?}");
        assert!(read.is_ok(), "{read:?}");
        assert!(matches!(write, Outcome::Committed { .. }), "{write:?}");
    }
}

#[test]
fn slice4b3_shared_guards_release_only_their_own_description() {
    let f = Fixture::new();
    let root = f.0.join(".hair-layout");
    std::fs::create_dir(&root).unwrap();
    let file = std::fs::File::create(root.join("lock")).unwrap();
    let first = mac::HeldFlock::acquire(file, libc::LOCK_SH).unwrap();
    let file = std::fs::File::open(root.join("lock")).unwrap();
    let second = mac::HeldFlock::acquire(file, libc::LOCK_SH).unwrap();
    let child = ParkedSpawn::new(&f.0);
    let exclusive = || {
        mac::HeldFlock::acquire(
            std::fs::File::open(root.join("lock")).unwrap(),
            libc::LOCK_EX,
        )
    };
    assert!(exclusive().is_err());
    drop(first);
    assert!(exclusive().is_err(), "independent reader must remain locked");
    drop(second);
    let writer = exclusive().unwrap();
    assert!(exclusive().is_err(), "failed acquisition must not unlock writer");
    drop(writer);
    assert!(exclusive().is_ok(), "child must not pin released scopes");
    drop(child);
}

#[test]
fn slice4b3_inherited_guard_cannot_unlock_parent_transaction() {
    let f = Fixture::new();
    let e = Explorer::new(&f.0).unwrap();
    let root = f.0.join(".hair-layout");
    std::fs::create_dir(&root).unwrap();
    let file = std::fs::File::create(root.join("lock")).unwrap();
    let lock = mac::HeldFlock::acquire(file, libc::LOCK_EX).unwrap();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        // This branch only gets pid, closes the inherited fd, and _exits.
        // No allocation, mutex, test assertion or Rust unwinding after fork.
        drop(lock);
        unsafe { libc::_exit(0) };
    }
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    assert_eq!(status, 0);
    let result = positions(&e);
    assert!(result.as_ref().is_err_and(|s| s.contains("Layout is busy")), "{result:?}");
    drop(lock);
    assert!(positions(&e).unwrap().is_empty());
}
