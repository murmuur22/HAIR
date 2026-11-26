// Internal fault injection uses real owned disposable storage, not fake API output.
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(d: &Decoration) -> Self {
        let p = std::env::temp_dir().join(format!("hair-slice4b-fault-{}-{}", std::process::id(), d.placement_id));
        std::fs::create_dir(&p).unwrap(); Self(p)
    }
}
impl Drop for Fixture { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn new_decoration() -> Decoration {
    Decoration::from_entry(&crate::decoration::Entry { id: "choice".into(), tag: "hair.seating.chair".into(), asset: "mesh".into(), name: "Chair".into(), defaults: crate::decoration::Defaults { collision: true } }, [2.,-1.], 0.5, [0.8;3]).unwrap()
}
#[test]
fn slice4b_precommit_and_postcommit_faults_have_distinct_reopen_outcomes() {
    for stage in 0..=2 {
        let d = new_decoration(); let f = Fixture::new(&d); let e = Explorer::new(&f.0).unwrap();
        let out = place_decoration(&e, &d, &mut |_| Ok(()), |at| if at == stage { Err("injected sync/cancel boundary".into()) } else { Ok(()) });
        let reopened = super::super::read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
        if stage == 2 { assert_eq!(out, Outcome::Committed { synced: false }); assert_eq!(reopened.decorations, vec![d]); }
        else { assert!(matches!(out, Outcome::NotCommitted(_))); assert!(reopened.decorations.is_empty()); }
        if stage == 0 { assert!(!f.0.join(ROOT).exists()); } // Cancellation before prepare: zero writes.
    }
}
#[test]
fn slice4b_completed_storage_rebindings_and_in_place_edits_refuse_before_activation() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for target in ["root", "lock", "slot", "record", "bytes", "foreign-active"] {
        let d = new_decoration(); let f = Fixture::new(&d); let e = Explorer::new(&f.0).unwrap();
        let root = f.0.join(ROOT); let slot = root.join(format!("dc-{}", d.placement_id)); let pending = slot.join("pending.toml");
        let mut foreign = None;
        let out = place_decoration(&e, &d, &mut |_| Ok(()), |at| {
            if at != 1 { return Ok(()); }
            let path = match target { "root" => root.clone(), "lock" => root.join("lock"), "slot" => slot.clone(), _ => pending.clone() };
            if target == "bytes" { std::fs::write(&path, toml::to_string(&d).unwrap().replace("yaw = 0.5", "yaw = 0.6")).unwrap(); }
            else if target == "foreign-active" { std::fs::write(slot.join("decoration.toml"), b"foreign destination").unwrap(); }
            else {
                let displaced = f.0.join("held-original");
                std::fs::rename(&path, &displaced).unwrap();
                if target == "root" || target == "slot" { std::fs::create_dir(&path).unwrap(); }
                else { std::fs::write(&path, b"foreign replacement").unwrap(); }
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(if target == "root" || target == "slot" {0o700} else {0o600})).unwrap();
                foreign = Some((path.clone(), std::fs::symlink_metadata(&path).unwrap().ino()));
            }
            Ok(())
        });
        assert!(matches!(out, Outcome::NotCommitted(_)), "{target}: {out:?}");
        if let Some((path,ino)) = foreign { assert_eq!(std::fs::symlink_metadata(path).unwrap().ino(), ino); }
        assert!(!slot.join("decoration.toml").exists() || target == "foreign-active", "{target}");
        if target == "foreign-active" { assert_eq!(std::fs::read(slot.join("decoration.toml")).unwrap(), b"foreign destination"); }
    }
}
#[test]
fn slice4b_actual_late_record_is_seen_by_fresh_locked_validator() {
    use std::os::unix::fs::PermissionsExt;
    let d = new_decoration(); let f = Fixture::new(&d); let e = Explorer::new(&f.0).unwrap();
    let other = new_decoration(); let mut calls = 0;
    let out = place_decoration(&e, &d, &mut |live| {
        calls += 1;
        if !live.decorations.is_empty() { Err("fresh logical overlap refusal".into()) } else { Ok(()) }
    }, |at| {
        if at == 1 {
            // A noncooperating test writer adds an actual active record between passes.
            let tx = f.0.join(ROOT).join(format!("dc-{}", other.placement_id));
            std::fs::create_dir(&tx).unwrap(); std::fs::set_permissions(&tx, std::fs::Permissions::from_mode(0o700)).unwrap();
            let path = tx.join("decoration.toml"); std::fs::write(&path, toml::to_string(&other).unwrap()).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Ok(())
    });
    assert_eq!(calls, 2); assert_eq!(out, Outcome::NotCommitted("fresh logical overlap refusal".into()));
    assert_eq!(super::super::read_layout(&e).unwrap().decorations, vec![other]);
}
#[test]
fn slice4b_rename_errno_classification_preserves_uncertainty() {
    // Synthetic errno classification only; not claims of actual disk/transport failures.
    for code in [libc::EEXIST, libc::ENOSPC, libc::EACCES] { assert!(matches!(decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::NotCommitted(_))); }
    for code in [libc::EIO, libc::EINTR, libc::ESTALE, libc::ETIMEDOUT] { assert!(matches!(decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::Ambiguous(_))); }
}
#[test]
#[ignore = "child-only abrupt process termination fixture"]
fn slice4b_crash_child() {
    let root = std::env::var_os("HAIR_SLICE4B_CRASH_ROOT").unwrap();
    let stage: u8 = std::env::var("HAIR_SLICE4B_CRASH_STAGE").unwrap().parse().unwrap();
    let e = Explorer::new(std::path::Path::new(&root)).unwrap();
    let d = new_decoration();
    let _ = place_decoration(&e, &d, &mut |_| Ok(()), |at| {
        if at == stage { unsafe { libc::_exit(73) }; }
        Ok(())
    });
    panic!("child did not reach the requested crash boundary");
}
#[test]
fn slice4b_actual_process_termination_reopens_only_activated_metadata() {
    for stage in [1, 2] {
        let d = new_decoration();
        let f = Fixture::new(&d);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "placement_store::mac::decoration_tests::slice4b_crash_child", "--ignored", "--nocapture"])
            .env("HAIR_SLICE4B_CRASH_ROOT", &f.0)
            .env("HAIR_SLICE4B_CRASH_STAGE", stage.to_string())
            .status().unwrap();
        assert_eq!(status.code(), Some(73));
        let layout = super::super::read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
        if stage == 1 { assert!(layout.decorations.is_empty()); assert_eq!(layout.incomplete_records, 1); }
        else { assert_eq!(layout.decorations.len(), 1); assert_eq!(layout.incomplete_records, 0); }
    }
}
#[test]
fn slice4b_publication_uses_held_directory_not_reopened_display_path() {
    let d = new_decoration(); let f = Fixture::new(&d);
    let original = f.0.join("original"); let moved = f.0.join("moved");
    std::fs::create_dir(&original).unwrap();
    let e = Explorer::new(&original).unwrap();
    std::fs::rename(&original, &moved).unwrap(); std::fs::create_dir(&original).unwrap();
    std::fs::write(original.join("sentinel"), b"replacement namespace").unwrap();
    assert_eq!(place_decoration(&e, &d, &mut |_| Ok(()), |_| Ok(())), Outcome::Committed { synced: true });
    assert!(!original.join(ROOT).exists());
    assert_eq!(std::fs::read(original.join("sentinel")).unwrap(), b"replacement namespace");
    assert_eq!(super::super::read_layout(&Explorer::new(&moved).unwrap()).unwrap().decorations, vec![d]);
}

