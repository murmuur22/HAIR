// Read-only inspection controls: all mutations below target disposable fixtures.
fn slice8a_semantics(i: &PlacementInspection) -> String {
    format!("{:?}|{:?}|{:?}|{}|{:?}|{:?}", i.layout().positions, i.layout().decorations,
        i.layout().retained_slots, i.layout().incomplete_records, i.layout().warnings, i.attempts())
}
#[derive(Debug, PartialEq)]
struct Slice8aSnapshot {
    files: Vec<(PathBuf, u64, u32, Vec<u8>)>,
    // Exact per-path xattr name/value evidence; retain the fixture's comparison shape.
    #[allow(clippy::type_complexity)]
    attrs: Vec<(PathBuf, Vec<(Vec<u8>, Vec<u8>)>)>,
}
fn slice8a_snapshot(root: &std::path::Path) -> Slice8aSnapshot {
    let files = slice5a_snapshot(root);
    let mut attrs = vec![];
    for (path, _, _, _) in &files {
        if std::fs::symlink_metadata(path).unwrap().is_symlink() { continue; }
        let f = std::fs::File::open(path).unwrap();
        let mut names = [0u8; 4096];
        let n = unsafe { libc::flistxattr(f.as_raw_fd(), names.as_mut_ptr().cast(), names.len(), 0) };
        assert!(n >= 0);
        let mut values = vec![];
        for name in names[..n as usize].split(|b| *b == 0).filter(|s| !s.is_empty()) {
            let c = std::ffi::CString::new(name).unwrap();
            let mut bytes = [0u8; 4096];
            let n = unsafe { libc::fgetxattr(f.as_raw_fd(), c.as_ptr(), bytes.as_mut_ptr().cast(), bytes.len(), 0, 0) };
            assert!(n >= 0);
            values.push((name.to_vec(), bytes[..n as usize].to_vec()));
        }
        values.sort(); attrs.push((path.clone(), values));
    }
    Slice8aSnapshot { files, attrs }
}
fn slice8a_cold(root: &std::path::Path, expected: &PlacementInspection) {
    let before = slice8a_snapshot(root);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::slice8a_cold_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE8A_ROOT", root).env("HAIR_SLICE8A_EXPECTED", slice8a_semantics(expected)).output().unwrap();
    assert!(child.status.success(), "cold inspection failed: {}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
    assert_eq!(slice8a_snapshot(root), before, "cold inspection changed bytes/inodes/modes/xattrs");
}
#[test]
#[ignore = "child-only fresh-process read-only inspection"]
fn slice8a_cold_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8A_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let e = Explorer::new(&root).unwrap();
    let before = slice8a_snapshot(&root);
    let i = inspect_placements(&e).unwrap();
    assert_eq!(slice8a_semantics(&i), std::env::var("HAIR_SLICE8A_EXPECTED").unwrap());
    validate_inspection(&e, &i).unwrap();
    assert_eq!(slice8a_snapshot(&root), before);
}
fn slice8a_file_crash(root: &std::path::Path, stage: u8) {
    use std::os::unix::process::ExitStatusExt;
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::crash_child", "--ignored", "--nocapture"])
        .env("HAIR_PLACEMENT_CRASH_ROOT", root).env("HAIR_PLACEMENT_CRASH_STAGE", stage.to_string()).output().unwrap();
    assert_eq!(child.status.signal(), Some(libc::SIGKILL), "{}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
}
fn slice8a_tx(f: &Fixture) -> PathBuf {
    std::fs::read_dir(f.0.join(".hair-layout")).unwrap().map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_str().unwrap().starts_with("tx-")).unwrap()
}
#[test]
fn slice8a_new_decoration_real_crash_and_committed_controls_cold() {
    for stage in [1, 2] {
        let f = Fixture::new();
        std::fs::write(f.0.join("original.txt"), b"owner original").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "placement_store::mac::decoration_tests::slice4b_crash_child", "--ignored", "--nocapture"])
            .env("HAIR_SLICE4B_CRASH_ROOT", &f.0).env("HAIR_SLICE4B_CRASH_STAGE", stage.to_string()).output().unwrap();
        assert_eq!(output.status.code(), Some(73), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let before = slice8a_snapshot(&f.0);
        let e = Explorer::new(&f.0).unwrap(); let i = inspect_placements(&e).unwrap();
        assert_eq!(i.attempts().len(), 1);
        let a = &i.attempts()[0];
        assert_eq!(a.state(), if stage == 1 { AttemptState::DefinitelyInactive } else { AttemptState::Published });
        assert_eq!(a.kind(), if stage == 1 { AttemptKind::NewDecoration } else { AttemptKind::Decoration });
        assert_eq!(a.active_predecessor(), Some(false));
        let slot = f.0.join(".hair-layout").join(a.identity());
        assert!(slot.is_dir());
        assert_eq!(slot.join("pending.toml").exists(), stage == 1);
        assert_eq!(i.layout(), &read_layout(&e).unwrap());
        assert_eq!(slice8a_snapshot(&f.0), before);
        validate_inspection(&e, &i).unwrap(); slice8a_cold(&f.0, &i);
        println!("SLICE8A new-decoration stage={stage} exit=73 state={:?} exact-local-identity={} unchanged=bytes,inodes,modes,xattrs cold=PASS", a.state(), a.identity());
    }
}
#[test]
fn slice8a_pin_all_actual_partial_pending_finalized_and_committed_shapes_cold() {
    for stage in [8, 9, 3, 4, 5, 6, 7, 1, 2] {
        let (f, e, _) = slice6a1_saved();
        let prior = read_layout(&e).unwrap();
        let crash = slice6a1_child(&f.0, &stage.to_string(), 0, 0);
        assert_eq!(crash.status.code(), Some(73), "stage {stage}: {}{}", String::from_utf8_lossy(&crash.stdout), String::from_utf8_lossy(&crash.stderr));
        let closure = slice6a_closure(&f);
        let pending = closure.join("closure.pending.toml");
        if stage == 4 { assert_eq!(std::fs::metadata(&pending).unwrap().len(), 0); }
        if stage == 5 { assert_eq!(std::fs::metadata(&pending).unwrap().len(), 11); }
        assert_eq!(pending.exists(), [4, 5, 6].contains(&stage));
        assert_eq!(closure.join("closure.toml").exists(), [7, 1, 2].contains(&stage));
        let before = slice8a_snapshot(&f.0);
        let i = inspect_placements(&e).unwrap();
        let a = i.attempts().iter().find(|a| a.kind() == AttemptKind::PinClosure).unwrap();
        assert_eq!(a.identity(), closure.file_name().unwrap().to_str().unwrap());
        assert_eq!(a.state(), if stage == 2 { AttemptState::Published } else { AttemptState::DefinitelyInactive });
        assert_eq!(i.layout().positions, prior.positions);
        assert!(i.layout().decorations.contains(&prior.decorations[0]));
        assert_eq!(i.layout().incomplete_records, if stage == 1 { 2 } else { usize::from(stage != 2) });
        assert_eq!(slice8a_snapshot(&f.0), before);
        validate_inspection(&e, &i).unwrap(); slice8a_cold(&f.0, &i);
        println!("SLICE8A pin stage={stage} exit=73 incomplete={} closure={:?} unchanged=bytes,inodes,modes,xattrs cold=PASS", i.layout().incomplete_records, a.state());
    }
}
#[test]
fn slice8a_file_actual_sigkill_stages_and_non_actionable_history_cold() {
    for stage in [1, 2, 3, 4] {
        let f = Fixture::new();
        std::fs::write(f.0.join("original.txt"), b"keep original").unwrap();
        slice8a_file_crash(&f.0, stage);
        let before = slice8a_snapshot(&f.0);
        let e = Explorer::new(&f.0).unwrap(); let i = inspect_placements(&e).unwrap();
        assert_eq!(i.attempts().len(), 1);
        let a = &i.attempts()[0];
        assert_eq!(a.identity(), slice8a_tx(&f).file_name().unwrap().to_str().unwrap());
        assert_eq!(a.kind(), AttemptKind::FileTransaction);
        assert_eq!(a.state(), match stage { 1 | 2 => AttemptState::Unsupported, 3 => AttemptState::DefinitelyInactive, 4 => AttemptState::Published, _ => unreachable!() });
        assert_eq!(i.layout(), &read_layout(&e).unwrap());
        assert_eq!(slice8a_snapshot(&f.0), before); validate_inspection(&e, &i).unwrap(); slice8a_cold(&f.0, &i);
        println!("SLICE8A file stage={stage} signal=SIGKILL state={:?} unchanged=bytes,inodes,modes,xattrs cold=PASS", a.state());
        if stage == 4 {
            std::fs::create_dir(f.0.join("away")).unwrap();
            std::fs::rename(f.0.join("Final"), f.0.join("away/Final")).unwrap();
            let before = slice8a_snapshot(&f.0); let dormant = inspect_placements(&e).unwrap();
            assert_eq!(dormant.attempts()[0].state(), AttemptState::Ambiguous);
            slice8a_cold(&f.0, &dormant); assert_eq!(slice8a_snapshot(&f.0), before);
        }
    }
}
#[test]
fn slice8a_file_reposition_copy_token_and_malformed_evidence_never_gain_authority() {
    for kind in ["reposition", "copy-token", "malformed", "malformed-active", "schema-active", "hardlink-live"] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
        if kind == "reposition" {
            std::fs::write(f.0.join("original.txt"), b"unchanged").unwrap();
            let e = Explorer::new(&f.0).unwrap(); let selected = select_existing(&e, OsStr::new("original.txt")).unwrap();
            let mut reached = false;
            let out = mac::reposition(&e, &selected, &pos(), &mut |_| Ok(()), |at| {
                if at == 1 { reached = true; Err("demonstrated pre-activation refusal".into()) } else { Ok(()) }
            });
            assert!(reached); assert_eq!(out, Outcome::NotCommitted("demonstrated pre-activation refusal".into()));
        } else {
            slice8a_file_crash(&f.0, if ["malformed-active", "schema-active", "hardlink-live"].contains(&kind) { 4 } else { 3 });
            let record = slice8a_tx(&f).join("record.toml");
            match kind {
                "copy-token" => {
                    let payload = std::fs::File::open(slice8a_tx(&f).join("payload")).unwrap();
                    std::fs::write(f.0.join("copied.txt"), b"not original inode").unwrap();
                    let copy = std::fs::File::open(f.0.join("copied.txt")).unwrap();
                    let mut token = [0u8; 16];
                    assert_eq!(unsafe { libc::fgetxattr(payload.as_raw_fd(), c"org.hair.placement-token-v1".as_ptr(), token.as_mut_ptr().cast(), token.len(), 0, 0) }, 16);
                    assert_eq!(unsafe { libc::fsetxattr(copy.as_raw_fd(), c"org.hair.placement-token-v1".as_ptr(), token.as_ptr().cast(), token.len(), 0, libc::XATTR_CREATE) }, 0);
                }
                "malformed" | "malformed-active" => std::fs::write(record, b"version = [").unwrap(),
                "schema-active" => {
                    let text = std::fs::read_to_string(&record).unwrap().replace("version = 2", "version = 99");
                    std::fs::write(record, text).unwrap();
                }
                "hardlink-live" => std::fs::hard_link(f.0.join("Final"), f.0.join("alias")).unwrap(),
                _ => unreachable!(),
            }
        }
        let before = slice8a_snapshot(&f.0);
        let result = inspect_placements(&e);
        if ["malformed-active", "schema-active"].contains(&kind) { assert!(result.is_err(), "{kind} must fail closed"); }
        else {
            let i = result.unwrap();
            assert_eq!(i.attempts()[0].state(), if kind == "malformed" { AttemptState::Unsupported } else { AttemptState::Ambiguous }, "{kind}");
            slice8a_cold(&f.0, &i);
        }
        assert_eq!(slice8a_snapshot(&f.0), before, "{kind}");
    }
}
#[test]
fn slice8a_missing_root_empty_slots_opaque_pending_and_descriptor_only_are_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    let before = slice8a_snapshot(&f.0); let empty = inspect_placements(&e).unwrap();
    assert!(empty.attempts().is_empty()); validate_inspection(&e, &empty).unwrap(); slice8a_cold(&f.0, &empty);
    assert_eq!(slice8a_snapshot(&f.0), before); assert!(!f.0.join(".hair-layout").exists());
    let d = slice5a_decoration([0., 0.]);
    assert_eq!(mac::place_decoration(&e, &d, &mut |_| Ok(()), |at| if at == 1 { Err("prepared refusal".into()) } else { Ok(()) }), Outcome::NotCommitted("prepared refusal".into()));
    let pending = slice5a_record(&f, &d).with_file_name("pending.toml");
    std::fs::write(&pending, b"[deliberately incomplete").unwrap();
    let i = inspect_placements(&e).unwrap(); assert_eq!(i.attempts()[0].state(), AttemptState::DefinitelyInactive); slice8a_cold(&f.0, &i);
    std::fs::rename(pending, f.0.join("retained-evidence")).unwrap();
    let i = inspect_placements(&e).unwrap(); assert_eq!(i.attempts()[0].kind(), AttemptKind::NewDecoration); slice8a_cold(&f.0, &i);
    // Existing strict closure reader admits empty and final-descriptor-only shapes.
    let source_fixture = Fixture::new(); let (_, theme) = slice6a_pack(&source_fixture); let source = select_pin_asset(&theme, "chair").unwrap();
    let slot = f.0.join(".hair-layout").join(format!("ac-{}", source.closure_identity().unwrap()));
    std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
    let i = inspect_placements(&e).unwrap(); assert_eq!(i.layout().incomplete_records, 2); slice8a_cold(&f.0, &i);
    let descriptor = slot.join("closure.toml"); std::fs::write(&descriptor, source.descriptor.text().unwrap()).unwrap();
    std::fs::set_permissions(descriptor, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = slice8a_snapshot(&f.0); let i = inspect_placements(&e).unwrap();
    assert_eq!(i.layout().incomplete_records, 2); assert!(i.attempts().iter().all(|a| a.state() == AttemptState::DefinitelyInactive));
    slice8a_cold(&f.0, &i); assert_eq!(slice8a_snapshot(&f.0), before);
}
#[test]
fn slice8a_stale_snapshot_replacement_bytes_xattr_and_ancestry_detection_no_writes() {
    use std::os::unix::fs::PermissionsExt;
    for target in ["record", "pending", "pending-bytes", "pending-xattr", "slot", "root", "lock", "directory"] {
        let (f, e, d, proposed) = slice5a_saved();
        assert_eq!(mac::relocate_decoration(&e, &d, &proposed, &mut |_| Ok(()), |at| if at == 1 { Err("leave pending".into()) } else { Ok(()) }), Outcome::NotCommitted("leave pending".into()));
        let i = inspect_placements(&e).unwrap(); validate_inspection(&e, &i).unwrap();
        let record = slice5a_record(&f, &d);
        let path = match target { "record" => record.clone(), "pending" | "pending-bytes" | "pending-xattr" => record.with_file_name("pending.toml"), "slot" => record.parent().unwrap().to_path_buf(), "root" => f.0.join(".hair-layout"), "lock" => f.0.join(".hair-layout/lock"), "directory" => {
            let other = Fixture::new(); let other_e = Explorer::new(&other.0).unwrap();
            assert!(validate_inspection(&other_e, &i).is_err()); continue;
        }, _ => unreachable!() };
        if target == "pending-bytes" { std::fs::write(&path, b"different incomplete bytes").unwrap(); }
        else if target == "pending-xattr" { slice6a_tag(&path); }
        else {
            let is_dir = path.is_dir(); let bytes = if is_dir { vec![] } else { std::fs::read(&path).unwrap() };
            std::fs::rename(&path, f.0.join("displaced")).unwrap();
            if is_dir { std::fs::create_dir(&path).unwrap(); std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap(); }
            else { std::fs::write(&path, bytes).unwrap(); std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap(); }
        }
        let before = slice8a_snapshot(&f.0);
        assert!(validate_inspection(&e, &i).is_err(), "stale {target} accepted");
        assert_eq!(slice8a_snapshot(&f.0), before, "validation changed {target}");
    }
}
#[test]
fn slice8a_busy_writer_and_child_inherited_inspection_lock_release() {
    let (f, e, _, _) = slice5a_saved();
    let writer = mac::HeldFlock::acquire(std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap(), libc::LOCK_EX).unwrap();
    let before = slice8a_snapshot(&f.0);
    assert!(inspect_placements(&e).err().unwrap().contains("busy"));
    assert_eq!(slice8a_snapshot(&f.0), before); drop(writer);
    let mut child = None;
    let i = mac::inspect_placements(&e, || {
        child = Some(ParkedSpawn::new(&f.0));
        assert!(mac::HeldFlock::acquire(std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap(), libc::LOCK_EX).is_err());
        Ok(())
    }).unwrap();
    // Keep both snapshot and pre-exec child alive: EX must already be available.
    let writer = mac::HeldFlock::acquire(std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap(), libc::LOCK_EX).unwrap();
    drop(writer); validate_inspection(&e, &i).unwrap(); drop(child);
}
#[test]
fn slice8a_corrupt_active_foreign_symlink_hardlink_and_acquisition_rebound_refuse() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for kind in ["active-malformed", "active-unknown", "foreign", "root-mode", "record-hardlink", "record-symlink", "slot-symlink", "root-symlink", "record-xattr", "slot-xattr", "record-acl"] {
        let (f, e, d, _) = slice5a_saved(); let record = slice5a_record(&f, &d);
        match kind {
            "active-malformed" => std::fs::write(&record, b"[broken").unwrap(),
            "active-unknown" => std::fs::write(&record, format!("{}\nforeign=true\n", toml::to_string(&d).unwrap())).unwrap(),
            "foreign" => std::fs::write(record.parent().unwrap().join("foreign"), b"never delete").unwrap(),
            "root-mode" => std::fs::set_permissions(f.0.join(".hair-layout"), std::fs::Permissions::from_mode(0o755)).unwrap(),
            "record-hardlink" => std::fs::hard_link(&record, f.0.join("alias")).unwrap(),
            "record-xattr" => slice6a_tag(&record), "slot-xattr" => slice6a_tag(record.parent().unwrap()),
            "record-acl" => slice6a_acl(&record),
            _ => {
                let path = match kind { "record-symlink" => record.clone(), "slot-symlink" => record.parent().unwrap().to_path_buf(), "root-symlink" => f.0.join(".hair-layout"), _ => unreachable!() };
                std::fs::rename(&path, f.0.join("displaced")).unwrap(); symlink(f.0.join("displaced"), &path).unwrap();
            }
        }
        let before = slice8a_snapshot(&f.0); assert!(inspect_placements(&e).is_err(), "{kind}");
        assert_eq!(slice8a_snapshot(&f.0), before, "{kind}");
    }
    let (f, e, d, _) = slice5a_saved(); let record = slice5a_record(&f, &d);
    let result = mac::inspect_placements(&e, || {
        let bytes = std::fs::read(&record).unwrap(); std::fs::rename(&record, f.0.join("displaced")).unwrap();
        std::fs::write(&record, bytes).unwrap(); std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).unwrap(); Ok(())
    });
    assert!(result.is_err(), "completed acquisition rebound must fail even at exact same bytes");
}
#[test]
fn slice8a_shared_slot_byte_record_and_closure_caps_are_not_reclaimed() {
    use std::os::unix::fs::PermissionsExt;
    let (f, e, _, _) = slice5a_saved(); let root = f.0.join(".hair-layout");
    for n in 0..255 {
        let slot = root.join(format!("dc-{n:032x}")); std::fs::create_dir(&slot).unwrap();
        std::fs::set_permissions(slot, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let i = inspect_placements(&e).unwrap(); assert_eq!(i.attempts().len(), 256); assert_eq!(i.layout().incomplete_records, 255);
    let ids: Vec<_> = i.attempts().iter().map(|a| a.identity()).collect(); assert!(ids.windows(2).all(|w| w[0] < w[1]));
    slice8a_cold(&f.0, &i);
    let overflow = root.join("dc-ffffffffffffffffffffffffffffffff"); std::fs::create_dir(&overflow).unwrap();
    std::fs::set_permissions(overflow, std::fs::Permissions::from_mode(0o700)).unwrap();
    let before = slice8a_snapshot(&f.0); assert!(inspect_placements(&e).is_err()); assert_eq!(slice8a_snapshot(&f.0), before);
    // Actual interrupted file writer fills distinct historical slots. Explicit
    // fixture extension measures logical byte caps; it is not a writer guarantee.
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    for _ in 0..8 {
        slice8a_file_crash(&f.0, 2);
        let payload = std::fs::read_dir(f.0.join(".hair-layout")).unwrap().map(|e| e.unwrap().path().join("payload"))
            .find(|p| p.exists() && std::fs::metadata(p).unwrap().len() < crate::local_explorer::MAX_BODY_BYTES).unwrap();
        std::fs::OpenOptions::new().write(true).open(payload).unwrap().set_len(crate::local_explorer::MAX_BODY_BYTES).unwrap();
    }
    let before = slice8a_snapshot(&f.0); let i = inspect_placements(&e).unwrap(); assert_eq!(i.attempts().len(), 8);
    assert_eq!(slice8a_snapshot(&f.0), before); drop(i);
    let refused = publish(&e, OsStr::new("BeyondBudget"), b"one", &pos(), &AtomicBool::new(false));
    assert!(matches!(refused, Outcome::NotCommitted(ref s) if s.contains("budget")));
    // Deliberate owned over-cap shape, not an invented successful writer result.
    let extra = f.0.join(".hair-layout/tx-ffffffffffffffffffffffffffffffff");
    std::fs::create_dir(&extra).unwrap(); std::fs::set_permissions(&extra, std::fs::Permissions::from_mode(0o700)).unwrap();
    let extra_payload = extra.join("payload"); std::fs::write(&extra_payload, b"one").unwrap();
    std::fs::set_permissions(extra_payload, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = slice8a_snapshot(&f.0); assert!(inspect_placements(&e).err().unwrap().contains("byte budget")); assert_eq!(slice8a_snapshot(&f.0), before);
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); slice8a_file_crash(&f.0, 3);
    std::fs::OpenOptions::new().write(true).open(slice8a_tx(&f).join("record.toml")).unwrap().set_len(4097).unwrap();
    let before = slice8a_snapshot(&f.0); assert!(inspect_placements(&e).is_err()); assert_eq!(slice8a_snapshot(&f.0), before);
    let (f, e, _) = slice6a1_saved();
    for n in 0..3 {
        let slot = f.0.join(".hair-layout").join(format!("ac-{n:064x}")); std::fs::create_dir(&slot).unwrap();
        std::fs::set_permissions(slot, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(inspect_placements(&e).unwrap().layout().incomplete_records, 3);
    let slot = f.0.join(".hair-layout/ac-ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(slot, std::fs::Permissions::from_mode(0o700)).unwrap();
    let before = slice8a_snapshot(&f.0); assert!(inspect_placements(&e).err().unwrap().contains("limit")); assert_eq!(slice8a_snapshot(&f.0), before);
}
