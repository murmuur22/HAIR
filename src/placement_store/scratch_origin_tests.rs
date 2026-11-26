// Cold-process tests exercise real disposable filesystem state; no global env races.
use std::path::Path;
use std::os::unix::fs::PermissionsExt;

fn origin_snapshot(path: &Path) -> Vec<(std::path::PathBuf, Vec<u8>, [u64; 10])> {
    fn walk(path: &Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>, [u64; 10])>) {
        let m = std::fs::symlink_metadata(path).unwrap();
        let bytes = if m.is_symlink() {
            std::fs::read_link(path).unwrap().as_os_str().as_bytes().to_vec()
        } else if m.is_file() { std::fs::read(path).unwrap() } else { Vec::new() };
        out.push((path.to_owned(), bytes, [m.dev(), m.ino(), m.uid() as u64, m.gid() as u64,
            m.mode() as u64, m.nlink(), m.mtime() as u64, m.mtime_nsec() as u64,
            m.ctime() as u64, m.ctime_nsec() as u64]));
        if m.is_dir() {
            let mut paths: Vec<_> = std::fs::read_dir(path).unwrap().map(|e| e.unwrap().path()).collect();
            paths.sort();
            for child in paths { walk(&child, out); }
        }
    }
    let mut out = Vec::new(); walk(path, &mut out); out
}
fn scratch_child(owner: &Path, scratch: &Path, expected: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::mac::decoration_tests::slice4b4_scratch_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE4B4_OWNER", owner).env("TMPDIR", scratch)
        .env("HAIR_SLICE4B4_EXPECT", expected).output().unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
#[test]
#[ignore = "child-only cold scratch-origin fixture"]
fn slice4b4_scratch_child() {
    assert!(FRESH_ORIGIN.get().is_none());
    let owner = std::path::PathBuf::from(std::env::var_os("HAIR_SLICE4B4_OWNER").unwrap());
    let expected = std::env::var("HAIR_SLICE4B4_EXPECT").unwrap();
    let e = Explorer::new(&owner).unwrap();
    if expected == "Unsupported different-device placement origin" {
        let file = File::open(owner.join("sentinel")).unwrap();
        // Explicit mismatched-device seam: not an observed second filesystem.
        assert!(FRESH_ORIGIN.set(FreshOrigin { dev: file.metadata().unwrap().dev() ^ 1, value: None }).is_ok());
        assert!(clean_attributes(&file, false).unwrap_err().contains(&expected));
        assert!(remember_fresh_origin(&file).unwrap_err().contains(&expected));
    }
    for _ in 0..3 {
        let result = super::super::read_layout(&e);
        if expected == "ok" {
            assert_eq!(result.unwrap().decorations.len(), 1);
            assert!(FRESH_ORIGIN.get().is_some());
        } else {
            let error = result.unwrap_err();
            println!("scratch refusal: {error}");
            assert!(error.contains(&expected), "{error}");
            assert_eq!(FRESH_ORIGIN.get().is_some(), expected == "Unsupported different-device placement origin");
        }
    }
}
fn scratch_fixture() -> (Fixture, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let f = Fixture::new(&new_decoration());
    let owner = f.0.join("owner");
    let ancestor = f.0.join("ancestor");
    let scratch = ancestor.join("scratch");
    for path in [&owner, &ancestor, &scratch] {
        std::fs::create_dir(path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(owner.join("sentinel"), b"owner untouched").unwrap();
    std::fs::write(scratch.join("sentinel"), b"scratch untouched").unwrap();
    origin_child(&owner, "write");
    (f, owner, ancestor, scratch)
}
fn ancestor_acl(path: &Path, entry: &str) {
    let output = std::process::Command::new("/bin/chmod").args(["+a", entry]).arg(path).output().unwrap();
    assert!(output.status.success(), "ACL fixture operation refused; stop: {}", String::from_utf8_lossy(&output.stderr));
}
#[test]
fn slice4b4_unsafe_writable_ancestors_refuse_without_probe_or_owner_changes() {
    for mode in [0o770, 0o707, 0o1777] {
        let (_f, owner, ancestor, scratch) = scratch_fixture();
        std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(mode)).unwrap();
        let before = origin_snapshot(&_f.0);
        scratch_child(&owner, &scratch, "Untrusted provenance scratch ancestor");
        assert_eq!(origin_snapshot(&_f.0), before, "mode {mode:o}");
    }
}
#[test]
fn slice4b4_write_granting_ancestor_acl_refuses_without_probe_or_owner_changes() {
    let (f, owner, ancestor, scratch) = scratch_fixture();
    ancestor_acl(&ancestor, "group:everyone allow add_file,add_subdirectory,delete_child");
    let before = origin_snapshot(&f.0);
    scratch_child(&owner, &scratch, "Untrusted provenance scratch ancestor");
    assert_eq!(origin_snapshot(&f.0), before);
}
#[test]
fn slice4b4_safe_chain_including_live_restrictive_acl_allows_same_device_sample() {
    // The RED deny-delete fixture hit EACCES during deletion and is retained
    // unchanged (receipt). Do not retry deletion or change that ACL. Inspect the
    // existing home ACL read-only instead; never create another deletion denial.
    let home = File::open(std::env::var_os("HOME").unwrap()).unwrap();
    println!("live home has extended ACL: {}", !no_extended_acl(&home).unwrap());
    assert!(restrictive_ancestor_acl(&home).unwrap());
    let (_f, owner, _ancestor, scratch) = scratch_fixture();
    let before_owner = origin_snapshot(&owner);
    let sentinel = origin_snapshot(&scratch.join("sentinel"));
    scratch_child(&owner, &scratch, "ok");
    assert_eq!(origin_snapshot(&owner), before_owner);
    assert_eq!(origin_snapshot(&scratch.join("sentinel")), sentinel);
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
}
#[test]
fn slice4b4_symlink_leaf_with_or_without_slash_refuses_untouched() {
    for suffix in ["", "/", "///"] {
        let (f, owner, _ancestor, scratch) = scratch_fixture();
        let link = f.0.join("link");
        std::os::unix::fs::symlink(&scratch, &link).unwrap();
        let mut raw = link.as_os_str().to_os_string(); raw.push(suffix);
        let before = origin_snapshot(&f.0);
        scratch_child(&owner, Path::new(&raw), "Open private provenance sampling directory");
        assert_eq!(origin_snapshot(&f.0), before);
    }
}

#[test]
fn slice4b4_mismatched_device_seam_reports_limitation_before_scratch_or_foreign_metadata() {
    let (f, owner, _ancestor, scratch) = scratch_fixture();
    let before = origin_snapshot(&f.0);
    scratch_child(&owner, &scratch.join("nonexistent"), "Unsupported different-device placement origin");
    assert_eq!(origin_snapshot(&f.0), before);
    let dev = File::open(&owner).unwrap().metadata().unwrap().dev();
    let origin = FreshOrigin { dev, value: None };
    assert!(!supported_origin_device(dev, None).unwrap());
    assert!(supported_origin_device(dev, Some(&origin)).unwrap());
    let error = supported_origin_device(dev ^ 1, Some(&origin)).unwrap_err();
    assert!(error.contains("restart HAIR") && error.contains("one device"));
    println!("synthetic mismatched-device seam only; no second filesystem observed: {error}");
}

#[test]
fn slice4b4_ancestor_flags_are_explicit_and_root_sticky_is_not_an_euid_exception() {
    for (flags, expected) in [(libc::UF_HIDDEN, "ok"), (libc::UF_NODUMP, "Untrusted provenance scratch ancestor")] {
        let (f, owner, ancestor, scratch) = scratch_fixture();
        let file = File::open(&ancestor).unwrap();
        assert_eq!(unsafe { libc::fchflags(file.as_raw_fd(), flags) }, 0, "fixture flags refused; stop: {}", std::io::Error::last_os_error());
        let before = origin_snapshot(&f.0);
        let owner_before = origin_snapshot(&owner);
        scratch_child(&owner, &scratch, expected);
        assert_eq!(stat_name(&file, OsStr::new(".")).unwrap().unwrap().st_flags, flags);
        assert_eq!(origin_snapshot(&owner), owner_before);
        if expected != "ok" { assert_eq!(origin_snapshot(&f.0), before); }
    }
    // Real root-owned sticky ancestor, inspected read-only; no new mount/chown.
    let sticky = File::open("/private/tmp").unwrap();
    let st = stat_name(&sticky, OsStr::new(".")).unwrap().unwrap();
    assert_eq!(st.st_uid, 0);
    assert_ne!(st.st_mode & libc::S_ISVTX, 0);
    assert!(trusted_ancestor_shape(&st, unsafe { libc::geteuid() }));
    assert!(restrictive_ancestor_acl(&sticky).unwrap());
    let mut synthetic = st;
    synthetic.st_uid = 12345;
    assert!(!trusted_ancestor_shape(&synthetic, 12345), "euid-owned sticky writable is not trusted");
    synthetic.st_mode &= !0o022;
    assert!(!trusted_ancestor_shape(&synthetic, 54321), "synthetic foreign owner is not trusted");
    synthetic.st_uid = 0;
    synthetic.st_flags = libc::UF_NODUMP;
    assert!(!trusted_ancestor_shape(&synthetic, 54321), "root does not bypass flags");
}

#[test]
fn slice4b4_dot_traversal_is_not_normalized_and_real_trailing_slash_is_supported() {
    let (f, owner, _ancestor, scratch) = scratch_fixture();
    for suffix in ["/.", "/../scratch", "/../scratch/"] {
        let mut path = scratch.as_os_str().to_os_string(); path.push(suffix);
        let before = origin_snapshot(&f.0);
        scratch_child(&owner, Path::new(&path), "unsafe scratch path");
        assert_eq!(origin_snapshot(&f.0), before);
    }
    let mut path = scratch.as_os_str().to_os_string(); path.push("///");
    let owner_before = origin_snapshot(&owner);
    scratch_child(&owner, Path::new(&path), "ok");
    assert_eq!(origin_snapshot(&owner), owner_before);
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
}

#[test]
fn slice4b4_bounded_actual_ancestry_refuses_without_probe() {
    let (f, owner, _ancestor, mut scratch) = scratch_fixture();
    let mut owned_chain = Vec::new();
    for _ in 0..256 {
        scratch.push("d");
        std::fs::create_dir(&scratch).unwrap();
        owned_chain.push(scratch.clone());
    }
    let before = origin_snapshot(&f.0);
    scratch_child(&owner, &scratch, "scratch ancestry limit");
    let after = origin_snapshot(&f.0);
    // remove_dir_all can hold one descriptor per level and exhaust the default
    // macOS limit on this deliberate 256-level fixture. Remove only these known
    // empty owned directories leaf-first, preserving the full assertion above.
    for path in owned_chain.iter().rev() {
        std::fs::remove_dir(path).unwrap();
    }
    assert_eq!(after, before);
}

#[test]
fn slice4b4_owned_probe_cleanup_covers_repeated_read_and_device_faults() {
    let (_f, owner, _ancestor, scratch) = scratch_fixture();
    let dir = File::open(&scratch).unwrap();
    let dev = dir.metadata().unwrap().dev();
    let owner_before = origin_snapshot(&owner);
    let sentinel = origin_snapshot(&scratch.join("sentinel"));
    for _ in 0..4 {
        let error = sample_fresh_origin(&dir, dev, |_, _, _| Err("injected provenance read refusal (not OS errno)".into())).err().unwrap();
        assert_eq!(error, "injected provenance read refusal (not OS errno)");
        assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
    }
    let error = sample_fresh_origin(&dir, dev ^ 1, |_, _, file| provenance(file)).err().unwrap();
    assert_eq!(error, "Fresh provenance sample crossed device");
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
    assert_eq!(origin_snapshot(&owner), owner_before);
    assert_eq!(origin_snapshot(&scratch.join("sentinel")), sentinel);
}

#[test]
fn slice4b4_probe_cleanup_preserves_rebound_or_uncertain_names() {
    for fault in ["rebound", "hardlinked", "shape"] {
        let (_f, _owner, _ancestor, scratch) = scratch_fixture();
        let dir = File::open(&scratch).unwrap();
        let dev = dir.metadata().unwrap().dev();
        let mut retained = None;
        let error = sample_fresh_origin(&dir, dev, |_, name, file| {
            let path = scratch.join(name);
            match fault {
                "rebound" => {
                    std::fs::rename(&path, scratch.join("displaced")).unwrap();
                    std::fs::write(&path, b"foreign replacement").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                }
                "hardlinked" => { std::fs::hard_link(&path, scratch.join("alias")).unwrap(); }
                "shape" => { file.set_permissions(std::fs::Permissions::from_mode(0o400)).unwrap(); }
                _ => unreachable!(),
            }
            retained = Some((path.clone(), origin_snapshot(&path)));
            Err("injected inspection failure (not OS errno)".into())
        }).err().unwrap();
        assert!(error.contains("injected inspection failure") && error.contains("entry preserved"), "{error}");
        let (path, before) = retained.unwrap();
        assert_eq!(origin_snapshot(&path), before, "{fault}");
        if fault == "rebound" {
            assert_eq!(std::fs::read(&path).unwrap(), b"foreign replacement");
            assert!(scratch.join("displaced").exists());
        }
        if fault == "hardlinked" { assert_eq!(std::fs::metadata(&path).unwrap().nlink(), 2); }
    }
}