#[test]
fn slice4b2_final_validator_edit_is_checked_through_the_held_record() {
    let d = new_decoration(); let f = Fixture::new(&d); let e = Explorer::new(&f.0).unwrap();
    let pending = f.0.join(ROOT).join(format!("dc-{}", d.placement_id)).join("pending.toml");
    let mut calls = 0;
    let changed = toml::to_string(&d).unwrap().replace("yaw = 0.5", "yaw = 0.6");
    let out = place_decoration(&e, &d, &mut |_| {
        calls += 1;
        if calls == 2 { std::fs::write(&pending, changed.as_bytes()).unwrap(); }
        Ok(())
    }, |_| Ok(()));
    assert_eq!(out, Outcome::NotCommitted("Prepared decoration metadata changed".into()));
    assert_eq!(std::fs::read(&pending).unwrap(), changed.as_bytes());
    assert!(!pending.with_file_name("decoration.toml").exists());
}

#[test]
#[ignore = "child-only fresh-process provenance fixture"]
fn slice4b2_origin_child() {
    let root = std::path::PathBuf::from(std::env::var_os("HAIR_SLICE4B2_ROOT").unwrap());
    assert!(root.starts_with(std::path::PathBuf::from(std::env::var_os("HAIR_SLICE4B2_BASE").unwrap())));
    assert!(FRESH_ORIGIN.get().is_none(), "child must start without a baseline");
    let action = std::env::var("HAIR_SLICE4B2_ACTION").unwrap();
    let e = Explorer::new(&root).unwrap();
    if action == "write" {
        assert_eq!(place_decoration(&e, &new_decoration(), &mut |_| Ok(()), |_| Ok(())), Outcome::Committed { synced: true });
        return;
    }
    let slot = std::fs::read_dir(root.join(ROOT)).unwrap()
        .map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
    let path = slot.join(if action == "pending" { "pending.toml" } else { "decoration.toml" });
    let file = File::open(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let attrs = provenance(&file).unwrap();
    println!("{action}: record provenance before initialization: {attrs:?}; foreign mode alone uses explicit synthetic tagging");
    let metadata = file.metadata().unwrap();
    if action == "commit" {
        // First storage operation in this fresh process is a commit, not a read
        // that could accidentally prime the baseline for the writer.
        assert_eq!(place_decoration(&e, &new_decoration(), &mut |live| {
            assert_eq!(live.decorations.len(), 1); Ok(())
        }, |_| Ok(())), Outcome::Committed { synced: true });
    }
    let layout = super::super::read_layout(&e);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(provenance(&file).unwrap(), attrs);
    assert_eq!(file.metadata().unwrap().ino(), metadata.ino());
    assert_eq!(file.metadata().unwrap().mtime(), metadata.mtime());
    assert_eq!(file.metadata().unwrap().ctime(), metadata.ctime());
    assert_eq!(file.metadata().unwrap().mtime_nsec(), metadata.mtime_nsec());
    assert_eq!(file.metadata().unwrap().ctime_nsec(), metadata.ctime_nsec());
    if action == "overlap" {
        assert!(layout.unwrap_err().contains("overlaps visited directory"));
        assert!(FRESH_ORIGIN.get().is_none());
        return;
    }
    if action == "foreign" {
        assert!(layout.is_err(), "synthetic foreign attributes must refuse: {layout:?}");
        let snapshot = std::fs::read_dir(root.join(ROOT)).unwrap().count();
        assert!(matches!(place_decoration(&e, &new_decoration(), &mut |_| Ok(()), |_| Ok(())), Outcome::NotCommitted(_)));
        assert_eq!(std::fs::read_dir(root.join(ROOT)).unwrap().count(), snapshot);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(provenance(&file).unwrap(), attrs);
        return;
    }
    let layout = layout.expect("fresh-process natural record must reopen");
    assert_eq!(layout.decorations.len(), if action == "pending" { 0 } else if action == "commit" { 2 } else { 1 });
    assert_eq!(layout.incomplete_records, usize::from(action == "pending"));
    // Even when this host supplies no provenance, exercise the cold-start
    // initialization invariant needed before an attributed record can be read.
    assert!(FRESH_ORIGIN.get().is_some(), "decoration read must establish a fresh same-device origin without writing to the visited directory");
    let origin = FRESH_ORIGIN.get().unwrap();
    assert_eq!(origin.dev, metadata.dev());
    assert_eq!(origin.value, attrs);
}

fn origin_child(root: &std::path::Path, action: &str) {
    origin_child_with_scratch(root, action, None);
}
fn origin_child_with_scratch(root: &std::path::Path, action: &str, scratch: Option<&std::path::Path>) {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "placement_store::mac::decoration_tests::slice4b2_origin_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE4B2_ROOT", root).env("HAIR_SLICE4B2_ACTION", action)
        .env("HAIR_SLICE4B2_BASE", std::env::temp_dir());
    if let Some(scratch) = scratch { command.env("TMPDIR", scratch); }
    let output = command.output().unwrap();
    println!("{action}: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "{action}: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn slice4b2_natural_origin_reopens_and_commits_in_fresh_processes() {
    let f = Fixture::new(&new_decoration());
    origin_child(&f.0, "write");
    let root = File::open(&f.0).unwrap();
    let layout = File::open(f.0.join(ROOT)).unwrap();
    let before_root = root.metadata().unwrap();
    let before_layout = layout.metadata().unwrap();
    origin_child(&f.0, "read");
    assert_eq!(root.metadata().unwrap().mtime(), before_root.mtime());
    assert_eq!(root.metadata().unwrap().mtime_nsec(), before_root.mtime_nsec());
    assert_eq!(root.metadata().unwrap().ctime(), before_root.ctime());
    assert_eq!(root.metadata().unwrap().ctime_nsec(), before_root.ctime_nsec());
    assert_eq!(layout.metadata().unwrap().mtime(), before_layout.mtime());
    assert_eq!(layout.metadata().unwrap().mtime_nsec(), before_layout.mtime_nsec());
    assert_eq!(layout.metadata().unwrap().ctime(), before_layout.ctime());
    assert_eq!(layout.metadata().unwrap().ctime_nsec(), before_layout.ctime_nsec());
    origin_child(&f.0, "commit");
}

#[test]
fn slice4b2_synthetic_foreign_attributes_refuse_in_fresh_processes() {
    for attr in [c"org.hair.fixture", PROVENANCE] {
        let f = Fixture::new(&new_decoration());
        origin_child(&f.0, "write");
        let slot = std::fs::read_dir(f.0.join(ROOT)).unwrap()
            .map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
        let file = File::open(slot.join("decoration.toml")).unwrap();
        // Explicit synthetic tagging, NOT observed OS provenance. A real
        // fsetxattr refusal fails this test; never substitute or bypass it.
        let value = b"synthetic-foreign-origin";
        assert_eq!(unsafe { libc::fsetxattr(file.as_raw_fd(), attr.as_ptr(), value.as_ptr().cast(), value.len(), 0, 0) }, 0,
            "synthetic tagging refused: {}", std::io::Error::last_os_error());
        origin_child(&f.0, "foreign");
        let mut actual = [0u8; 64];
        let n = unsafe { libc::fgetxattr(file.as_raw_fd(), attr.as_ptr(), actual.as_mut_ptr().cast(), actual.len(), 0, 0) };
        assert_eq!(n, value.len() as isize);
        assert_eq!(&actual[..n as usize], value);
    }
}

#[test]
fn slice4b2_overlapping_scratch_refuses_without_modifying_visited_directories() {
    for nested in [false, true] {
        let f = Fixture::new(&new_decoration());
        origin_child(&f.0, "write");
        let scratch = if nested {
            let path = f.0.join("scratch"); std::fs::create_dir(&path).unwrap(); path
        } else { f.0.clone() };
        let before = File::open(&scratch).unwrap().metadata().unwrap();
        let entries = std::fs::read_dir(&scratch).unwrap().count();
        origin_child_with_scratch(&f.0, "overlap", Some(&scratch));
        let after = File::open(&scratch).unwrap().metadata().unwrap();
        assert_eq!((after.mtime(), after.mtime_nsec(), after.ctime(), after.ctime_nsec()),
            (before.mtime(), before.mtime_nsec(), before.ctime(), before.ctime_nsec()));
        assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), entries);
    }
}

