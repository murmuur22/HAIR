// M1 reproduction uses only disposable owned fixtures and authentic pack bytes.
fn slice6a1_child(root: &std::path::Path, action: &str, incomplete: usize, pins: usize) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::slice6a1_process_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE6A1_ROOT", root).env("HAIR_SLICE6A1_ACTION", action)
        .env("HAIR_SLICE6A1_INCOMPLETE", incomplete.to_string())
        .env("HAIR_SLICE6A1_PINS", pins.to_string()).output().unwrap()
}
#[test]
#[ignore = "child-only exact partial-write / abrupt termination / cold read"]
fn slice6a1_process_child() {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE6A1_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let action = std::env::var("HAIR_SLICE6A1_ACTION").unwrap();
    let e = Explorer::new(&root).unwrap();
    if action == "reject" {
        assert!(read_layout(&e).is_err(), "foreign or active corrupt data must fail closed");
        return;
    }
    if action == "read" {
        let layout = read_layout(&e).expect("interrupted owned preparation must not hide prior saved layout");
        assert_eq!(layout.positions, vec![("saved.txt".into(), pos())]);
        assert_eq!(layout.decorations.iter().filter(|d| !d.is_pinned()).count(), 1);
        assert_eq!(layout.decorations.iter().filter(|d| d.is_pinned()).count(), std::env::var("HAIR_SLICE6A1_PINS").unwrap().parse::<usize>().unwrap());
        assert_eq!(layout.incomplete_records, std::env::var("HAIR_SLICE6A1_INCOMPLETE").unwrap().parse::<usize>().unwrap());
        assert!(layout.warnings.is_empty());
        return;
    }
    let theme = crate::theme::Theme::load(&root.join("pack/pack.toml"), &mut Default::default()).unwrap();
    let source = select_pin_asset(&theme, "chair").unwrap();
    let d = slice6a_new(&theme);
    let prefix = action.strip_prefix("legacy-").map(|s| s.parse::<usize>().unwrap());
    let stage = if prefix.is_some() { 3 } else { action.parse::<u8>().unwrap() };
    let descriptor = source.descriptor.text().unwrap();
    let _ = mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |at| {
        if at == stage {
            if let Some(prefix) = prefix {
                // Exact pre-fix O_EXCL create + deterministic descriptor write prefix.
                // The real writer prepared the real asset and owned closure directory.
                let slot = root.join(".hair-layout").join(format!("ac-{}", source.closure_identity().unwrap()));
                let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).mode(0o600)
                    .open(slot.join("closure.toml")).unwrap();
                file.write_all(&descriptor.as_bytes()[..prefix]).unwrap();
            }
            unsafe { libc::_exit(73) };
        }
        Ok(())
    });
    panic!("child did not reach requested crash boundary");
}
fn slice6a1_saved() -> (Fixture, Explorer, crate::theme::Theme) {
    let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    assert_eq!(publish(&e, OsStr::new("saved.txt"), b"prior owned file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
    assert_eq!(place_decoration_checked(&e, &slice5a_decoration([2., 0.]), |_| Ok(())), Outcome::Committed { synced: true });
    (f, e, theme)
}
#[test]
fn slice6a1_interrupted_descriptor_preparation_preserves_cold_layout() {
    let mut failures = vec![];
    for stage in [4, 5] {
        let (f, _, _) = slice6a1_saved();
        let old = slice5a_snapshot(&f.0);
        let crash = slice6a1_child(&f.0, &stage.to_string(), 1, 0);
        assert_eq!(crash.status.code(), Some(73), "{}{}", String::from_utf8_lossy(&crash.stdout), String::from_utf8_lossy(&crash.stderr));
        let before_read = slice5a_snapshot(&f.0);
        let read = slice6a1_child(&f.0, "read", 1, 0);
        if !read.status.success() {
            failures.push(stage);
            println!("SLICE6A1 interrupted stage={stage}: {}{}", String::from_utf8_lossy(&read.stdout), String::from_utf8_lossy(&read.stderr));
        }
        assert_eq!(slice5a_snapshot(&f.0), before_read, "cold read must preserve all evidence");
        for entry in old { assert!(before_read.contains(&entry), "prior bytes/inode changed: {}", entry.0.display()); }
    }
    assert!(failures.is_empty(), "interrupted descriptor stages hide saved layout: {failures:?}");
}

#[test]
fn slice6a1_actual_termination_all_preparation_and_activation_boundaries() {
    for stage in [8, 9, 3, 4, 5, 6, 7, 1, 2] {
        let (f, e, theme) = slice6a1_saved();
        let old_layout = read_layout(&e).unwrap();
        let old = slice5a_snapshot(&f.0);
        let incomplete = if stage == 1 { 2 } else { usize::from(stage != 2) };
        let pins = usize::from(stage == 2);
        let crash = slice6a1_child(&f.0, &stage.to_string(), incomplete, pins);
        assert_eq!(crash.status.code(), Some(73), "stage {stage}: {}{}", String::from_utf8_lossy(&crash.stdout), String::from_utf8_lossy(&crash.stderr));
        let closure = slice6a_closure(&f);
        let asset = std::fs::read(closure.join("asset.glb")).unwrap();
        let source = std::fs::read(f.0.join("pack/chair.glb")).unwrap();
        assert_eq!(asset.as_slice(), if stage == 8 { &source[..0] } else if stage == 9 { &source[..11] } else { source.as_slice() });
        let descriptor = select_pin_asset(&theme, "chair").unwrap().descriptor.text().unwrap();
        assert_eq!(closure.join("closure.pending.toml").exists(), [4, 5, 6].contains(&stage));
        assert_eq!(closure.join("closure.toml").exists(), [7, 1, 2].contains(&stage));
        if [4, 5, 6].contains(&stage) {
            let bytes = std::fs::read(closure.join("closure.pending.toml")).unwrap();
            assert_eq!(bytes, &descriptor.as_bytes()[..if stage == 4 { 0 } else if stage == 5 { 11 } else { descriptor.len() }]);
        }
        if [7, 1, 2].contains(&stage) { assert_eq!(std::fs::read(closure.join("closure.toml")).unwrap(), descriptor.as_bytes()); }
        let before_read = slice5a_snapshot(&f.0);
        let read = slice6a1_child(&f.0, "read", incomplete, pins);
        assert!(read.status.success(), "stage {stage}: {}{}", String::from_utf8_lossy(&read.stdout), String::from_utf8_lossy(&read.stderr));
        assert_eq!(slice5a_snapshot(&f.0), before_read);
        for entry in old { assert!(before_read.contains(&entry), "stage {stage} prior bytes/inode changed: {}", entry.0.display()); }
        let live = read_layout(&e).unwrap();
        assert_eq!(live.positions, old_layout.positions);
        assert!(live.decorations.contains(&old_layout.decorations[0]));
        assert_eq!(live.incomplete_records, incomplete);
        if stage != 2 {
            let out = pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| panic!("incomplete layout reached validator"));
            assert!(matches!(out, Outcome::NotCommitted(_)), "stage {stage}: {out:?}");
            assert_eq!(slice5a_snapshot(&f.0), before_read, "no automatic retry, repair or deletion");
        }
        println!("SLICE6A1 crash stage={stage} exit=73 fresh_read=PASS incomplete={incomplete} pins={pins} prior_bytes_inodes=exact");
    }
}

#[test]
fn slice6a1_preparation_failure_outcomes_remain_truthful() {
    for stage in [8, 9, 3, 4, 5, 6, 7, 1, 2] {
        let (f, e, theme) = slice6a1_saved();
        let source = select_pin_asset(&theme, "chair").unwrap();
        let records = slice4c1_record_snapshot(&f.0);
        let out = mac::publish_pin(&e, &source, &slice6a_new(&theme), &mut |_| Ok(()), |at| {
            if at == stage { Err("injected preparation/activation failure".into()) } else { Ok(()) }
        });
        if stage == 2 { assert_eq!(out, Outcome::Committed { synced: false }); }
        else { assert!(matches!(out, Outcome::NotCommitted(_)), "stage {stage}: {out:?}"); }
        let after = slice4c1_record_snapshot(&f.0);
        for record in records { assert!(after.contains(&record)); }
        let snapshot = slice5a_snapshot(&f.0);
        let read = slice6a1_child(&f.0, "read", if stage == 1 { 2 } else { usize::from(stage != 2) }, usize::from(stage == 2));
        assert!(read.status.success(), "stage {stage}: {}{}", String::from_utf8_lossy(&read.stdout), String::from_utf8_lossy(&read.stderr));
        assert_eq!(slice5a_snapshot(&f.0), snapshot);
    }
    // Logical activation's existing definite/uncertain rename classification.
    for code in [libc::EEXIST, libc::EACCES] {
        assert!(matches!(mac::decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::NotCommitted(_)));
    }
    for code in [libc::EIO, libc::EINTR] {
        assert!(matches!(mac::decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::Ambiguous(_)));
    }
}

#[test]
fn slice6a1_legacy_partial_final_and_corrupt_active_descriptors_stay_fail_closed() {
    for prefix in [0, 11] {
        let (f, e, _) = slice6a1_saved();
        let crash = slice6a1_child(&f.0, &format!("legacy-{prefix}"), 1, 0);
        assert_eq!(crash.status.code(), Some(73));
        assert!(read_layout(&e).unwrap_err().contains("Malformed pinned descriptor"));
        let before = slice5a_snapshot(&f.0);
        assert!(slice6a1_child(&f.0, "reject", 0, 0).status.success());
        assert_eq!(slice5a_snapshot(&f.0), before);
    }
    for bytes in [b"".as_slice(), b"version = 1", b"version = ["] {
        let (f, e, theme) = slice6a1_saved();
        assert_eq!(pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(())), Outcome::Committed { synced: true });
        std::fs::write(slice6a_closure(&f).join("closure.toml"), bytes).unwrap();
        let before = slice5a_snapshot(&f.0);
        assert!(read_layout(&e).unwrap_err().contains("Malformed pinned descriptor"));
        assert!(slice6a1_child(&f.0, "reject", 0, 0).status.success());
        assert!(matches!(pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(())), Outcome::NotCommitted(_)));
        assert_eq!(slice5a_snapshot(&f.0), before);
    }
}

