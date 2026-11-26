// Adversarial controls use only disposable owned Fixture directories.
fn slice5a_snapshot(root: &std::path::Path) -> Vec<(PathBuf, u64, u32, Vec<u8>)> {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    fn walk(path: &std::path::Path, out: &mut Vec<(PathBuf, u64, u32, Vec<u8>)>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let m = std::fs::symlink_metadata(&path).unwrap();
            let bytes = if m.is_symlink() { std::fs::read_link(&path).unwrap().as_os_str().as_bytes().to_vec() }
                else if m.is_file() { std::fs::read(&path).unwrap() } else { vec![] };
            out.push((path.clone(), m.ino(), m.mode(), bytes));
            if m.is_dir() { walk(&path, out); }
        }
    }
    let mut out = vec![]; walk(root, &mut out); out.sort_by(|a, b| a.0.cmp(&b.0)); out
}
fn slice5a_saved() -> (Fixture, Explorer, Decoration, Decoration) {
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    let d = slice5a_decoration([-2., 0.]);
    assert_eq!(place_decoration_checked(&e, &d, |_| Ok(())), Outcome::Committed { synced: true });
    let mut proposed = d.clone(); proposed.at = [2., 0.];
    (f, e, d, proposed)
}
#[test]
fn slice5a_noop_stale_missing_and_semantic_changes_refuse_without_writes() {
    for kind in ["noop", "stale-pose", "stale-collision", "missing", "identity", "tag", "size", "nan"] {
        let (f, e, d, mut proposed) = slice5a_saved();
        let mut expected = d.clone();
        let message = match kind {
            "noop" => { proposed = d.clone(); "unchanged" }
            "stale-pose" => { expected.yaw = 0.4; "Stale expected" }
            "stale-collision" => { expected.collision = false; "Stale expected" }
            "missing" => { expected.placement_id = "ffffffffffffffffffffffffffffffff".into(); proposed.placement_id = expected.placement_id.clone(); "missing" }
            "identity" => { proposed.placement_id = "ffffffffffffffffffffffffffffffff".into(); "preserve placement identity" }
            "tag" => { proposed.tag = "hair.storage.shelf".into(); "preserve placement identity" }
            "size" => { proposed.size[0] = 1.; "preserve placement identity" }
            "nan" => { proposed.yaw = f32::NAN; "Invalid Follow-theme" }
            _ => unreachable!(),
        };
        let before = slice5a_snapshot(&f.0);
        let out = relocate_decoration_checked(&e, &expected, &proposed, |_| panic!("invalid request reached validator"));
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains(message)), "{kind}: {out:?}");
        assert_eq!(slice5a_snapshot(&f.0), before, "{kind}");
    }
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    let d = slice5a_decoration([0., 0.]); let mut proposed = d.clone(); proposed.at = [1., 0.];
    assert!(matches!(relocate_decoration_checked(&e, &d, &proposed, |_| Ok(())), Outcome::NotCommitted(_)));
    assert!(std::fs::read_dir(&f.0).unwrap().next().is_none(), "missing layout must not be created");
}
#[test]
fn slice5a_busy_lock_refuses_without_writes() {
    let (f, e, d, proposed) = slice5a_saved();
    let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
    let _held = mac::HeldFlock::acquire(lock, libc::LOCK_EX).unwrap();
    let before = slice5a_snapshot(&f.0);
    let out = relocate_decoration_checked(&e, &d, &proposed, |_| panic!("busy reached validator"));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("busy")), "{out:?}");
    assert_eq!(slice5a_snapshot(&f.0), before);
}
#[test]
fn slice5a_duplicate_identity_refuses_candidate_and_actual_namespace_alias() {
    let (f, e, d, proposed) = slice5a_saved();
    let mut layout = read_layout(&e).unwrap(); layout.decorations.push(d.clone());
    assert_eq!(layout.relocating(&d, &proposed).unwrap_err(), "Duplicate decoration placement identity");
    assert_eq!(crate::placement_scene::validate_decoration_candidate(&e, &layout, [0., 2.], 0).unwrap_err(), "Duplicate decoration placement identity");
    // A second slot cannot legitimately bear the same id: pathname is strict,
    // not revision ordering. This owned alias must fail actual disk reconciliation.
    use std::os::unix::fs::PermissionsExt;
    let alias = f.0.join(".hair-layout/dc-ffffffffffffffffffffffffffffffff");
    std::fs::create_dir(&alias).unwrap();
    std::fs::set_permissions(&alias, std::fs::Permissions::from_mode(0o700)).unwrap();
    let record = alias.join("decoration.toml"); std::fs::write(&record, toml::to_string(&d).unwrap()).unwrap();
    std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = slice5a_snapshot(&f.0);
    let out = relocate_decoration_checked(&e, &d, &proposed, |_| panic!("duplicate reached validator"));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("identity/namespace mismatch")), "{out:?}");
    assert_eq!(slice5a_snapshot(&f.0), before);
}
#[test]
fn slice5a_malformed_foreign_symlink_and_incomplete_records_refuse_without_repair() {
    use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};
    for kind in ["malformed", "unknown", "mode", "hardlink", "xattr", "provenance", "acl", "record-link", "slot-link", "root-link", "pending", "foreign-child"] {
        let (f, e, d, proposed) = slice5a_saved();
        let record = slice5a_record(&f, &d); let slot = record.parent().unwrap();
        match kind {
            "malformed" => std::fs::write(&record, b"[broken").unwrap(),
            "unknown" => std::fs::write(&record, format!("{}\nforeign=true\n", toml::to_string(&d).unwrap())).unwrap(),
            "mode" => std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o644)).unwrap(),
            "hardlink" => std::fs::hard_link(&record, f.0.join("alias")).unwrap(),
            "xattr" | "provenance" => {
                let held = std::fs::File::open(&record).unwrap();
                let attr = if kind == "xattr" { c"org.hair.fixture" } else { c"com.apple.provenance" };
                assert_eq!(unsafe { libc::fsetxattr(held.as_raw_fd(), attr.as_ptr(), b"foreign".as_ptr().cast(), 7, 0, 0) }, 0,
                    "synthetic fixture tagging refused: {}", std::io::Error::last_os_error());
            }
            "acl" => {
                let id = std::process::Command::new("/usr/bin/id").arg("-un").output().unwrap(); assert!(id.status.success());
                let user = String::from_utf8(id.stdout).unwrap();
                assert!(std::process::Command::new("/bin/chmod").args(["+a", &format!("user:{} allow read", user.trim())]).arg(&record).status().unwrap().success());
            }
            "record-link" | "slot-link" | "root-link" => {
                let path = match kind { "record-link" => record.clone(), "slot-link" => slot.to_path_buf(), _ => f.0.join(".hair-layout") };
                let displaced = f.0.join("displaced"); std::fs::rename(&path, &displaced).unwrap();
                std::os::unix::fs::symlink(&displaced, &path).unwrap();
            }
            "pending" => {
                let p = slot.join("pending.toml"); std::fs::write(&p, b"[interrupted").unwrap();
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            "foreign-child" => std::fs::write(slot.join("foreign"), b"untouched").unwrap(),
            _ => unreachable!(),
        }
        let before = slice5a_snapshot(&f.0);
        let message = match kind {
            "malformed" | "unknown" => "Invalid decoration metadata",
            "mode" | "hardlink" | "acl" => "Foreign/shared layout storage",
            "xattr" | "provenance" => "Foreign or oversized decoration metadata",
            "record-link" => "not a regular file",
            "slot-link" | "root-link" => "open placement entry",
            "pending" => "recovery",
            "foreign-child" => "Foreign decoration storage child",
            _ => unreachable!(),
        };
        let out = relocate_decoration_checked(&e, &d, &proposed, |_| panic!("foreign reached validator: {kind}"));
        assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains(message)), "{kind}: {out:?}");
        assert_eq!(slice5a_snapshot(&f.0), before, "{kind}");
        if kind == "pending" {
            let layout = read_layout(&e).unwrap(); assert_eq!(layout.decorations, vec![d]); assert_eq!(layout.incomplete_records, 1);
        }
    }
}
#[test]
fn slice5a_late_validator_refusal_preserves_old_bytes_and_retains_pending() {
    let (f, e, d, proposed) = slice5a_saved();
    let before = slice4c1_record_snapshot(&f.0); let mut calls = 0;
    let out = relocate_decoration_checked(&e, &d, &proposed, |_| {
        calls += 1;
        assert_eq!(slice4c1_record_snapshot(&f.0), before, "precommit must retain exact old inode and bytes");
        if calls == 2 { Err("late checked refusal".into()) } else { Ok(()) }
    });
    assert_eq!(calls, 2); assert_eq!(out, Outcome::NotCommitted("late checked refusal".into()));
    let reopened = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
    assert_eq!(reopened.decorations, vec![d.clone()]); assert_eq!(reopened.incomplete_records, 1);
    let pending = slice5a_record(&f, &d).with_file_name("pending.toml");
    assert_eq!(std::fs::read_to_string(pending).unwrap(), toml::to_string(&proposed).unwrap());
    let snapshot = slice5a_snapshot(&f.0);
    assert!(matches!(relocate_decoration_checked(&e, &d, &proposed, |_| panic!("unfinished attempt reused")), Outcome::NotCommitted(ref s) if s.contains("recovery")));
    assert_eq!(slice5a_snapshot(&f.0), snapshot);
}
#[test]
fn slice5a_completed_rebindings_and_final_callback_edits_refuse_activation() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for phase in [1, 2] {
        for target in ["root", "lock", "slot", "source", "pending", "source-bytes", "pending-bytes"] {
            let (f, e, d, proposed) = slice5a_saved();
            let record = slice5a_record(&f, &d); let slot = record.parent().unwrap().to_path_buf();
            let root = f.0.join(".hair-layout"); let pending = slot.join("pending.toml");
            let old_bytes = std::fs::read(&record).unwrap();
            let old = std::fs::File::open(&record).unwrap();
            let edit = || {
                let path = match target { "root" => root.clone(), "lock" => root.join("lock"), "slot" => slot.clone(), "source" | "source-bytes" => record.clone(), _ => pending.clone() };
                if target.ends_with("-bytes") {
                    let mut changed = if target == "source-bytes" { d.clone() } else { proposed.clone() };
                    changed.yaw = 0.3; std::fs::write(&path, toml::to_string(&changed).unwrap()).unwrap();
                } else {
                    let displaced = f.0.join("displaced"); std::fs::rename(&path, &displaced).unwrap();
                    if target == "root" || target == "slot" {
                        std::fs::create_dir(&path).unwrap(); std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
                    } else {
                        // Byte-identical replacement still must fail binding, not
                        // accidentally pass merely because its TOML matches.
                        let text = if target == "source" { old_bytes.clone() } else if target == "pending" { toml::to_string(&proposed).unwrap().into_bytes() } else { vec![] };
                        std::fs::write(&path, text).unwrap(); std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                    }
                }
                slice5a_snapshot(&f.0)
            };
            let mut at_barrier = None; let mut at_callback = None; let mut calls = 0;
            let out = mac::relocate_decoration(&e, &d, &proposed, &mut |_| {
                calls += 1;
                if phase == 2 && calls == 2 { at_callback = Some(edit()); }
                Ok(())
            }, |stage| {
                if phase == 1 && stage == 1 { at_barrier = Some(edit()); }
                Ok(())
            });
            assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("changed") || s.contains("missing") || s.contains("Stale expected")), "phase={phase} {target}: {out:?}");
            assert_eq!(slice5a_snapshot(&f.0), at_callback.or(at_barrier).unwrap(), "refusal must not clean up or overwrite tampered evidence");
            if target != "source-bytes" {
                let mut held = &old; use std::io::{Read, Seek}; held.rewind().unwrap();
                let mut bytes = vec![]; held.read_to_end(&mut bytes).unwrap(); assert_eq!(bytes, old_bytes);
            }
            assert!(old.metadata().unwrap().ino() != 0);
        }
    }
}
#[test]
fn slice5a_precommit_and_postcommit_failure_seams_reopen_truthfully() {
    for stage in 0..=2 {
        let (f, e, d, proposed) = slice5a_saved();
        let old = slice4c1_record_snapshot(&f.0);
        let out = mac::relocate_decoration(&e, &d, &proposed, &mut |_| Ok(()), |at|
            if at == stage { Err("injected boundary refusal".into()) } else { Ok(()) });
        let reopened = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
        if stage == 2 { assert_eq!(out, Outcome::Committed { synced: false }); assert_eq!(reopened.decorations, vec![proposed]); }
        else {
            assert_eq!(out, Outcome::NotCommitted("injected boundary refusal".into())); assert_eq!(reopened.decorations, vec![d]);
            assert_eq!(slice4c1_record_snapshot(&f.0), old);
        }
        assert_eq!(reopened.incomplete_records, usize::from(stage == 1));
    }
    // Classification is the real shared activation-error seam, not a claim of
    // actual ENOSPC/EIO. Neither definite nor uncertain errors trigger cleanup.
    for code in [libc::EACCES, libc::ENOSPC, libc::EEXIST] {
        assert!(matches!(mac::decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::NotCommitted(_)));
    }
    for code in [libc::EIO, libc::EINTR, libc::ETIMEDOUT, libc::ESTALE] {
        assert!(matches!(mac::decoration_rename_failure(std::io::Error::from_raw_os_error(code)), Outcome::Ambiguous(_)));
    }
}
#[test]
fn slice5a_relocation_at_shared_256_record_bound_needs_no_extra_slot() {
    use std::os::unix::fs::PermissionsExt;
    let (f, e, d, proposed) = slice5a_saved();
    let root = f.0.join(".hair-layout");
    for i in 0..254 {
        let mut other = d.clone(); other.placement_id = format!("{i:032x}");
        let slot = root.join(format!("dc-{}", other.placement_id)); std::fs::create_dir(&slot).unwrap();
        std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = slot.join("decoration.toml"); std::fs::write(&path, toml::to_string(&other).unwrap()).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert_eq!(publish_checked(&e, OsStr::new("saved.txt"), b"owned source", &pos(), &AtomicBool::new(false), |_| Ok(())), Outcome::Committed { synced: true });
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 257);
    let before = slice4c1_record_snapshot(&f.0);
    assert_eq!(relocate_decoration_checked(&e, &d, &proposed, |candidate| {
        assert_eq!(candidate.decorations.len(), 255); assert_eq!(candidate.positions.len(), 1); Ok(())
    }), Outcome::Committed { synced: true });
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 257);
    let after = slice4c1_record_snapshot(&f.0);
    for record in before { if record.0 != slice5a_record(&f, &d) { assert!(after.contains(&record)); } }
    let extra = slice5a_decoration([4., 0.]); let snapshot = slice5a_snapshot(&f.0);
    assert!(matches!(place_decoration_checked(&e, &extra, |_| Ok(())), Outcome::NotCommitted(ref s) if s.contains("256 shared slots")));
    assert_eq!(slice5a_snapshot(&f.0), snapshot);
    // Above the inherited cap even same-slot relocation must refuse enumeration.
    let overflow = root.join("dc-ffffffffffffffffffffffffffffffff"); std::fs::create_dir(overflow).unwrap();
    let snapshot = slice5a_snapshot(&f.0);
    assert!(matches!(relocate_decoration_checked(&e, &proposed, &d, |_| Ok(())), Outcome::NotCommitted(ref s) if s.contains("limit")));
    assert_eq!(slice5a_snapshot(&f.0), snapshot);
}
