// Included inside placement_store::tests, using only owned disposable Fixture roots.
#[test]
fn slice2_library_reposition_preserves_bytes_identity_and_old_record_then_rejects_stale_target() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    std::fs::write(f.0.join("File.txt"), b"existing bytes").unwrap();
    let explorer = Explorer::new(&f.0).unwrap();
    let target = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
    assert!(!f.0.join(".hair-layout").exists());
    let before = std::fs::metadata(f.0.join("File.txt")).unwrap();
    assert!(matches!(
        reposition_checked(&explorer, &target, &pos(), |_| Ok(())),
        Outcome::Committed { .. }
    ));
    let layout = read_layout(&explorer).unwrap();
    assert_eq!(layout.positions, vec![("File.txt".into(), pos())]);
    assert_eq!(
        layout.retained_slots,
        vec![std::ffi::OsString::from("File.txt")]
    );
    let record = std::fs::read_dir(f.0.join(".hair-layout"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.is_dir())
        .unwrap()
        .join("record.toml");
    let record_bytes = std::fs::read(&record).unwrap();
    assert!(matches!(
        reposition_checked(&explorer, &target, &pos(), |_| Ok(())),
        Outcome::NotCommitted(_)
    ));
    let next = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
    let mut position = pos();
    position.at = [3.0, 1.0];
    assert!(matches!(
        reposition_checked(&explorer, &next, &position, |_| Ok(())),
        Outcome::Committed { .. }
    ));
    assert_eq!(
        read_layout(&explorer).unwrap().positions,
        vec![("File.txt".into(), position)]
    );
    assert_eq!(
        std::fs::read(&record).unwrap(),
        record_bytes,
        "old committed metadata never truncated"
    );
    let after = std::fs::metadata(f.0.join("File.txt")).unwrap();
    assert_eq!(
        (before.dev(), before.ino(), before.created().unwrap()),
        (after.dev(), after.ino(), after.created().unwrap())
    );
    assert_eq!(
        std::fs::read(f.0.join("File.txt")).unwrap(),
        b"existing bytes"
    );
}
#[test]
fn slice2_library_reposition_refuses_foreign_malformed_and_rebound_storage() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for corruption in 0..7 {
        let f = Fixture::new();
        std::fs::write(f.0.join("File.txt"), b"original").unwrap();
        let explorer = Explorer::new(&f.0).unwrap();
        let first = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
        assert!(matches!(
            reposition_checked(&explorer, &first, &pos(), |_| Ok(())),
            Outcome::Committed { .. }
        ));
        let target = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
        let root = f.0.join(".hair-layout");
        let tx = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap();
        let record = tx.join("record.toml");
        let token = mac::validate_existing(&explorer, &target);
        assert!(token.is_ok());
        match corruption {
            0 => std::fs::write(&record, b"malformed metadata").unwrap(),
            1 => {
                std::fs::rename(&record, tx.join("record-held")).unwrap();
                symlink(tx.join("record-held"), &record).unwrap();
            }
            2 => std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap(),
            3 => {
                std::fs::rename(f.0.join("File.txt"), f.0.join("held-original")).unwrap();
                std::fs::write(f.0.join("File.txt"), b"rebound").unwrap();
            }
            4 => std::fs::write(tx.join("foreign"), b"foreign evidence").unwrap(),
            5 => std::fs::hard_link(&record, tx.join("shared")).unwrap(),
            _ => std::fs::write(&record, vec![b'a'; 4097]).unwrap(),
        }
        let preserved = std::fs::read(&record).unwrap();
        let outcome = reposition_checked(&explorer, &target, &pos(), |_| Ok(()));
        assert!(
            matches!(outcome, Outcome::NotCommitted(_)),
            "corruption {corruption}: {outcome:?}"
        );
        assert_eq!(std::fs::read(&record).unwrap(), preserved);
        if corruption != 3 {
            assert!(
                validate_existing(&explorer, &target).is_ok(),
                "old activation retained"
            );
        }
    }
}
#[test]
fn slice2_library_precommit_rebinding_and_tamper_never_activate_or_truncate_foreign_bytes() {
    for mutation in 0..5 {
        let f = Fixture::new();
        std::fs::write(f.0.join("File.txt"), b"original bytes").unwrap();
        let explorer = Explorer::new(&f.0).unwrap();
        let target = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
        let root = f.0.join(".hair-layout");
        let mut foreign = None;
        let outcome = mac::reposition(&explorer, &target, &pos(), &mut |_| Ok(()), |stage| {
            if stage != 1 {
                return Ok(());
            }
            let tx = std::fs::read_dir(&root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.is_dir())
                .unwrap();
            match mutation {
                0 => {
                    let p = tx.join("record.toml");
                    std::fs::write(&p, b"foreign record").unwrap();
                    foreign = Some(p);
                }
                1 => {
                    let p = tx.join("record.toml");
                    std::fs::rename(&p, tx.join("held-record")).unwrap();
                    std::fs::write(&p, b"foreign record").unwrap();
                    foreign = Some(p);
                }
                2 => {
                    std::fs::rename(&root, f.0.join("held-root")).unwrap();
                    std::fs::create_dir(&root).unwrap();
                    let p = root.join("foreign");
                    std::fs::write(&p, b"foreign record").unwrap();
                    foreign = Some(p);
                }
                3 => {
                    std::fs::rename(root.join("lock"), root.join("held-lock")).unwrap();
                    let p = root.join("lock");
                    std::fs::write(&p, b"foreign record").unwrap();
                    foreign = Some(p);
                }
                _ => {
                    std::fs::rename(f.0.join("File.txt"), f.0.join("held-source")).unwrap();
                    std::fs::write(f.0.join("File.txt"), b"replacement").unwrap();
                }
            }
            Ok(())
        });
        assert!(
            matches!(outcome, Outcome::NotCommitted(_)),
            "{mutation}: {outcome:?}"
        );
        if let Some(p) = foreign {
            assert_eq!(std::fs::read(p).unwrap(), b"foreign record");
        }
        if mutation != 4 {
            assert!(validate_existing(&explorer, &target).is_ok());
        }
        assert_eq!(
            std::fs::read(if mutation == 4 {
                f.0.join("held-source")
            } else {
                f.0.join("File.txt")
            })
            .unwrap(),
            b"original bytes"
        );
    }
}
#[test]
fn slice2_library_reposition_lock_and_postcommit_sync_outcome_are_not_rollback() {
    use std::os::fd::AsRawFd;
    let f = Fixture::new();
    std::fs::write(f.0.join("File.txt"), b"original").unwrap();
    let explorer = Explorer::new(&f.0).unwrap();
    let target = select_existing(&explorer, OsStr::new("File.txt")).unwrap();
    let outcome = mac::reposition(&explorer, &target, &pos(), &mut |_| Ok(()), |stage| {
        if stage == 1 {
            let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
            assert_ne!(
                unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0
            );
            assert!(read_layout(&explorer).is_err());
        }
        if stage == 2 {
            Err("simulated postcommit sync failure".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(outcome, Outcome::Committed { synced: false });
    assert_eq!(
        read_layout(&explorer).unwrap().positions,
        vec![("File.txt".into(), pos())]
    );
    assert!(matches!(
        reposition_checked(&explorer, &target, &pos(), |_| Ok(())),
        Outcome::NotCommitted(_)
    ));
}