#[test]
fn slice6a1_pending_shape_metadata_and_active_references_are_strict() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for kind in ["valid-opaque", "oversize", "symlink", "hardlink", "xattr", "acl", "mode", "slot-xattr", "root-xattr", "extra", "no-asset", "mixed-final", "active-reference"] {
        let (f, e, theme) = slice6a1_saved();
        let source = select_pin_asset(&theme, "chair").unwrap();
        let d = slice6a_new(&theme);
        let out = mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |at| if at == 5 { Err("leave partial descriptor".into()) } else { Ok(()) });
        assert!(matches!(out, Outcome::NotCommitted(_)));
        let closure = slice6a_closure(&f); let pending = closure.join("closure.pending.toml");
        match kind {
            "valid-opaque" => std::fs::write(&pending, [0xff, 0x00, 0xfe]).unwrap(),
            "oversize" => std::fs::OpenOptions::new().write(true).open(&pending).unwrap().set_len(4097).unwrap(),
            "symlink" => { std::fs::rename(&pending, f.0.join("evidence")).unwrap(); symlink(f.0.join("evidence"), &pending).unwrap(); }
            "hardlink" => std::fs::hard_link(&pending, f.0.join("evidence")).unwrap(),
            "xattr" => slice6a_tag(&pending), "acl" => slice6a_acl(&pending),
            "mode" => std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o644)).unwrap(),
            "slot-xattr" => slice6a_tag(&closure), "root-xattr" => slice6a_tag(&f.0.join(".hair-layout")),
            "extra" => std::fs::write(closure.join("foreign"), b"preserve").unwrap(),
            "no-asset" => std::fs::rename(closure.join("asset.glb"), f.0.join("evidence")).unwrap(),
            "mixed-final" => { std::fs::rename(closure.join("asset.glb"), f.0.join("evidence")).unwrap(); std::fs::write(closure.join("closure.toml"), source.descriptor.text().unwrap()).unwrap(); }
            "active-reference" => {
                let slot = f.0.join(".hair-layout").join(format!("dc-{}", d.placement_id));
                std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                let path = slot.join("decoration.toml");
                std::fs::write(&path, toml::to_string(&d.pin(&source).unwrap()).unwrap()).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            _ => unreachable!(),
        }
        let before = slice5a_snapshot(&f.0);
        if kind == "valid-opaque" {
            assert_eq!(read_layout(&e).unwrap().incomplete_records, 1);
            assert!(slice6a1_child(&f.0, "read", 1, 0).status.success());
        } else {
            let error = read_layout(&e).unwrap_err();
            if kind == "active-reference" { assert!(error.contains("closure missing/incomplete"), "{error}"); }
            let child = slice6a1_child(&f.0, "reject", 0, 0);
            assert!(child.status.success(), "{kind}: {}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
        }
        assert!(matches!(publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| Ok(())), Outcome::NotCommitted(_)), "{kind}");
        assert_eq!(slice5a_snapshot(&f.0), before, "{kind}: never repair/delete foreign or inactive evidence");
    }
}