#[test]
fn slice4b2_cold_pending_reopen_samples_only_disposable_external_scratch() {
    let f = Fixture::new(&new_decoration());
    let owner = f.0.join("owner"); let scratch = f.0.join("scratch");
    std::fs::create_dir(&owner).unwrap(); std::fs::create_dir(&scratch).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::mac::decoration_tests::slice4b_crash_child", "--ignored"])
        .env("HAIR_SLICE4B_CRASH_ROOT", &owner).env("HAIR_SLICE4B_CRASH_STAGE", "1")
        .status().unwrap();
    assert_eq!(status.code(), Some(73));
    let before = File::open(&owner).unwrap().metadata().unwrap();
    let layout_before = File::open(owner.join(ROOT)).unwrap().metadata().unwrap();
    origin_child_with_scratch(&owner, "pending", Some(&scratch));
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0, "successful sampling must remove only its owned scratch entry");
    let after = File::open(&owner).unwrap().metadata().unwrap();
    let layout_after = File::open(owner.join(ROOT)).unwrap().metadata().unwrap();
    assert_eq!((after.mtime(), after.mtime_nsec(), after.ctime(), after.ctime_nsec()),
        (before.mtime(), before.mtime_nsec(), before.ctime(), before.ctime_nsec()));
    assert_eq!((layout_after.mtime(), layout_after.mtime_nsec(), layout_after.ctime(), layout_after.ctime_nsec()),
        (layout_before.mtime(), layout_before.mtime_nsec(), layout_before.ctime(), layout_before.ctime_nsec()));
}
