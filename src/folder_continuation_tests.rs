//! First physical-continuation tracer: synthetic aim + the production App key adapter.
//! No window, GPU, native-human input, page-key shortcut or speculative runtime API.

fn aim_current_portal(app: &mut App, id: &str) {
    let host = &app.themes[app.theme_index].host;
    let a = host
        .anchors
        .iter()
        .find(|a| a.id == id)
        .expect("current portal");
    let n = host.portal_inward(id);
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [a.at[0] + 1.05 * n[0], a.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some(id)
    );
}
fn assert_current_arrival(app: &App, id: &str) {
    let host = &app.themes[app.theme_index].host;
    let a = host
        .anchors
        .iter()
        .find(|a| a.id == id)
        .expect("reciprocal portal");
    let n = host.portal_inward(id);
    for (actual, expected) in app
        .pose
        .position
        .into_iter()
        .zip([a.at[0] + 1.05 * n[0], a.at[2] + 1.05 * n[1]])
    {
        assert!(
            (actual - expected).abs() < 0.01,
            "arrival must be beside exact reciprocal anchor: {:?}",
            app.pose
        );
    }
    assert!(
        app.pose.yaw.sin() * n[0] - app.pose.yaw.cos() * n[1] > 0.99,
        "arrival faces inward"
    );
    assert!(!host.blocked(app.pose.position));
}

#[test]
fn continuation_arrives_beside_reciprocal_door_and_returns() {
    let path =
        std::env::temp_dir().join(format!("hair-paired-continuation-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    for n in 0..25 {
        fs::write(fixture.0.join(format!("File {n:03}.bin")), b"unchanged").unwrap();
    }
    let folder = fixture.0.canonicalize().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(folder.clone()))).unwrap();
    let snapshot = app.local.as_ref().unwrap().explorer.current().clone();
    // Former open-front [11,-5.45]/yaw0 fixture: use the real right-wall door.
    aim_current_portal(&mut app, "folder-next");
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(app.world.folder_view.as_ref().unwrap().page, 1);
    assert_current_arrival(&app, "folder-previous");
    // Turn around in place: no page key or synthetic target assignment.
    app.pose.yaw += std::f32::consts::PI;
    app.pose.pitch = ((1.0_f32 - 1.65) / 1.05).atan();
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    let return_target = app
        .world
        .interaction
        .clone()
        .expect("turning around must reveal an eligible reciprocal doorway");
    assert!(
        !app.world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .contains_key(&return_target.anchor)
    );
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(app.world.folder_view.as_ref().unwrap().page, 0);
    assert_current_arrival(&app, "folder-next");
    assert_eq!(app.local.as_ref().unwrap().explorer.current(), &snapshot);
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        folder
    );
    for n in 0..25 {
        assert_eq!(
            fs::read(fixture.0.join(format!("File {n:03}.bin"))).unwrap(),
            b"unchanged"
        );
    }
}

use super::*;

#[test]
fn physical_parent_returns_to_childs_original_continuation_room() {
    let path = std::env::temp_dir().join(format!("hair-parent-context-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    for n in 0..25 {
        fs::create_dir(fixture.0.join(format!("Child {n:03}"))).unwrap();
    }
    let folder = fixture.0.canonicalize().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(folder.clone()))).unwrap();
    fn use_door(app: &mut App, id: &str) {
        aim_current_portal(app, id);
        app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap();
        app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap();
    }
    use_door(&mut app, "folder-next");
    use_door(&mut app, "folder-next");
    assert_eq!(app.world.folder_view.as_ref().unwrap().page, 2);
    let child_anchor = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.index == 24)
        .unwrap()
        .0
        .clone();
    use_door(&mut app, &child_anchor);
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        folder.join("Child 024")
    );
    use_door(&mut app, "folder-parent");
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        folder
    );
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().page,
        2,
        "physical Parent must restore the room containing the entered child, not the first page"
    );
    let view = app.world.folder_view.as_ref().unwrap();
    assert!(view.bindings.values().any(|b| b.entry.name == "Child 024"));
    let a = app.themes[app.theme_index]
        .host
        .anchors
        .iter()
        .find(|a| a.id == child_anchor)
        .unwrap();
    assert!(
        (app.pose.position[0] - a.at[0]).abs() < 0.1,
        "return beside originating child door"
    );
    assert_current_arrival(&app, &child_anchor);
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 25);
}

