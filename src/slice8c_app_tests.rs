//! Actual App input; all writer refusals and external changes are owned fixtures.
use super::*;
use hair::placement_store::{Decoration, Outcome};
#[path = "slice8c_controls_tests.rs"]
mod controls;
#[path = "slice8c_extra_tests.rs"]
mod extra;
#[path = "storage_boundary_recovery_tests.rs"]
mod boundary;

pub(super) fn click_label(a: &mut App, label: &str) {
    if label == "Pin this asset to this folder" { click(a, place_mode_native::Control::PinAsset); return; }
    let (control, _, _) = a.toolbox_controls().into_iter()
        .find(|(_, _, s)| s.contains(label))
        .unwrap_or_else(|| panic!("missing actual toolbox control: {label}; {}", a.local_status));
    click(a, control);
}
pub(super) fn prepare(kind: &str) -> (Fixture, App, Decoration, Option<Decoration>) {
    prepare_at(kind, [-2., 0.])
}
fn prepare_at(kind: &str, at: [f32; 2]) -> (Fixture, App, Decoration, Option<Decoration>) {
    let (f, mut a) = open_fixture("slice8c-writer");
    start(&mut a, at, true);
    let follow = a.decoration_position().unwrap();
    if kind.contains("pin") { click_label(&mut a, "Pin this asset to this folder"); }
    let mut proposed = a.decoration_position().unwrap();
    let e = &a.local.as_ref().unwrap().explorer;
    let theme = &a.themes[a.theme_index];
    let predecessor = if kind.contains("relocation") {
        let out = if proposed.is_pinned() {
            let source = hair::placement_store::select_pin_asset(theme, &theme.decorations.entries[0].id).unwrap();
            theme.publish_pin_checked(e, &source, &follow, |c| hair::placement_scene::validate_decoration_candidate(e, c, a.pose.position, 0))
        } else { hair::placement_store::place_decoration_checked(e, &proposed, |l| {
            theme.validate_decoration_layout(l, &proposed)?;
            hair::placement_scene::validate_decoration(e, l, &proposed, a.pose.position, 0)
        }) };
        assert_eq!(out, Outcome::Committed { synced: true });
        Some(proposed.clone())
    } else { None };
    if predecessor.is_some() { proposed.at[0] += 0.1; proposed.yaw = 0.2; proposed.collision = false; }
    let mut calls = 0;
    let mut refuse = || { calls += 1; if calls == 2 { Err("owned writer post-preparation refusal".into()) } else { Ok(()) } };
    let out = if let Some(old) = &predecessor {
        hair::placement_store::relocate_decoration_checked(e, old, &proposed, |c| {
            theme.validate_decoration_candidate_layout(c)?;
            hair::placement_scene::validate_decoration_candidate(e, c, a.pose.position, 0)?;
            refuse()
        })
    } else if proposed.is_pinned() {
        let source = hair::placement_store::select_pin_asset(theme, &theme.decorations.entries[0].id).unwrap();
        theme.publish_pin_checked(e, &source, &follow, |c| {
            hair::placement_scene::validate_decoration_candidate(e, c, a.pose.position, 0)?;
            refuse()
        })
    } else { hair::placement_store::place_decoration_checked(e, &proposed, |l| {
        theme.validate_decoration_layout(l, &proposed)?;
        hair::placement_scene::validate_decoration(e, l, &proposed, a.pose.position, 0)?;
        refuse()
    }) };
    assert_eq!(calls, 2, "real writer must reach final validation: {out:?}");
    assert_eq!(out, Outcome::NotCommitted("owned writer post-preparation refusal".into()));
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::F5);
    tap(&mut a, KeyCode::KeyP);
    (f, a, proposed, predecessor)
}

#[test]
fn slice8c_actual_inspector_review_resume_refresh_reopen() {
    for kind in ["follow", "pin", "relocation", "pin-relocation"] {
        let (f, mut a, proposed, old) = prepare(kind);
        let before = records(&f.0);
        let before_attrs = controls::attrs(&f.0);
        click_label(&mut a, "Inspect unfinished placements");
        assert!(a.local_status.contains("inspection"), "{}", a.local_status);
        for _ in 0..256 {
            if a.recovery_status().contains(&format!("exact storage-local ID dc-{}", proposed.placement_id)) { break; }
            tap(&mut a, KeyCode::PageDown);
        }
        click_label(&mut a, "Select attempt");
        assert!(a.place_preview().unwrap().len() > 36, "selected authored mesh, never final envelope");
        assert_eq!(records(&f.0), before, "inspect/select/preview write-free");
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen recovery review"), "{}", a.local_status);
        assert_eq!(records(&f.0), before, "first Enter must not invoke action");
        if let Some(old) = &old { assert_eq!(a.themes[0].host.decorations.get(&old.placement_id), Some(old)); }
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("resumed") && !a.local_status.contains("failed"), "{}", a.local_status);
        let saved = layout(&a);
        assert_eq!(saved.decorations, vec![proposed.clone()]);
        assert_eq!(saved.incomplete_records, 0);
        assert_eq!(a.themes[0].host.decorations.get(&proposed.placement_id), Some(&proposed));
        let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        assert_eq!(layout(&reopened), saved);
        assert_eq!(reopened.themes[0].host.decorations, a.themes[0].host.decorations);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "directory_map_app_tests::place_mode_app_tests::slice4c::recovery::slice8c_cold_app_child", "--ignored", "--nocapture"])
            .env("HAIR_SLICE8C_COLD_ROOT", &f.0)
            .env("HAIR_SLICE8C_COLD_EXPECT", toml::to_string(&proposed).unwrap()).output().unwrap();
        assert!(child.status.success(), "cold actual App failed: {}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
        let pending = f.0.join(format!(".hair-layout/dc-{}/pending.toml", proposed.placement_id));
        let active = pending.with_file_name("decoration.toml");
        let after = records(&f.0);
        assert_eq!(after[&active], before[&pending], "exact prepared inode/bytes become active");
        for (path, value) in &before {
            if *path != pending && *path != active { assert_eq!(after.get(path), Some(value)); }
        }
        let after_attrs = controls::attrs(&f.0);
        for attr in before_attrs {
            if attr.0 != pending && attr.0 != active { assert!(after_attrs.contains(&attr)); }
        }
        println!("SLICE8C actual-input kind={kind} outcome=Committed(synced) refresh=PASS reopen=PASS same-id=once");
    }
}

#[test]
#[ignore = "child-only actual cold App strict reopening of an owned resumed folder"]
fn slice8c_cold_app_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8C_COLD_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let expected: Decoration = toml::from_str(&std::env::var("HAIR_SLICE8C_COLD_EXPECT").unwrap()).unwrap();
    let before = records(&root); let mut a = app(); a.complete_folder(Ok(Some(root.clone()))).unwrap();
    assert_eq!(layout(&a).decorations, vec![expected.clone()]);
    assert_eq!(a.themes[0].host.decorations.get(&expected.placement_id), Some(&expected));
    assert!(a.themes[0].decoration_preview(&layout(&a).decorations[0]).unwrap().len() > 36);
    assert_eq!(records(&root), before);
}
