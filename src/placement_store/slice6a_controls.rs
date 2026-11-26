fn slice6a_new(theme: &crate::theme::Theme) -> Decoration {
    Decoration::from_entry(theme.decorations.entry("chair").unwrap(), [-2., 0.], 0., [0.8; 3]).unwrap()
}
fn slice6a_closure(f: &Fixture) -> PathBuf {
    std::fs::read_dir(f.0.join(".hair-layout")).unwrap().map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_str().unwrap().starts_with("ac-")).unwrap()
}
fn slice6a_tag(path: &std::path::Path) {
    use std::os::fd::AsRawFd;
    let file = std::fs::File::open(path).unwrap();
    assert_eq!(unsafe { libc::fsetxattr(file.as_raw_fd(), c"org.hair.slice6a-fixture".as_ptr(), b"preserve".as_ptr().cast(), 8, 0, libc::XATTR_CREATE) }, 0,
        "fixture attribute refused: {}", std::io::Error::last_os_error());
}
fn slice6a_acl(path: &std::path::Path) {
    let id = std::process::Command::new("/usr/bin/id").arg("-un").output().unwrap();
    assert!(id.status.success());
    let user = String::from_utf8(id.stdout).unwrap();
    assert!(std::process::Command::new("/bin/chmod").args(["+a", &format!("user:{} allow read", user.trim())]).arg(path).status().unwrap().success());
}
#[test]
fn slice6a_source_selection_refuses_unsafe_symlink_hardlink_foreign_acl_and_malformed() {
    use std::os::unix::fs::{symlink, PermissionsExt, MetadataExt};
    for kind in ["relative", "absolute", "empty-component", "dot-component", "symlink", "ancestor-symlink", "hardlink", "xattr", "acl", "mode", "oversize", "truncated", "malformed", "forged-fit", "forged-entry"] {
        let f = Fixture::new(); let (pack, mut theme) = slice6a_pack(&f);
        let path = pack.join("chair.glb");
        let bytes = std::fs::read(&path).unwrap();
        let ino = std::fs::metadata(&path).unwrap().ino();
        match kind {
            "relative" => theme.decorations.assets[0].spec.file = "../chair.glb".into(),
            "absolute" => theme.decorations.assets[0].spec.file = path.to_str().unwrap().into(),
            "empty-component" => theme.decorations.assets[0].spec.file = "sub//chair.glb".into(),
            "dot-component" => theme.decorations.assets[0].spec.file = "./chair.glb".into(),
            "symlink" => { symlink("chair.glb", pack.join("alias.glb")).unwrap(); theme.decorations.assets[0].spec.file = "alias.glb".into(); }
            "ancestor-symlink" => { symlink(".", pack.join("alias")).unwrap(); theme.decorations.assets[0].spec.file = "alias/chair.glb".into(); }
            "hardlink" => std::fs::hard_link(&path, pack.join("link.glb")).unwrap(),
            "xattr" => slice6a_tag(&path),
            "acl" => slice6a_acl(&path),
            "mode" => std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap(),
            "oversize" => std::fs::OpenOptions::new().write(true).open(&path).unwrap().set_len(8_000_001).unwrap(),
            "truncated" => std::fs::write(&path, &bytes[..bytes.len()/2]).unwrap(),
            "malformed" => std::fs::write(&path, b"not a real GLB").unwrap(),
            "forged-fit" => theme.decorations.assets[0].spec.yaw_degrees = 90.,
            "forged-entry" => theme.decorations.entries[0].tag = "hair.forged".into(),
            _ => unreachable!(),
        }
        let before = std::fs::read(&path).unwrap();
        assert!(select_pin_asset(&theme, "chair").is_err(), "{kind}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{kind}");
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), ino, "{kind}");
        assert!(!f.0.join(".hair-layout").exists(), "{kind}");
    }
}
#[test]
fn slice6a_stale_selected_source_refuses_before_copy_and_after_last_callback() {
    for phase in [0, 1, 2] {
        for change in ["bytes", "same-bytes-new-inode", "hardlink", "xattr", "pack-rename", "manifest"] {
            let f = Fixture::new(); let (pack, theme) = slice6a_pack(&f);
            let source = select_pin_asset(&theme, "chair").unwrap();
            let d = slice6a_new(&theme); let e = Explorer::new(&f.0).unwrap();
            let bytes = std::fs::read(pack.join("chair.glb")).unwrap();
            let mutate = || match change {
                "bytes" => std::fs::write(pack.join("chair.glb"), b"stale fixture").unwrap(),
                "same-bytes-new-inode" => { std::fs::write(pack.join("replacement"), &bytes).unwrap(); std::fs::rename(pack.join("replacement"), pack.join("chair.glb")).unwrap(); }
                "hardlink" => std::fs::hard_link(pack.join("chair.glb"), pack.join("alias")).unwrap(),
                "xattr" => slice6a_tag(&pack.join("chair.glb")),
                "pack-rename" => std::fs::rename(&pack, f.0.join("moved-pack")).unwrap(),
                "manifest" => std::fs::write(pack.join("pack.toml"), b"stale manifest").unwrap(),
                _ => unreachable!(),
            };
            if phase == 0 { mutate(); }
            let mut calls = 0;
            let out = publish_pin_checked(&e, &source, &d, |_| { calls += 1; if calls == phase { mutate(); } Ok(()) });
            assert!(matches!(out, Outcome::NotCommitted(_)), "{phase}/{change}: {out:?}");
            assert!(read_layout(&e).unwrap().decorations.is_empty());
            if phase == 0 { assert!(!f.0.join(".hair-layout").exists()); }
        }
    }
}
#[test]
fn slice6a_storage_foreign_malformed_truncated_and_unbound_records_fail_closed() {
    use std::os::unix::fs::{symlink, PermissionsExt, MetadataExt};
    for kind in ["asset-symlink", "asset-hardlink", "descriptor-hardlink", "asset-xattr", "descriptor-xattr", "slot-xattr", "root-xattr", "asset-acl", "asset-mode", "extra", "oversize", "truncated", "malformed", "unknown-schema", "unsafe-file", "unknown-record", "wrong-ref", "missing", "valid-hash-invalid-glb", "unknown-pin-field"] {
        let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let d = slice6a_new(&theme);
        let e = Explorer::new(&f.0).unwrap();
        assert_eq!(pin_decoration_checked(&e, &theme, "chair", &d, |_| Ok(())), Outcome::Committed { synced: true });
        let closure = slice6a_closure(&f); let asset = closure.join("asset.glb"); let descriptor = closure.join("closure.toml");
        let record = slice5a_record(&f, &d);
        let asset_bytes = std::fs::read(&asset).unwrap();
        match kind {
            "asset-symlink" => { std::fs::rename(&asset, f.0.join("foreign.glb")).unwrap(); symlink(f.0.join("foreign.glb"), &asset).unwrap(); }
            "asset-hardlink" => std::fs::hard_link(&asset, f.0.join("foreign.glb")).unwrap(),
            "descriptor-hardlink" => std::fs::hard_link(&descriptor, f.0.join("foreign.toml")).unwrap(),
            "asset-xattr" => slice6a_tag(&asset), "descriptor-xattr" => slice6a_tag(&descriptor),
            "slot-xattr" => slice6a_tag(&closure), "root-xattr" => slice6a_tag(&f.0.join(".hair-layout")),
            "asset-acl" => slice6a_acl(&asset),
            "asset-mode" => std::fs::set_permissions(&asset, std::fs::Permissions::from_mode(0o644)).unwrap(),
            "extra" => std::fs::write(closure.join("foreign"), b"preserve").unwrap(),
            "oversize" => std::fs::OpenOptions::new().write(true).open(&asset).unwrap().set_len(8_000_001).unwrap(),
            "truncated" => std::fs::write(&asset, &asset_bytes[..asset_bytes.len()/2]).unwrap(),
            "malformed" => std::fs::write(&descriptor, b"version = [").unwrap(),
            "unknown-schema" => { let text = std::fs::read_to_string(&descriptor).unwrap().replace("version = 1", "version = 99"); std::fs::write(&descriptor, text).unwrap(); }
            "unsafe-file" => { let text = std::fs::read_to_string(&descriptor).unwrap().replace("asset.glb", "../asset.glb"); std::fs::write(&descriptor, text).unwrap(); }
            "unknown-record" => { let text = std::fs::read_to_string(&record).unwrap().replace("decoration-pinned", "decoration-future"); std::fs::write(&record, text).unwrap(); }
            "wrong-ref" => { let text = std::fs::read_to_string(&record).unwrap(); let id = closure.file_name().unwrap().to_str().unwrap().strip_prefix("ac-").unwrap(); std::fs::write(&record, text.replace(id, &"0".repeat(64))).unwrap(); }
            "missing" => std::fs::rename(&asset, f.0.join("foreign.glb")).unwrap(),
            "unknown-pin-field" => { let text = std::fs::read_to_string(&record).unwrap().replace("[pinned]", "[pinned]\nloaded = {}"); std::fs::write(&record, text).unwrap(); }
            "valid-hash-invalid-glb" => {
                let mut desc: super::pinned::Descriptor = toml::from_str(&std::fs::read_to_string(&descriptor).unwrap()).unwrap();
                let malformed = b"not a GLB despite matching descriptor and filename";
                desc.sha256 = super::pinned::digest(malformed); desc.bytes = malformed.len() as u64;
                std::fs::write(&asset, malformed).unwrap(); std::fs::write(&descriptor, desc.text().unwrap()).unwrap();
                let old = closure.file_name().unwrap().to_str().unwrap().strip_prefix("ac-").unwrap(); let new = desc.identity().unwrap();
                let text = std::fs::read_to_string(&record).unwrap().replace(old, &new); std::fs::write(&record, text).unwrap();
                std::fs::rename(&closure, f.0.join(".hair-layout").join(format!("ac-{new}"))).unwrap();
            }
            _ => unreachable!(),
        }
        let record_before = std::fs::read(&record).unwrap(); let record_ino = std::fs::metadata(&record).unwrap().ino();
        assert!(read_layout(&e).is_err(), "{kind}");
        assert!(matches!(pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(())), Outcome::NotCommitted(_)), "{kind}");
        assert_eq!(std::fs::read(&record).unwrap(), record_before, "{kind}");
        assert_eq!(std::fs::metadata(&record).unwrap().ino(), record_ino, "{kind}");
        if f.0.join("foreign.glb").exists() { assert_eq!(std::fs::read(f.0.join("foreign.glb")).unwrap(), asset_bytes); }
    }
}
#[test]
fn slice6a_stages_busy_recovery_and_confirmed_commit_are_truthful() {
    for stage in [0, 3, 1, 2] {
        let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let source = select_pin_asset(&theme, "chair").unwrap();
        let d = slice6a_new(&theme); let e = Explorer::new(&f.0).unwrap();
        let out = mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |at| if at == stage { Err("injected pin sync/publication failure".into()) } else { Ok(()) });
        let layout = read_layout(&e).unwrap();
        if stage == 2 { assert_eq!(out, Outcome::Committed { synced: false }); assert_eq!(layout.decorations.len(), 1); }
        else { assert!(matches!(out, Outcome::NotCommitted(_))); assert!(layout.decorations.is_empty()); }
        assert_eq!(layout.incomplete_records, match stage { 3 => 1, 1 => 2, _ => 0 });
        if stage == 1 || stage == 3 {
            let before = slice4c1_record_snapshot(&f.0);
            assert!(matches!(publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| Ok(())), Outcome::NotCommitted(_)));
            assert_eq!(slice4c1_record_snapshot(&f.0), before);
        }
    }
    let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let d = slice6a_new(&theme); let e = Explorer::new(&f.0).unwrap();
    assert_eq!(place_decoration_checked(&e, &d, |_| Ok(())), Outcome::Committed { synced: true });
    let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
    let held = mac::HeldFlock::acquire(lock, libc::LOCK_EX).unwrap();
    let before = slice4c1_record_snapshot(&f.0);
    let out = pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(()));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("busy")));
    assert_eq!(slice4c1_record_snapshot(&f.0), before); drop(held);
}
#[test]
fn slice6a_same_closure_shares_one_mesh_and_metadata_only_relocation() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new(); let (pack, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    let source = select_pin_asset(&theme, "chair").unwrap();
    let preview = source.decoration(&slice6a_new(&theme)).unwrap();
    assert!(preview.resolve(&crate::decoration::Catalog::default()).is_ok());
    assert!(!f.0.join(".hair-layout").exists(), "explicit source preview is storage-free");
    for x in [-2., 2.] { let mut d = slice6a_new(&theme); d.at[0] = x; assert_eq!(publish_pin_checked(&e, &source, &d, |_| Ok(())), Outcome::Committed { synced: true }); }
    let layout = read_layout(&e).unwrap(); let empty = crate::decoration::Catalog::default();
    let a = layout.decorations[0].resolve(&empty).unwrap().1;
    let b = layout.decorations[1].resolve(&empty).unwrap().1;
    assert!(std::sync::Arc::ptr_eq(&a.model, &b.model));
    assert_eq!(std::fs::read_dir(f.0.join(".hair-layout")).unwrap().count(), 4);
    let closure = slice6a_closure(&f); let path = closure.join("asset.glb");
    let bytes = std::fs::read(&path).unwrap(); let ino = std::fs::metadata(&path).unwrap().ino();
    let old = &layout.decorations[0]; let mut moved = old.clone(); moved.at[1] += 0.2; moved.collision = !moved.collision;
    std::fs::remove_dir_all(pack).unwrap();
    assert_eq!(relocate_decoration_checked(&e, old, &moved, |_| Ok(())), Outcome::Committed { synced: true });
    assert_eq!(std::fs::read(&path).unwrap(), bytes); assert_eq!(std::fs::metadata(&path).unwrap().ino(), ino);
    let reopened = read_layout(&e).unwrap(); assert!(reopened.decorations.contains(&moved));
    assert!(reopened.decorations.iter().all(|d| d.resolve(&empty).is_ok()));
    let text = toml::to_string(&moved).unwrap(); let unacquired: Decoration = toml::from_str(&text).unwrap();
    assert!(unacquired.resolve(&theme.decorations).is_err(), "parser alone grants no retained asset authority");
    assert!(matches!(place_decoration_checked(&e, &unacquired, |_| Ok(())), Outcome::NotCommitted(_)));
}
#[test]
fn slice6a_exact_resource_budget_boundaries() {
    use super::pinned::Budget;
    let b = Budget { theme_models: 3, pin_models: 0, encoded: 24_000_000, vertices: 300_000, textures: 64,
        texture_bytes: 64_000_000 - 24_000_000 - 300_000 * std::mem::size_of::<crate::assets::Vertex>(), extra_retained: 0 };
    b.validate().unwrap(); // exact shared retained-payload boundary
    assert!(Budget { extra_retained: 1, ..b }.validate().is_err());
    for b in [Budget { theme_models: 4, ..Default::default() }, Budget { encoded: 24_000_001, ..Default::default() },
        Budget { vertices: 300_001, ..Default::default() }, Budget { textures: 65, ..Default::default() },
        Budget { texture_bytes: 32_000_001, ..Default::default() }] { assert!(b.validate().is_err()); }
    Budget { theme_models: 3, encoded: 24_000_000, textures: 64, ..Default::default() }.validate().unwrap();
    Budget { vertices: 300_000, texture_bytes: 32_000_000, ..Default::default() }.validate().unwrap();
}
#[test]
fn slice6a_distinct_duplicate_identities_and_unmatched_closures_spend_model_budget() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); let (pack, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    let original_manifest = std::fs::read_to_string(pack.join("pack.toml")).unwrap();
    let mut cache = Default::default();
    for i in 0..4 {
        std::fs::write(pack.join("pack.toml"), original_manifest.replace("chair-mesh", &format!("chair-mesh-{i}"))).unwrap();
        let theme = crate::theme::Theme::load(&pack.join("pack.toml"), &mut cache).unwrap();
        let d = slice6a_new(&theme);
        let out = pin_decoration_checked(&e, &theme, "chair", &d, |_| Ok(()));
        if i < 3 { assert_eq!(out, Outcome::Committed { synced: true }); }
        else { assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("budget")), "{out:?}"); }
    }
    // Exact bytes are duplicate, but descriptor identities differ: all three count.
    assert_eq!(read_layout(&e).unwrap().decorations.len(), 3);
    // Add a strictly owned unmatched fourth closure using real bytes and descriptor.
    std::fs::write(pack.join("pack.toml"), &original_manifest).unwrap();
    let mut descriptor = select_pin_asset(&theme, "chair").unwrap().descriptor;
    descriptor.asset.id = "unmatched".into();
    let id = descriptor.identity().unwrap(); let slot = f.0.join(".hair-layout").join(format!("ac-{id}"));
    std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
    for (name, bytes) in [("asset.glb", std::fs::read(pack.join("chair.glb")).unwrap()), ("closure.toml", descriptor.text().unwrap().into_bytes())] {
        let p = slot.join(name); std::fs::write(&p, bytes).unwrap(); std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(read_layout(&e).unwrap_err().contains("model/file limit"));
}
#[test]
fn slice6a_shared_slot_budget_at_256_includes_closure_and_each_placement() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    assert_eq!(place_decoration_checked(&e, &slice5a_decoration([2., 0.]), |_| Ok(())), Outcome::Committed { synced: true });
    for i in 0..253 {
        let path = f.0.join(".hair-layout").join(format!("tx-{i:032x}")); std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(())), Outcome::Committed { synced: true });
    let before = slice4c1_record_snapshot(&f.0);
    let out = pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(()));
    assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("256 shared slots")), "{out:?}");
    assert_eq!(slice4c1_record_snapshot(&f.0), before);
    assert_eq!(std::fs::read_dir(f.0.join(".hair-layout")).unwrap().count(), 257);
}