use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn e_at_continuation_portal_reaches_second_page_of_same_folder() {
    // Match the existing exclusive, PID-qualified fixture convention. Never reuse
    // an existing directory or enumerate personal folders. TMPDIR is parent-owned.
    let fixture_path =
        std::env::temp_dir().join(format!("hair-physical-continuation-{}", std::process::id()));
    fs::create_dir(&fixture_path).unwrap();
    let fixture = Fixture(fixture_path);
    let mut inputs = Vec::new();
    for index in 0..25 {
        let name = if index == 12 {
            // A second-page raw name must survive without lossy display round trips.
            OsString::from_vec(b"File 012 Case\n.bin".to_vec())
        } else {
            OsString::from(format!("File {index:03} Case.bin"))
        };
        let path = fixture.0.join(&name);
        let body = format!("unchanged fixture {index:03}\n").into_bytes();
        fs::write(&path, &body).unwrap();
        let mode = fs::metadata(&path).unwrap().mode();
        inputs.push((name, body, mode));
    }
    let folder = fixture.0.canonicalize().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(folder.clone()))).unwrap();
    let before = app.local.as_ref().unwrap().explorer.current().clone();
    let source = hair::folder_scene::display(folder.as_os_str());
    assert_eq!(before.folder, folder);
    assert_eq!(before.entries.len(), inputs.len());
    for (name, _, _) in &inputs {
        assert!(before.entries.iter().any(|entry| {
            entry.name == *name
                && entry.path == folder.join(name)
                && entry.kind == hair::local_explorer::Kind::File
        }));
    }
    let view = app.world.folder_view.as_ref().unwrap();
    assert_eq!(view.page, 0);
    assert!(
        view.pages() >= 3,
        "fixture must overflow two physical rooms"
    );
    assert_eq!(view.source, source);
    assert_eq!(app.world.current, source);
    let first_bindings = view.bindings.clone();

    // The implemented continuation is an opposing side-wall portal. Keep the
    // real resolver/E/release proof, replacing only the obsolete front pose.
    aim_current_portal(&mut app, "folder-next");
    assert!(
        !app.themes[app.theme_index].host.blocked(app.pose.position),
        "synthetic approach must be collision safe"
    );
    app.world.explicit_target = None;
    hair::interaction::refresh(&app.themes[app.theme_index].host, &app.pose, &mut app.world);
    let aimed = app.world.interaction.clone();
    // Same outer adapter as WindowEvent::KeyboardInput, including key release.
    assert!(
        !app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
            .unwrap()
    );
    assert!(
        !app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
            .unwrap()
    );

    // Run preservation checks even on the expected RED, before the page oracle.
    assert_eq!(app.local.as_ref().unwrap().explorer.current(), &before);
    assert_eq!(app.world.current, source);
    let view = app.world.folder_view.as_ref().unwrap();
    assert_eq!(view.source, source);
    assert_eq!(view.count, before.entries.len());
    let actual_names: BTreeSet<_> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    let expected_names: BTreeSet<_> = inputs.iter().map(|(name, _, _)| name.clone()).collect();
    assert_eq!(actual_names, expected_names);
    for (name, body, mode) in &inputs {
        let path = folder.join(name);
        assert_eq!(fs::read(&path).unwrap(), *body);
        assert_eq!(fs::metadata(&path).unwrap().mode(), *mode);
    }
    assert_eq!(
        view.page, 1,
        "physical continuation: aimed E must reach page 2 of the SAME folder without page keys; aimed={aimed:?}; status={}",
        app.local_status
    );
    assert_eq!(app.local.as_ref().unwrap().page, 1);
    assert!(
        !app.world.reading,
        "continuation must not open a file reader"
    );
    assert!(
        !app.themes[app.theme_index].host.blocked(app.pose.position),
        "continuation arrival must be collision safe"
    );
    let aimed = aimed.expect("page transition must originate at a real eligible physical portal");
    assert_eq!(aimed.kind, hair::interaction::Kind::Enter);
    assert!(
        !first_bindings.contains_key(&aimed.anchor),
        "continuation is navigation within this folder, not a fabricated child entry"
    );
    assert!(!view.bindings.is_empty());
    for bound in view.bindings.values() {
        assert!(bound.index >= hair::folder_scene::PAGE_SIZE);
        assert!(bound.index < 2 * hair::folder_scene::PAGE_SIZE);
        assert_eq!(before.entries.get(bound.index), Some(&bound.entry));
    }
    let raw_second_page_name = &inputs[12].0;
    assert!(
        view.bindings
            .values()
            .any(|bound| bound.entry.name == *raw_second_page_name)
    );
}
