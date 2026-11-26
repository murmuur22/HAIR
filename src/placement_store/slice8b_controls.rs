// Additional selection, resource and actual cold-process controls.
#[test]
fn slice8b_device_identity_mismatch_refuses_before_validator_without_writes() {
    let (f, e, theme, d) = slice8b_prepare("pin");
    let mut i = inspect_placements(&e).unwrap();
    // Deterministic private-evidence fault injection, NOT a physical second
    // filesystem qualification or a public way to construct action authority.
    mac::resume_test_wrong_device(&mut i);
    let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
    let before = slice8a_snapshot(&f.0); let mut stages = vec![];
    let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, _| { stages.push(stage); Ok(()) });
    assert_eq!(out, Outcome::NotCommitted("Selection belongs to another inspected directory".into()));
    assert!(stages.is_empty()); assert_eq!(slice8a_snapshot(&f.0), before);
    println!("SLICE8B device-identity injected=mismatch reached={stages:?} outcome={out:?} physical-second-device=not-claimed");
}
#[test]
fn slice8b_partial_unsupported_orphan_and_file_history_never_select() {
    use std::os::unix::fs::PermissionsExt;
    for kind in ["empty", "partial", "binary", "unknown", "wrong-id", "pin-missing-closure", "relocation-tag", "relocation-size", "relocation-noop", "other-pending", "bad-other-active"] {
        let relocation = kind.starts_with("relocation");
        let (f, e, _, d) = slice8b_prepare(if relocation { "relocation" } else { "new" });
        let pending = slice5a_record(&f, &d).with_file_name("pending.toml");
        let bytes = std::fs::read_to_string(&pending).unwrap();
        match kind {
            "empty" => std::fs::write(&pending, b"").unwrap(),
            "partial" => std::fs::write(&pending, &bytes.as_bytes()[..11]).unwrap(),
            "binary" => std::fs::write(&pending, [0xff, 0, 0xfe]).unwrap(),
            "unknown" => std::fs::write(&pending, format!("{bytes}\nforeign=true")).unwrap(),
            "wrong-id" => { let mut p = d.clone(); p.placement_id = "ffffffffffffffffffffffffffffffff".into(); std::fs::write(&pending, toml::to_string(&p).unwrap()).unwrap(); }
            "pin-missing-closure" => {
                let theme = crate::theme::Theme::load(&f.0.join("pack/pack.toml"), &mut Default::default()).unwrap();
                let p = select_pin_asset(&theme, "chair").unwrap().decoration(&d).unwrap();
                std::fs::write(&pending, toml::to_string(&p).unwrap()).unwrap();
            }
            "relocation-tag" | "relocation-size" => {
                let mut p = d.clone(); if kind == "relocation-tag" { p.tag = "hair.different".into(); } else { p.size[0] += 0.1; }
                std::fs::write(&pending, toml::to_string(&p).unwrap()).unwrap();
            }
            "relocation-noop" => std::fs::write(&pending, std::fs::read(slice5a_record(&f, &d)).unwrap()).unwrap(),
            "other-pending" => {
                let slot = f.0.join(".hair-layout/dc-ffffffffffffffffffffffffffffffff"); std::fs::create_dir(&slot).unwrap();
                std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                std::fs::write(slot.join("pending.toml"), b"incomplete").unwrap();
                std::fs::set_permissions(slot.join("pending.toml"), std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            "bad-other-active" => {
                let layout = read_layout(&e).unwrap(); let other = layout.decorations.iter().find(|x| x.placement_id != d.placement_id).unwrap();
                std::fs::write(slice5a_record(&f, other), b"[malformed-active").unwrap();
            }
            _ => unreachable!(),
        }
        let before = slice8a_snapshot(&f.0);
        match inspect_placements(&e) {
            Ok(i) => { assert!(i.select_decoration_resume(slice8b_index(&i, &d)).is_err(), "{kind}"); }
            Err(reason) => println!("SLICE8B unsupported={kind} stage=inspection outcome={reason}"),
        }
        assert_eq!(slice8a_snapshot(&f.0), before);
    }
    for stage in [3, 4, 5, 6, 7] {
        let (f, e, theme) = slice6a1_saved(); let source = select_pin_asset(&theme, "chair").unwrap();
        let mut reached = false;
        let out = mac::publish_pin(&e, &source, &slice6a_new(&theme), &mut |_| Ok(()), |at| if at == stage { reached = true; Err("closure-only boundary".into()) } else { Ok(()) });
        assert!(reached); assert_eq!(out, Outcome::NotCommitted("closure-only boundary".into()));
        let before = slice8a_snapshot(&f.0); let i = inspect_placements(&e).unwrap();
        for index in 0..i.attempts().len() { assert!(i.select_decoration_resume(index).is_err(), "orphan/partial closure is not a placement intent"); }
        assert_eq!(slice8a_snapshot(&f.0), before);
    }
    for stage in [1, 2, 3, 4] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); slice8a_file_crash(&f.0, stage);
        let before = slice8a_snapshot(&f.0); let i = inspect_placements(&e).unwrap();
        for index in 0..i.attempts().len() { assert!(i.select_decoration_resume(index).is_err(), "file Copy/Move/predecessor history unproven"); }
        assert_eq!(slice8a_snapshot(&f.0), before);
    }
}
#[test]
fn slice8b_resource_and_appearance_admission_do_not_reclaim_or_weaken_caps() {
    use std::os::unix::fs::PermissionsExt;
    for kind in ["count", "models", "encoded", "aggregate-payload", "missing-tag"] {
        let (f, e, mut theme, d) = slice8b_prepare("new");
        let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
        match kind {
            "count" => {
                for n in 0..255 {
                    let slot = f.0.join(format!(".hair-layout/dc-{n:032x}")); std::fs::create_dir(&slot).unwrap();
                    std::fs::set_permissions(slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            "models" => {
                for n in 0..4 {
                    let slot = f.0.join(format!(".hair-layout/ac-{n:064x}")); std::fs::create_dir(&slot).unwrap();
                    std::fs::set_permissions(slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            "encoded" | "aggregate-payload" => {
                for n in 0..if kind == "encoded" { 1 } else { 9 } {
                    let slot = f.0.join(format!(".hair-layout/tx-{n:032x}")); std::fs::create_dir(&slot).unwrap();
                    std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                    let path = slot.join("payload"); let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
                    file.set_len(if kind == "encoded" { crate::local_explorer::MAX_BODY_BYTES + 1 } else { crate::local_explorer::MAX_BODY_BYTES }).unwrap();
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
                }
            }
            "missing-tag" => theme.decorations.entries.clear(),
            _ => unreachable!(),
        }
        let before = slice8a_snapshot(&f.0); let mut stages = vec![];
        let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
            stages.push(stage); crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
        });
        if kind == "missing-tag" {
            assert_eq!(out, Outcome::Committed { synced: true });
            assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final]);
            assert!(read_layout(&e).unwrap().decorations.contains(&d));
            assert!(theme.decoration_preview(&d).unwrap()[..36].iter().all(|v| v.color == [0.5; 3]));
            continue;
        }
        assert!(matches!(out, Outcome::NotCommitted(_)), "{kind}: {out:?}"); assert!(stages.is_empty());
        assert_eq!(slice8a_snapshot(&f.0), before);
        if kind != "missing-tag" { assert!(inspect_placements(&e).is_err(), "current caps must also independently refuse {kind}"); }
        println!("SLICE8B resource={kind} reached={stages:?} outcome={out:?} no-reclamation=PASS");
    }
}
#[test]
#[ignore = "child-only actual cold selection/resume process interruption"]
fn slice8b_action_process_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8B_ROOT").unwrap()); assert!(root.starts_with(std::env::temp_dir()));
    let e = Explorer::new(&root).unwrap(); let i = inspect_placements(&e).unwrap();
    let id = std::env::var("HAIR_SLICE8B_ID").unwrap();
    let index = i.attempts().iter().position(|a| a.identity() == format!("dc-{id}")).unwrap();
    let selected = i.select_decoration_resume(index).unwrap(); let stage: u8 = std::env::var("HAIR_SLICE8B_STAGE").unwrap().parse().unwrap();
    let theme = slice8b_theme();
    let out = mac::resume_decoration(&e, &selected, &AtomicBool::new(false), &mut |_, candidate| {
        theme.validate_layout_decorations(candidate)?;
        crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
    }, |at| { if at == stage { unsafe { libc::_exit(73) }; } Ok(()) }, None);
    panic!("process did not reach requested action stage: {out:?}");
}
#[test]
fn slice8b_actual_cold_action_interruption_before_and_after_atomic_activation() {
    for kind in ["new", "pin", "relocation", "pin-relocation"] {
        for stage in [0, 1, 2] {
            let (f, e, _, expected) = slice8b_prepare(kind); let prior = read_layout(&e).unwrap(); let before = slice8a_snapshot(&f.0);
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "placement_store::tests::slice8b_action_process_child", "--ignored", "--nocapture"])
                .env("HAIR_SLICE8B_ROOT", &f.0).env("HAIR_SLICE8B_ID", &expected.placement_id).env("HAIR_SLICE8B_STAGE", stage.to_string()).output().unwrap();
            assert_eq!(output.status.code(), Some(73), "{kind}/{stage}: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            let live = read_layout(&e).unwrap();
            if stage != 2 { assert_eq!(slice8a_snapshot(&f.0), before); assert_eq!(live, prior); }
            else { assert!(live.decorations.contains(&expected)); assert_eq!(live.incomplete_records, 0); assert_eq!(live.decorations.len(), 2); }
            let snapshot = slice8a_snapshot(&f.0); slice8b_cold(&f.0, &live); assert_eq!(slice8a_snapshot(&f.0), snapshot);
            println!("SLICE8B cold-action kind={kind} stage={stage} exit=73 active={} cold-read=PASS no-auto-resume=PASS", stage == 2);
        }
    }
}