#[test]
fn slice6a_final_callback_rebinding_cannot_activate_a_pin() {
    use std::os::unix::fs::PermissionsExt;
    for target in ["asset", "descriptor", "closure-dir", "pending", "slot", "root", "lock", "foreign-active", "pending-removed"] {
        let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
        let source = select_pin_asset(&theme, "chair").unwrap(); let d = slice6a_new(&theme);
        let root = f.0.join(".hair-layout"); let slot = root.join(format!("dc-{}", d.placement_id));
        let mut calls = 0;
        let out = publish_pin_checked(&e, &source, &d, |_| {
            calls += 1;
            if calls == 2 {
                let closure = slice6a_closure(&f);
                if matches!(target, "asset" | "descriptor" | "pending" | "lock") {
                    let path = match target { "asset" => closure.join("asset.glb"), "descriptor" => closure.join("closure.toml"), "lock" => root.join("lock"), _ => slot.join("pending.toml") };
                    let bytes = std::fs::read(&path).unwrap();
                    std::fs::rename(&path, f.0.join("retained-evidence")).unwrap();
                    std::fs::write(&path, &bytes).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                } else if target == "foreign-active" { std::fs::write(slot.join("decoration.toml"), b"foreign evidence").unwrap(); }
                else if target == "pending-removed" { std::fs::rename(slot.join("pending.toml"), f.0.join("retained-evidence")).unwrap(); }
                else {
                    let path = match target { "closure-dir" => closure, "slot" => slot.clone(), _ => root.clone() };
                    std::fs::rename(&path, f.0.join("retained-evidence")).unwrap();
                    std::fs::create_dir(&path).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            Ok(())
        });
        assert_eq!(calls, 2, "{target}: {out:?}");
        assert!(matches!(out, Outcome::NotCommitted(_)), "{target}: {out:?}");
        if target == "foreign-active" { assert_eq!(std::fs::read(slot.join("decoration.toml")).unwrap(), b"foreign evidence"); }
        else { assert!(!slot.join("decoration.toml").exists(), "{target}"); assert!(f.0.join("retained-evidence").exists()); }
    }
}

#[test]
fn slice6a_shared_staging_plus_closure_byte_budget_exact_boundary() {
    use std::os::unix::fs::PermissionsExt;
    for excess in [0usize, 1] {
        let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
        let source = select_pin_asset(&theme, "chair").unwrap();
        assert_eq!(place_decoration_checked(&e, &slice5a_decoration([2., 0.]), |_| Ok(())), Outcome::Committed { synced: true });
        let mut remaining = crate::inventory::MAX_TOTAL - source.descriptor.bytes as usize + excess;
        let mut i = 0;
        while remaining != 0 {
            let amount = remaining.min(crate::local_explorer::MAX_BODY_BYTES as usize);
            let slot = f.0.join(".hair-layout").join(format!("tx-{i:032x}"));
            std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
            let path = slot.join("payload"); let file = std::fs::File::create(&path).unwrap(); file.set_len(amount as u64).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            remaining -= amount; i += 1;
        }
        let out = publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| Ok(()));
        if excess == 0 { assert_eq!(out, Outcome::Committed { synced: true }); }
        else { assert!(matches!(out, Outcome::NotCommitted(ref s) if s.contains("staging/closure budget")), "{out:?}"); }
    }
}

#[test]
fn decoration_b_reused_closure_held_before_each_callback() {
    use std::os::unix::fs::PermissionsExt;
    for stage in [1, 2] {
        for target in ["asset.glb", "closure.toml"] {
            let f = Fixture::new(); let (_, theme) = slice6a_pack(&f);
            let e = Explorer::new(&f.0).unwrap();
            let source = select_pin_asset(&theme, "chair").unwrap();
            assert_eq!(publish_pin_checked(&e, &source, &slice6a_new(&theme), |_| Ok(())), Outcome::Committed { synced: true });
            let d = slice6a_new(&theme); let mut calls = 0;
            let path = slice6a_closure(&f).join(target);
            let out = publish_pin_checked(&e, &source, &d, |_| {
                calls += 1;
                if calls == stage {
                    let bytes = std::fs::read(&path).unwrap();
                    std::fs::rename(&path, f.0.join("held-evidence")).unwrap();
                    std::fs::write(&path, bytes).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                }
                Ok(())
            });
            assert!(matches!(out, Outcome::NotCommitted(_)), "stage={stage} target={target}: {out:?}");
            assert!(!f.0.join(format!(".hair-layout/dc-{}/decoration.toml", d.placement_id)).exists());
            assert!(f.0.join("held-evidence").exists());
        }
    }
}

#[test]
fn slice6a_pins_and_unmatched_pack_models_share_cold_and_warm_theme_budget() {
    let f = Fixture::new(); let (_, theme) = slice6a_pack(&f); let e = Explorer::new(&f.0).unwrap();
    assert_eq!(pin_decoration_checked(&e, &theme, "chair", &slice6a_new(&theme), |_| Ok(())), Outcome::Committed { synced: true });
    let layout = read_layout(&e).unwrap();
    let (world, mut host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    assert!(crate::placement_scene::apply(&layout, &world, &mut host).unwrap().is_empty());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/archive.toml");
    let mut cache = Default::default();
    for _ in 0..2 {
        // 6A3: three pack sources plus one pin is within separate partitions.
        let refreshed = crate::theme::Theme::load_for_scene(&path, &mut cache, &host).unwrap();
        refreshed.validate_decoration_candidate_layout(&layout).unwrap();
    }
    assert_eq!(cache.loads, 3, "warm admission must charge already-loaded models again, not decode again");
    // Retain cold/warm negative aggregate-resource coverage, not the retired
    // aggregate-three expectation (its original failing run is kept separately).
    let f = Fixture::new(); let (pack, theme) = slice6a2_budget_pack(&f, "encoded", true);
    let source = select_pin_asset(&theme, "chair").unwrap();
    let d = source.decoration(&slice6a_new(&theme)).unwrap();
    let mut host = theme.host.clone(); host.decorations.insert(d.placement_id.clone(), d);
    let mut cache = Default::default();
    for _ in 0..2 {
        assert!(crate::theme::Theme::load_for_scene(&pack.join("pack.toml"), &mut cache, &host).err().unwrap().contains("aggregate whole-pack budget"));
    }
}