#[test]
fn slice6a1_descriptor_publication_revalidates_source_and_owned_bindings_exclusively() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for target in ["source", "source-inode", "pack", "manifest", "pending", "pending-bytes", "pending-symlink", "pending-hardlink", "pending-xattr", "pending-acl", "asset", "closure-dir", "root", "lock", "exclusive-pending", "exclusive-final"] {
        let (f, e, theme) = slice6a1_saved();
        let source = select_pin_asset(&theme, "chair").unwrap(); let d = slice6a_new(&theme);
        let root = f.0.join(".hair-layout");
        let mut evidence = None;
        let out = mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |at| {
            if at != if target == "exclusive-pending" { 3 } else { 6 } { return Ok(()); }
            let closure = slice6a_closure(&f); let pending = closure.join("closure.pending.toml");
            match target {
                "source" => std::fs::write(f.0.join("pack/chair.glb"), b"test stale source").unwrap(),
                "source-inode" => { let p = f.0.join("pack/chair.glb"); let bytes = std::fs::read(&p).unwrap(); std::fs::rename(&p, f.0.join("evidence")).unwrap(); std::fs::write(&p, bytes).unwrap(); }
                "pack" => std::fs::rename(f.0.join("pack"), f.0.join("evidence")).unwrap(),
                "manifest" => std::fs::write(f.0.join("pack/pack.toml"), b"stale manifest").unwrap(),
                "pending-bytes" => std::fs::write(&pending, b"version = 1").unwrap(),
                "pending-hardlink" => std::fs::hard_link(&pending, f.0.join("evidence")).unwrap(),
                "pending-xattr" => slice6a_tag(&pending), "pending-acl" => slice6a_acl(&pending),
                "exclusive-pending" | "exclusive-final" => {
                    let p = closure.join(if target == "exclusive-pending" { "closure.pending.toml" } else { "closure.toml" });
                    std::fs::write(&p, b"foreign bytes must survive").unwrap(); std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
                }
                _ => {
                    let p = match target { "pending" | "pending-symlink" => pending, "asset" => closure.join("asset.glb"), "closure-dir" => closure, "root" => root.clone(), "lock" => root.join("lock"), _ => unreachable!() };
                    let is_dir = p.is_dir(); let bytes = if is_dir { vec![] } else { std::fs::read(&p).unwrap() };
                    std::fs::rename(&p, f.0.join("evidence")).unwrap();
                    if target == "pending-symlink" { symlink(f.0.join("evidence"), &p).unwrap(); }
                    else if is_dir { std::fs::create_dir(&p).unwrap(); std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap(); }
                    else { std::fs::write(&p, bytes).unwrap(); std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap(); }
                }
            }
            evidence = Some(slice5a_snapshot(&f.0));
            Ok(())
        });
        assert!(matches!(out, Outcome::NotCommitted(_)), "{target}: {out:?}");
        assert_eq!(slice5a_snapshot(&f.0), evidence.expect("must reach publication boundary"), "{target}: no overwrite/activation/cleanup after refusal");
        assert!(!root.join(format!("dc-{}", d.placement_id)).exists(), "{target}");
    }
}
