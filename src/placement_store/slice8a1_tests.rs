// Repair controls mutate only disposable owned protocol fixtures.
fn slice8a1_child(test: &str, root: Option<&std::path::Path>, limit: bool) -> std::process::Output {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", test, "--ignored", "--nocapture"]);
    if let Some(root) = root { cmd.env("HAIR_SLICE8A1_ROOT", root); }
    if limit {
        // Only the owned child changes its soft limit, never the test host/hard limit.
        unsafe { cmd.pre_exec(|| {
            let mut r = std::mem::zeroed();
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) != 0 { return Err(std::io::Error::last_os_error()); }
            r.rlim_cur = 256;
            if libc::setrlimit(libc::RLIMIT_NOFILE, &r) != 0 { return Err(std::io::Error::last_os_error()); }
            Ok(())
        }); }
    }
    cmd.output().unwrap()
}
fn slice8a1_success(output: std::process::Output) {
    println!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "owned child exited {:?}", output.status);
}
#[test]
fn slice8a1_fd256_existing_cap_and_repeated_snapshot_subprocesses() {
    // Existing cap test is NOT ignored, so invoke separately without --ignored.
    use std::os::unix::process::CommandExt;
    let mut cap = std::process::Command::new(std::env::current_exe().unwrap());
    cap.args(["--exact", "placement_store::tests::slice8a_shared_slot_byte_record_and_closure_caps_are_not_reclaimed", "--nocapture"]);
    unsafe { cap.pre_exec(|| {
        let mut r = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) != 0 { return Err(std::io::Error::last_os_error()); }
        r.rlim_cur = 256;
        if libc::setrlimit(libc::RLIMIT_NOFILE, &r) != 0 { return Err(std::io::Error::last_os_error()); }
        Ok(())
    }); }
    slice8a1_success(cap.output().unwrap());
    slice8a1_success(slice8a1_child("placement_store::tests::slice8a1_fd256_child", None, true));
}
fn slice8a1_fd_count() -> usize {
    (0..256).filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0).count()
}
#[test]
#[ignore = "owned fd256 child only"]
fn slice8a1_fd256_child() {
    use std::os::unix::fs::PermissionsExt;
    let mut r = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) }, 0);
    assert_eq!(r.rlim_cur, 256, "resource proof must not inherit a higher limit");
    let (f, e, d, _) = slice5a_saved();
    for n in 0..255 {
        let slot = f.0.join(".hair-layout").join(format!("dc-{n:032x}"));
        std::fs::create_dir(&slot).unwrap();
        std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut record = d.clone(); record.placement_id = format!("{n:032x}");
        let mut text = toml::to_string(&record).unwrap(); assert!(text.len() <= 4096);
        text.extend(std::iter::repeat_n(' ', 4096 - text.len()));
        for name in ["decoration.toml", "pending.toml"] {
            let path = slot.join(name); std::fs::write(&path, text.as_bytes()).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    let mut text = toml::to_string(&d).unwrap(); text.extend(std::iter::repeat_n(' ', 4096 - text.len()));
    let record = slice5a_record(&f, &d); std::fs::write(&record, text.as_bytes()).unwrap();
    let pending = record.with_file_name("pending.toml"); std::fs::write(&pending, text.as_bytes()).unwrap();
    std::fs::set_permissions(pending, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = slice8a_snapshot(&f.0);
    let baseline = slice8a1_fd_count();
    let mut peak = baseline;
    for _ in 0..20 {
        let i = mac::inspect_with_hooks(&e, || Ok(()), |_, _| { peak = peak.max(slice8a1_fd_count()); Ok(()) }).unwrap();
        assert_eq!(i.attempts().len(), 256);
        assert_eq!(i.layout().decorations.len(), 256); assert_eq!(i.layout().incomplete_records, 256);
        assert_eq!(mac::inspection_raw_accounting(&i), (0, 2 * 256 * 4096, 0));
        assert_eq!(slice8a1_fd_count(), baseline + 1, "only the directory anchor survives return");
        validate_inspection(&e, &i).unwrap();
        assert_eq!(slice8a1_fd_count(), baseline + 1, "fresh validation leaked descriptors");
        let reserve: Vec<_> = (0..32).map(|_| std::fs::File::open("/dev/null").unwrap()).collect();
        validate_inspection(&e, &i).unwrap(); // Retained UI plus unrelated process work.
        drop(reserve); drop(i);
        assert_eq!(slice8a1_fd_count(), baseline, "inspect/validate/drop leaked descriptors");
    }
    assert!(peak <= baseline + 16, "scoped acquisition is not constant-space: {peak} vs {baseline}");
    assert_eq!(slice8a_snapshot(&f.0), before);
    println!("SLICE8A1 FD soft=256 slots=256 children=512 metadata_bytes={} iterations=20 baseline={baseline} sampled_peak={peak} retained_delta=1 reserve=32 leak=0", 2 * 256 * 4096);
}
#[test]
fn slice8a1_exact_acquired_bytes_survive_root_slot_record_aba_seam_without_mixed_labels() {
    use std::os::unix::fs::PermissionsExt;
    for target in ["root", "slot", "record"] {
        let (f, e, d, _) = slice5a_saved();
        let record = slice5a_record(&f, &d);
        let path = match target { "root" => f.0.join(".hair-layout"), "slot" => record.parent().unwrap().to_path_buf(), _ => record.clone() };
        let other = Fixture::new();
        let replacement = other.0.join("replacement");
        if target == "record" {
            std::fs::write(&replacement, b"[malformed-active-B").unwrap();
            std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600)).unwrap();
        } else {
            std::fs::create_dir(&replacement).unwrap();
            std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
            let slot = if target == "root" {
                let lock = replacement.join("lock"); std::fs::write(&lock, b"").unwrap();
                std::fs::set_permissions(lock, std::fs::Permissions::from_mode(0o600)).unwrap();
                let slot = replacement.join(record.parent().unwrap().file_name().unwrap());
                std::fs::create_dir(&slot).unwrap();
                std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap(); slot
            } else { replacement.clone() };
            let bad = slot.join("decoration.toml"); std::fs::write(&bad, b"[malformed-active-B").unwrap();
            std::fs::set_permissions(bad, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let displaced = other.0.join("A");
        let original_bytes = std::fs::read(&record).unwrap();
        let mut entered = false; let mut derived_from_a = false;
        let result = mac::inspect_with_hooks(&e, || Ok(()), |stage, _| {
            if stage == "before-derive" {
                entered = true;
                std::fs::rename(&path, &displaced).unwrap();
                std::fs::rename(&replacement, &path).unwrap();
                // A secondary reader here must reject B, not supply A's labels.
                assert!(read_layout(&e).is_err(), "B must be observably distinct and invalid");
            } else if stage == "after-derive" {
                // Reaching here proves derivation did not reopen invalid B.
                derived_from_a = true;
                std::fs::rename(&path, &replacement).unwrap();
                std::fs::rename(&displaced, &path).unwrap();
            }
            Ok(())
        });
        assert!(entered && derived_from_a, "actual {target} derivation seam was not exercised");
        assert!(result.is_err(), "A-B-A changed metadata must not return a mixed snapshot: {target}");
        assert_eq!(std::fs::read(&record).unwrap(), original_bytes);
        assert_eq!(read_layout(&e).unwrap().decorations, vec![d]);
        println!("SLICE8A1 ABA target={target} B-invalid=observed exact-A-derived=true stale-refused=true original-bytes-preserved=true");
    }
}
#[test]
fn slice8a1_actual_aggregate_boundary_and_post_check_growth_refuse_without_changes() {
    use std::os::unix::fs::PermissionsExt;
    let (f, e, _, _) = slice5a_saved();
    let mut payloads = vec![];
    for n in 0..9 {
        let tx = f.0.join(".hair-layout").join(format!("tx-{n:032x}"));
        std::fs::create_dir(&tx).unwrap(); std::fs::set_permissions(&tx, std::fs::Permissions::from_mode(0o700)).unwrap();
        let payload = tx.join("payload"); std::fs::write(&payload, b"").unwrap();
        std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::OpenOptions::new().write(true).open(&payload).unwrap().set_len(if n < 8 { crate::local_explorer::MAX_BODY_BYTES } else { 0 }).unwrap();
        payloads.push(payload);
    }
    let before = slice8a_snapshot(&f.0);
    let i = inspect_placements(&e).unwrap();
    assert_eq!(i.attempts().len(), 10); validate_inspection(&e, &i).unwrap();
    let (body, metadata, _) = mac::inspection_raw_accounting(&i);
    assert_eq!(body, crate::inventory::MAX_TOTAL); assert!(metadata > 0);
    assert_eq!(slice8a_snapshot(&f.0), before); drop(i);
    let mut grown = None;
    let result = mac::inspect_with_hooks(&e, || Ok(()), |stage, _| {
        if stage == "budget-checked" {
            std::fs::OpenOptions::new().write(true).open(&payloads[8]).unwrap().set_len(1).unwrap();
            grown = Some(slice8a_snapshot(&f.0));
        }
        Ok(())
    });
    assert!(result.err().unwrap().contains("acquired byte budget"));
    assert_eq!(slice8a_snapshot(&f.0), grown.unwrap(), "refusal must preserve even over-budget evidence");
    println!("SLICE8A1 aggregate actual-boundary=33554432 plus-decoration-metadata=PASS post-check-plus-one=refused unchanged=true");
}
#[test]
fn slice8a1_reader_compatible_position_and_known_legacy_are_never_action_handles() {
    for legacy in [false, true] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
        assert_eq!(publish(&e, OsStr::new("Final"), b"only content", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
        let path = slice8a_tx(&f).join("record.toml");
        let mut r: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut position: Position = r["position"].clone().try_into().unwrap();
        position.surface = Some(crate::placement::surface::Contact { at: [position.at[0], 1., position.at[1]], normal: [0., 1., 0.] });
        position.properties.base = true;
        assert!(position.validate().is_err(), "new placements must retain stronger base/floor validation");
        r["position"] = toml::Value::try_from(position).unwrap();
        if legacy { r["version"] = toml::Value::Integer(1); r.as_table_mut().unwrap().remove("identity"); }
        std::fs::write(&path, toml::to_string(&r).unwrap()).unwrap();
        let before = slice8a_snapshot(&f.0);
        let ordinary = read_layout(&e).unwrap();
        assert_eq!(ordinary.positions.len(), usize::from(!legacy));
        let i = inspect_placements(&e).unwrap(); assert_eq!(i.layout(), &ordinary);
        let display = i.attempts()[0].clone();
        assert_eq!(display.active_predecessor(), None);
        assert_eq!(display.state(), if legacy { AttemptState::Unsupported } else { AttemptState::Published });
        if legacy { assert!(display.status().contains("legacy") && display.status().contains("no resume/discard")); }
        validate_inspection(&e, &i).unwrap(); slice8a_cold(&f.0, &i);
        assert_eq!(slice8a_snapshot(&f.0), before);
    }
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); slice8a_file_crash(&f.0, 3);
    let i = inspect_placements(&e).unwrap(); let display = i.attempts()[0].clone();
    assert_eq!(display.state(), AttemptState::DefinitelyInactive);
    assert_eq!(display.active_predecessor(), None); assert!(display.status().contains("no discard/resume"));
    let payload = slice8a_tx(&f).join("payload"); let sole_copy = std::fs::read(&payload).unwrap(); assert!(!sole_copy.is_empty());
    drop(i); assert_eq!(std::fs::read(payload).unwrap(), sole_copy);
    // Cloned display survives but there is deliberately no selection/action API.
    assert_eq!(display.kind(), AttemptKind::FileTransaction);
}
#[test]
fn slice8a1_no_new_cold_scratch_trigger_for_reader_empty_and_file_only_shapes() {
    use std::os::unix::fs::PermissionsExt;
    for shape in ["missing", "lock-only", "empty-decoration", "file-only"] {
        let f = Fixture::new();
        if shape != "missing" {
            let root = f.0.join(".hair-layout"); std::fs::create_dir(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            let lock = root.join("lock"); std::fs::write(&lock, b"").unwrap();
            std::fs::set_permissions(lock, std::fs::Permissions::from_mode(0o600)).unwrap();
            if shape != "lock-only" {
                let slot = root.join(format!("{}-{:032x}", if shape == "file-only" { "tx" } else { "dc" }, 1));
                std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                if shape == "file-only" {
                    let payload = slot.join("payload"); std::fs::write(&payload, b"sole-copy").unwrap();
                    std::fs::set_permissions(payload, std::fs::Permissions::from_mode(0o600)).unwrap();
                }
            }
        }
        let before = slice8a_snapshot(&f.0);
        slice8a1_success(slice8a1_child("placement_store::tests::slice8a1_no_probe_child", Some(&f.0), false));
        assert_eq!(slice8a_snapshot(&f.0), before);
        println!("SLICE8A1 cold no-new-probe shape={shape} unchanged=true");
    }
}
#[test]
#[ignore = "owned cold origin-policy child only"]
fn slice8a1_no_probe_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8A1_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    assert!(!mac::inspection_origin_initialized());
    let e = Explorer::new(&root).unwrap(); let ordinary = read_layout(&e).unwrap();
    assert!(!mac::inspection_origin_initialized(), "ordinary shape must not qualify scratch");
    let i = inspect_placements(&e).unwrap(); assert_eq!(i.layout(), &ordinary);
    validate_inspection(&e, &i).unwrap();
    assert!(!mac::inspection_origin_initialized(), "inspection introduced a new cold scratch trigger");
}
#[test]
fn slice8a1_closure_layout_shares_exact_raw_bytes_and_orphans_are_not_paired() {
    let (f, e, theme) = slice6a1_saved();
    let source = select_pin_asset(&theme, "chair").unwrap();
    let d = slice6a_new(&theme);
    assert_eq!(mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |_| Ok(())), Outcome::Committed { synced: true });
    let before = slice8a_snapshot(&f.0);
    let i = inspect_placements(&e).unwrap();
    let (body, metadata, shared) = mac::inspection_raw_accounting(&i);
    assert_eq!(body as u64, source.descriptor.bytes); assert!(metadata > 0);
    assert_eq!(shared, 1, "layout must share evidence's encoded Arc, not retain duplicate GLB bytes");
    assert_eq!(i.layout(), &read_layout(&e).unwrap()); validate_inspection(&e, &i).unwrap();
    assert_eq!(slice8a_snapshot(&f.0), before);
    // A pending decoration does not establish an actionable pairing with the
    // finalized closure after the exact active reference is removed in fixture.
    let active = slice5a_record(&f, &d);
    std::fs::rename(&active, active.with_file_name("pending.toml")).unwrap();
    let i = inspect_placements(&e).unwrap();
    let orphan = i.attempts().iter().find(|a| a.kind() == AttemptKind::PinClosure).unwrap();
    assert_eq!(orphan.state(), AttemptState::DefinitelyInactive);
    assert!(orphan.status().contains("orphan"));
    assert_eq!(mac::inspection_raw_accounting(&i).2, 0);
    assert!(i.layout().decorations.iter().all(|d| d.closure_identity().is_none()));
    println!("SLICE8A1 closure encoded evidence/layout Arc shared=1 orphan-pending-unpaired=true");
}
