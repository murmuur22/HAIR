// Supplemental protocol-shape tests, still using only owned disposable fixtures.
#[test]
fn slice8a_inventory_order_is_independent_of_writer_creation_order() {
    let first = Fixture::new(); let second = Fixture::new();
    let mut records = vec![];
    for n in [3, 1, 2] {
        let mut d = slice5a_decoration([n as f32, 0.]); d.placement_id = format!("{n:032x}"); records.push(d);
    }
    let mut semantics = vec![];
    for (fixture, reversed) in [(&first, false), (&second, true)] {
        let e = Explorer::new(&fixture.0).unwrap();
        let order: Vec<_> = if reversed { records.iter().rev().collect() } else { records.iter().collect() };
        for d in order { assert_eq!(place_decoration_checked(&e, d, |_| Ok(())), Outcome::Committed { synced: true }); }
        let mut pending = slice5a_decoration([0., 0.]); pending.placement_id = "00000000000000000000000000000004".into();
        assert_eq!(mac::place_decoration(&e, &pending, &mut |_| Ok(()), |at| if at == 1 { Err("prepared refusal".into()) } else { Ok(()) }), Outcome::NotCommitted("prepared refusal".into()));
        let before = slice8a_snapshot(&fixture.0); let i = inspect_placements(&e).unwrap();
        semantics.push(slice8a_semantics(&i)); slice8a_cold(&fixture.0, &i); assert_eq!(slice8a_snapshot(&fixture.0), before);
    }
    assert_eq!(semantics[0], semantics[1], "creation/enumeration order must not change identity/state/status/layout");
}
#[test]
fn slice8a_pending_pin_is_opaque_but_final_active_and_foreign_shapes_fail_closed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for kind in ["opaque-binary", "pending-oversize", "asset-oversize", "pending-symlink", "pending-hardlink", "pending-xattr", "slot-xattr", "no-asset", "mixed-final", "malformed-final", "active-reference", "malformed-active-final"] {
        let (f, e, theme) = slice6a1_saved(); let source = select_pin_asset(&theme, "chair").unwrap();
        let d = slice6a_new(&theme);
        let mut reached = false;
        let stage = if kind == "malformed-active-final" { 2 } else { 5 };
        let out = mac::publish_pin(&e, &source, &d, &mut |_| Ok(()), |at| {
            if at == stage { reached = true; Err("demonstrated pin boundary".into()) } else { Ok(()) }
        });
        assert!(reached, "{kind} did not reach stage {stage}");
        if stage == 2 { assert_eq!(out, Outcome::Committed { synced: false }); }
        else { assert_eq!(out, Outcome::NotCommitted("demonstrated pin boundary".into())); }
        let closure = slice6a_closure(&f); let pending = closure.join("closure.pending.toml");
        match kind {
            "opaque-binary" => std::fs::write(&pending, [0xff, 0x00, 0xfe]).unwrap(),
            "pending-oversize" => std::fs::OpenOptions::new().write(true).open(&pending).unwrap().set_len(4097).unwrap(),
            "asset-oversize" => std::fs::OpenOptions::new().write(true).open(closure.join("asset.glb")).unwrap().set_len(super::pinned::GLB_BYTES + 1).unwrap(),
            "pending-symlink" => { std::fs::rename(&pending, f.0.join("evidence")).unwrap(); symlink(f.0.join("evidence"), &pending).unwrap(); }
            "pending-hardlink" => std::fs::hard_link(&pending, f.0.join("evidence")).unwrap(),
            "pending-xattr" => slice6a_tag(&pending), "slot-xattr" => slice6a_tag(&closure),
            "no-asset" => std::fs::rename(closure.join("asset.glb"), f.0.join("evidence")).unwrap(),
            "mixed-final" => {
                // Three children exceed the existing two-child cap immediately.
                let final_path = closure.join("closure.toml"); std::fs::write(&final_path, source.descriptor.text().unwrap()).unwrap();
                std::fs::set_permissions(final_path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            "malformed-final" => std::fs::rename(&pending, closure.join("closure.toml")).unwrap(),
            "malformed-active-final" => std::fs::write(closure.join("closure.toml"), b"[corrupt").unwrap(),
            "active-reference" => {
                let slot = f.0.join(".hair-layout").join(format!("dc-{}", d.placement_id));
                std::fs::create_dir(&slot).unwrap(); std::fs::set_permissions(&slot, std::fs::Permissions::from_mode(0o700)).unwrap();
                let active = slot.join("decoration.toml"); std::fs::write(&active, toml::to_string(&d.pin(&source).unwrap()).unwrap()).unwrap();
                std::fs::set_permissions(active, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            _ => unreachable!(),
        }
        let before = slice8a_snapshot(&f.0);
        if kind == "opaque-binary" {
            let i = inspect_placements(&e).unwrap();
            let a = i.attempts().iter().find(|a| a.kind() == AttemptKind::PinClosure).unwrap();
            assert_eq!(a.state(), AttemptState::DefinitelyInactive); assert!(a.status().contains("opaque"));
            slice8a_cold(&f.0, &i);
        } else { assert!(inspect_placements(&e).is_err(), "{kind} was hidden as recoverable"); }
        assert_eq!(slice8a_snapshot(&f.0), before, "{kind}: no repair or deletion");
    }
}
#[test]
fn slice8a_file_payload_and_record_foreign_bounds_and_identity_are_strict() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for kind in ["payload-hardlink", "payload-symlink", "payload-xattr", "record-xattr", "slot-xattr", "unknown-child", "payload-oversize", "record-mode", "payload-replacement", "invalid-namespace"] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); slice8a_file_crash(&f.0, 3);
        let tx = slice8a_tx(&f); let payload = tx.join("payload"); let record = tx.join("record.toml");
        match kind {
            "payload-hardlink" => std::fs::hard_link(&payload, f.0.join("alias")).unwrap(),
            "payload-symlink" => { std::fs::rename(&payload, f.0.join("evidence")).unwrap(); symlink(f.0.join("evidence"), &payload).unwrap(); }
            "payload-xattr" => slice6a_tag(&payload), "record-xattr" => slice6a_tag(&record), "slot-xattr" => slice6a_tag(&tx),
            "unknown-child" => std::fs::write(tx.join("foreign"), b"preserve").unwrap(),
            "payload-oversize" => std::fs::OpenOptions::new().write(true).open(&payload).unwrap().set_len(crate::local_explorer::MAX_BODY_BYTES + 1).unwrap(),
            "record-mode" => std::fs::set_permissions(record, std::fs::Permissions::from_mode(0o644)).unwrap(),
            "payload-replacement" => {
                let bytes = std::fs::read(&payload).unwrap(); std::fs::rename(&payload, f.0.join("evidence")).unwrap();
                std::fs::write(&payload, bytes).unwrap(); std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            "invalid-namespace" => std::fs::rename(&tx, tx.with_file_name("tx-not-a-storage-identity")).unwrap(),
            _ => unreachable!(),
        }
        let before = slice8a_snapshot(&f.0);
        if kind == "payload-replacement" {
            let i = inspect_placements(&e).unwrap(); assert_eq!(i.attempts()[0].state(), AttemptState::Ambiguous);
            assert_eq!(i.attempts()[0].active_predecessor(), None); slice8a_cold(&f.0, &i);
        } else { assert!(inspect_placements(&e).is_err(), "{kind}"); }
        assert_eq!(slice8a_snapshot(&f.0), before, "{kind}");
    }
}
