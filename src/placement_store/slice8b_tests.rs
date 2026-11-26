// Explicit checked action tests. All filesystem mutations are disposable fixture controls.
use std::sync::atomic::Ordering;
fn slice8b_theme() -> crate::theme::Theme {
    crate::theme::Theme::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/office.toml"), &mut Default::default()).unwrap()
}
fn slice8b_prepare(kind: &str) -> (Fixture, Explorer, crate::theme::Theme, Decoration) {
    let f = Fixture::new();
    std::fs::write(f.0.join("original.txt"), b"owner original, never disposition authority").unwrap();
    let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    let other = slice5a_decoration([-2., 0.]);
    assert_eq!(place_decoration_checked(&e, &other, |live| {
        theme.validate_decoration_layout(live, &other)?;
        crate::placement_scene::validate_decoration(&e, live, &other, [0., 2.], 0)
    }), Outcome::Committed { synced: true });
    let mut d = slice5a_decoration([2., 0.]); d.size = [0.4; 3];
    let pin = kind.contains("pin"); let relocation = kind.contains("relocation");
    if relocation {
        let out = if pin {
            let source = select_pin_asset(&theme, "chair").unwrap();
            let out = theme.publish_pin_checked(&e, &source, &d, |candidate| crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0));
            d = source.decoration(&d).unwrap(); out
        } else { place_decoration_checked(&e, &d, |live| {
            theme.validate_decoration_layout(live, &d)?;
            crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)
        }) };
        assert_eq!(out, Outcome::Committed { synced: true });
    }
    let mut proposed = d.clone(); if relocation { proposed.at = [2.1, 0.]; proposed.yaw = 0.1; proposed.collision = false; }
    let mut reached = false;
    let mut barrier = |stage| if stage == 1 { reached = true; Err("actual fully prepared boundary".into()) } else { Ok(()) };
    let out = if relocation {
        mac::relocate_decoration(&e, &d, &proposed, &mut |candidate| {
            theme.validate_layout_decorations(candidate)?;
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
        }, &mut barrier)
    } else if pin {
        let source = select_pin_asset(&theme, "chair").unwrap();
        proposed = source.decoration(&d).unwrap();
        mac::publish_pin(&e, &source, &d, &mut |candidate| {
            theme.validate_layout_decorations(candidate)?;
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
        }, &mut barrier)
    } else { mac::place_decoration(&e, &d, &mut |live| {
        theme.validate_decoration_layout(live, &d)?;
        crate::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)
    }, &mut barrier) };
    assert!(reached, "{kind}: writer did not reach prepared boundary: {out:?}");
    assert_eq!(out, Outcome::NotCommitted("actual fully prepared boundary".into()));
    (f, e, theme, proposed)
}
fn slice8b_index(i: &PlacementInspection, d: &Decoration) -> usize {
    i.attempts().iter().position(|a| a.identity() == format!("dc-{}", d.placement_id)).unwrap()
}
fn slice8b_cold(root: &std::path::Path, expected: &LayoutRead) {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::slice8b_cold_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE8B_ROOT", root).env("HAIR_SLICE8B_EXPECT", format!("{expected:?}")).output().unwrap();
    assert!(child.status.success(), "{}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
}
#[test]
#[ignore = "child-only strict cold reader with real retained closure and Theme admission"]
fn slice8b_cold_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8B_ROOT").unwrap()); assert!(root.starts_with(std::env::temp_dir()));
    let before = slice8a_snapshot(&root); let e = Explorer::new(&root).unwrap();
    let live = read_layout(&e).unwrap(); assert_eq!(format!("{live:?}"), std::env::var("HAIR_SLICE8B_EXPECT").unwrap());
    if live.incomplete_records == 0 {
        slice8b_theme().validate_layout_decorations(&live).unwrap();
        crate::placement_scene::validate_decoration_candidate(&e, &live, [0., 2.], 0).unwrap();
    }
    assert_eq!(slice8a_snapshot(&root), before);
}
#[test]
fn slice8b_vertical_follow_pin_and_both_relocations_keep_every_unselected_object_exact() {
    for kind in ["new", "pin", "relocation", "pin-relocation"] {
        let (f, e, theme, expected) = slice8b_prepare(kind);
        let before = slice8a_snapshot(&f.0);
        let i = inspect_placements(&e).unwrap(); let index = slice8b_index(&i, &expected);
        let display = i.attempts()[index].clone();
        let s = i.select_decoration_resume(index).unwrap();
        assert_eq!(s.proposal(), &expected); assert_eq!(s.predecessor().is_some(), kind.contains("relocation"));
        // A cloned display is inert, and changing a proposal clone lends no authority.
        let mut forged = s.proposal().clone(); forged.at = [100., 100.];
        assert_eq!(s.proposal(), &expected); assert_eq!(display, i.attempts()[index]);
        assert_eq!(slice8a_snapshot(&f.0), before, "selection must be write-free");
        let pending_path = f.0.join(".hair-layout").join(display.identity()).join("pending.toml");
        let active_path = pending_path.with_file_name("decoration.toml");
        let pending = before.files.iter().find(|x| x.0 == pending_path).unwrap().clone();
        let mut stages = vec![];
        let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
            stages.push(stage);
            assert!(read_layout(&e).unwrap_err().contains("busy")); // EX really held; no nested SH.
            assert_eq!(candidate.decorations.iter().filter(|d| d.placement_id == expected.placement_id).collect::<Vec<_>>(), vec![&expected]);
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
        });
        assert_eq!(out, Outcome::Committed { synced: true });
        assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final]);
        let after = slice8a_snapshot(&f.0);
        for entry in &before.files {
            if entry.0 == pending_path || entry.0 == active_path { continue; }
            assert!(after.files.contains(entry), "{kind}: unselected changed: {entry:?}");
        }
        for entry in &before.attrs {
            if entry.0 == pending_path || entry.0 == active_path { continue; }
            assert!(after.attrs.contains(entry), "{kind}: unselected attrs changed");
        }
        let active = after.files.iter().find(|x| x.0 == active_path).unwrap();
        assert_eq!((active.1, active.2, &active.3), (pending.1, pending.2, &pending.3));
        assert!(!pending_path.exists());
        let mut reopened = Explorer::new(&f.0).unwrap(); reopened.refresh().unwrap();
        let live = read_layout(&reopened).unwrap(); assert!(live.decorations.contains(&expected)); assert_eq!(live.decorations.len(), 2); assert_eq!(live.incomplete_records, 0);
        slice8b_cold(&f.0, &live);
        let snapshot = slice8a_snapshot(&f.0);
        let again = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |_, _| panic!("stale selection retried"));
        assert!(matches!(again, Outcome::NotCommitted(_))); assert_eq!(slice8a_snapshot(&f.0), snapshot);
        println!("SLICE8B vertical kind={kind} stages={stages:?} outcome={out:?} cold=PASS unselected=exact target=pending-inode-and-bytes");
    }
}
#[test]
fn slice8b_binding_foreign_folder_other_inspection_and_busy_cancel_are_preserving() {
    let (f, e, theme, d) = slice8b_prepare("new"); let i = inspect_placements(&e).unwrap();
    let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
    let (other_f, other_e, _, _) = slice8b_prepare("new");
    let other_i = inspect_placements(&other_e).unwrap();
    assert_ne!(i.attempts(), other_i.attempts());
    let before = slice8a_snapshot(&f.0); let other_before = slice8a_snapshot(&other_f.0);
    let out = theme.resume_decoration_checked(&other_e, &s, &AtomicBool::new(false), |_, _| panic!("foreign binding reached validator"));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("another inspected directory")), "{out:?}");
    assert!(i.select_decoration_resume(usize::MAX).is_err());
    for at in ["busy", "cancel-initial", "cancel-review", "cancel-final"] {
        let cancelled = AtomicBool::new(at == "cancel-initial");
        let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
        let held = if at == "busy" { Some(mac::HeldFlock::acquire(lock, libc::LOCK_SH).unwrap()) } else { None };
        let mut stages = vec![];
        let out = theme.resume_decoration_checked(&e, &s, &cancelled, |stage, candidate| {
            stages.push(stage);
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)?;
            if (at == "cancel-review" && stage == ResumeStage::Review) || (at == "cancel-final" && stage == ResumeStage::Final) { cancelled.store(true, Ordering::Release); }
            Ok(())
        });
        assert!(matches!(out, Outcome::NotCommitted(_)), "{at}: {out:?}"); drop(held);
        assert_eq!(slice8a_snapshot(&f.0), before); validate_inspection(&e, &i).unwrap();
        println!("SLICE8B binding-control={at} reached={stages:?} outcome={out:?}");
    }
    assert_eq!(slice8a_snapshot(&other_f.0), other_before);
}
#[test]
fn slice8b_fresh_held_mutation_refusal_before_review_after_review_and_final() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for phase in [None, Some(ResumeStage::Review), Some(ResumeStage::Final)] {
        for kind in ["pending-bytes", "pending-inode", "predecessor-bytes", "predecessor-inode", "closure-bytes", "closure-inode", "descriptor", "mode", "acl", "xattr", "hardlink", "symlink", "slot", "root", "lock", "foreign-child", "oversize"] {
            let (f, e, theme, d) = slice8b_prepare("pin-relocation");
            let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
            let slot = f.0.join(".hair-layout").join(format!("dc-{}", d.placement_id)); let pending = slot.join("pending.toml");
            let active = slot.join("decoration.toml"); let closure = slice6a_closure(&f);
            let mutate = || {
                match kind {
                    "pending-bytes" => std::fs::write(&pending, b"[changed").unwrap(),
                    "predecessor-bytes" => std::fs::write(&active, b"[changed").unwrap(),
                    "closure-bytes" => std::fs::write(closure.join("asset.glb"), b"changed").unwrap(),
                    "descriptor" => std::fs::write(closure.join("closure.toml"), b"changed").unwrap(),
                    "pending-inode" | "predecessor-inode" | "closure-inode" => {
                        let path = if kind == "pending-inode" { pending.clone() } else if kind == "predecessor-inode" { active.clone() } else { closure.join("asset.glb") };
                        let bytes = std::fs::read(&path).unwrap(); std::fs::rename(&path, f.0.join("displaced")).unwrap();
                        std::fs::write(&path, bytes).unwrap(); std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
                    }
                    "mode" => std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o644)).unwrap(),
                    "acl" => slice6a_acl(&pending), "xattr" => slice6a_tag(&pending),
                    "hardlink" => std::fs::hard_link(&pending, f.0.join("alias")).unwrap(),
                    "symlink" => { std::fs::rename(&pending, f.0.join("displaced")).unwrap(); symlink(f.0.join("displaced"), &pending).unwrap(); }
                    "slot" | "root" => {
                        let path = if kind == "slot" { slot.clone() } else { f.0.join(".hair-layout") };
                        std::fs::rename(&path, f.0.join("displaced")).unwrap(); symlink(f.0.join("displaced"), path).unwrap();
                    }
                    "lock" => { let path = f.0.join(".hair-layout/lock"); std::fs::rename(&path, f.0.join("displaced")).unwrap(); std::fs::write(&path, b"").unwrap(); std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap(); }
                    "foreign-child" => std::fs::write(slot.join("foreign"), b"preserve").unwrap(),
                    "oversize" => std::fs::OpenOptions::new().write(true).open(&pending).unwrap().set_len(4097).unwrap(),
                    _ => unreachable!(),
                }
                slice8a_snapshot(&f.0)
            };
            let mut mutated = if phase.is_none() { Some(mutate()) } else { None }; let mut stages = vec![];
            let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
                stages.push(stage);
                crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)?;
                if phase == Some(stage) { mutated = Some(mutate()); }
                Ok(())
            });
            assert!(matches!(out, Outcome::NotCommitted(_)), "{phase:?}/{kind}: {out:?}");
            assert_eq!(slice8a_snapshot(&f.0), mutated.unwrap(), "writer must not change mutated evidence");
            println!("SLICE8B held-control={kind} inject={phase:?} reached={stages:?} outcome={out:?}");
        }
    }
}
#[test]
fn slice8b_genuine_late_final_geometry_and_listing_rejections_preserve_pending() {
    for kind in ["visitor-route", "late-overlap", "post-validator-listing", "ceiling"] {
        let (f, e, theme, d) = slice8b_prepare("new");
        let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
        let before = slice8a_snapshot(&f.0); let mut stages = vec![];
        let out = theme.resume_decoration_checked(&e, &s, &AtomicBool::new(false), |stage, candidate| {
            stages.push(stage);
            let visitor = if stage == ResumeStage::Final && kind == "visitor-route" { d.at } else { [0., 2.] };
            if stage == ResumeStage::Final && kind == "late-overlap" {
                // Fresh real listing adds a generated slot under a separate
                // constraint checked by the real scene clearance oracle below.
                let mut constrained = candidate.clone(); let mut obstruction = slice5a_decoration(d.at); obstruction.size = [0.4; 3]; constrained.decorations.push(obstruction);
                return crate::placement_scene::validate_decoration_candidate(&e, &constrained, visitor, 0);
            }
            crate::placement_scene::validate_decoration_candidate(&e, candidate, visitor, 0)?;
            if stage == ResumeStage::Final && kind == "ceiling" {
                let mut fresh = e.try_clone()?; fresh.refresh()?;
                let (world, mut host) = crate::folder_scene::build(fresh.current(), 0, true)?;
                host.open_platform = false; host.height = 0.1;
                crate::placement_scene::apply(candidate, &world, &mut host)?;
            }
            if stage == ResumeStage::Final && kind == "post-validator-listing" { std::fs::write(f.0.join("zlate.txt"), b"unvalidated listing").unwrap(); }
            Ok(())
        });
        assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final], "must demonstrate genuine final validator boundary: {out:?}");
        assert!(matches!(out, Outcome::NotCommitted(_)), "{kind}: {out:?}");
        let after = slice8a_snapshot(&f.0);
        for entry in before.files { assert!(after.files.contains(&entry)); }
        for entry in before.attrs { assert!(after.attrs.contains(&entry)); }
        println!("SLICE8B late-final={kind} reached={stages:?} outcome={out:?}");
    }
}
#[test]
fn slice8b_rename_outcomes_never_rollback_or_automatically_retry() {
    for code in [None, Some(libc::EEXIST), Some(libc::EIO), Some(libc::EINTR)] {
        let (f, e, theme, d) = slice8b_prepare("new"); let i = inspect_placements(&e).unwrap(); let s = i.select_decoration_resume(slice8b_index(&i, &d)).unwrap();
        let before = slice8a_snapshot(&f.0); let mut stages = vec![]; let mut postcommit = false;
        let out = mac::resume_decoration(&e, &s, &AtomicBool::new(false), &mut |stage, candidate| {
            stages.push(stage); theme.validate_layout_decorations(candidate)?;
            crate::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
        }, |at| if at == 2 { postcommit = true; Err("sync seam".into()) } else { Ok(()) }, code);
        assert_eq!(stages, vec![ResumeStage::Review, ResumeStage::Final]);
        match code {
            None => { assert!(postcommit); assert_eq!(out, Outcome::Committed { synced: false }); assert!(read_layout(&e).unwrap().decorations.contains(&d)); }
            Some(libc::EEXIST) => { assert!(!postcommit); assert!(matches!(out, Outcome::NotCommitted(_))); assert_eq!(slice8a_snapshot(&f.0), before); }
            _ => { assert!(!postcommit); assert!(matches!(out, Outcome::Ambiguous(_))); assert_eq!(slice8a_snapshot(&f.0), before); }
        }
        println!("SLICE8B rename-injection={code:?} stages={stages:?} outcome={out:?} attempts=1");
    }
}
