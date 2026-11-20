//! First room-flow oracle: production initial arrival and middle-page host geometry.
//! CPU-only; no proposed runtime API, native panel, window, network or GPU.
use super::*;
use hair::{folder_scene, local_explorer::Kind};
use std::fs;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        // Do not turn an assertion panic into a double panic during unwinding.
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!(
                "room-flow fixture cleanup failed at {}: {error}",
                self.0.display()
            );
        }
    }
}

#[test]
fn initial_spawn_and_middle_page_doors_follow_room_semantics() {
    // The parent supplies owned TMPDIR. Exclusive creation refuses stale fixtures;
    // the guard owns only this newly created directory, including on assertion RED.
    let tmp = PathBuf::from(std::env::var_os("TMPDIR").expect("parent-owned TMPDIR required"));
    assert!(
        tmp.is_absolute() && tmp.is_dir(),
        "TMPDIR must exist and be absolute"
    );
    let path = tmp.join(format!("hair-room-flow-{}", std::process::id()));
    fs::create_dir(&path).expect("create exclusive synthetic fixture");
    let fixture = Fixture(path);
    for index in 0..25 {
        fs::create_dir(fixture.0.join(format!("Child {index:03}"))).unwrap();
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
    let initial_host = &app.themes[app.theme_index].host;
    assert_eq!(app.pose.position, initial_host.spawn);
    assert!(!initial_host.blocked(app.pose.position));
    // interaction::resolve defines yaw zero as -Z; room[1] is the front boundary.
    let forward = [app.pose.yaw.sin(), -app.pose.yaw.cos()];
    let front_from_spawn = [0.0, initial_host.room[1] - app.pose.position[1]];
    assert!(
        forward[1] < 0.0
            && forward[0].abs() < 0.0001
            && app.pose.pitch.abs() < 0.4
            && forward[1] * front_from_spawn[1] > 0.0,
        "initial folder spawn must face the room front (-Z): {:?}",
        app.pose
    );
    let snapshot = app.local.as_ref().unwrap().explorer.current();
    assert_eq!(snapshot.folder, folder);
    assert_eq!(snapshot.entries.len(), 25);
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.kind == Kind::Folder)
    );

    // Zero-based page 1 has both continuations. Build it directly so a future
    // side-door arrival change cannot mask this first geometry RED with an E error.
    let (world, host) = folder_scene::build(snapshot, 1, false).unwrap();
    let view = world.folder_view.as_ref().unwrap();
    assert_eq!(view.page, 1);
    assert_eq!(view.pages(), 3);
    assert_eq!(view.bindings.len(), folder_scene::PAGE_SIZE);
    let anchor = |id: &str| {
        host.anchors
            .iter()
            .find(|a| a.id == id)
            .expect("bound anchor")
    };
    let parent = anchor(view.parent.as_deref().expect("non-root fixture has Parent"));
    let [left, front, right, rear] = host.room;
    let ahead = |a: &hair::host_scene::Anchor| {
        (a.at[0] - host.spawn[0]) * forward[0] + (a.at[2] - host.spawn[1]) * forward[1]
    };

    // FIRST behavioral failure on accepted source: Parent is still on the front.
    // Doorways are portal affordances on a wall, not continuous passages through
    // that wall: they may sit just inside it. Do not require a new wall-cutting API.
    assert!(
        (parent.at[2] - rear).abs() <= parent.size[2] + 0.5 && ahead(parent) < 0.0,
        "Parent/rear entry must be behind the forward-facing spawn, adjacent to the rear wall; parent={:?}, room={:?}, spawn={:?}",
        parent.at,
        host.room,
        host.spawn
    );
    for (id, bound) in &view.bindings {
        assert_eq!(bound.entry.kind, Kind::Folder);
        let child = anchor(id);
        assert_eq!(child.role, "portal");
        assert!(
            child.at[2] < front && ahead(child) > 0.0,
            "subfolder doorway must remain in front of spawn: {child:?}"
        );
        assert!(child.at[0] > left && child.at[0] < right);
    }
    let continuation = |action| {
        anchor(
            view.continuations
                .iter()
                .find(|(_, value)| **value == action)
                .expect("middle page has each continuation")
                .0,
        )
    };
    let previous = continuation(folder_scene::Continuation::Previous);
    let next = continuation(folder_scene::Continuation::Next);
    let mid_z = (front + rear) * 0.5;
    // Adjacent portal frames may stand just inside solid walls: activation is E,
    // not continuous traversal through an opening in the room shell.
    let epsilon = 0.0001;
    assert!(
        previous.at[0] < host.spawn[0] && next.at[0] > host.spawn[0],
        "Previous/Next must occupy opposing left/right walls: previous={previous:?}, next={next:?}"
    );
    for door in [previous, next] {
        assert_eq!(door.role, "portal");
        assert!(
            (door.at[2] - mid_z).abs() < epsilon,
            "side continuation must be at its wall midpoint, not a front corner: {door:?}"
        );
    }
    assert!((previous.at[0] - left).abs() <= previous.size[0] + 0.5);
    assert!((next.at[0] - right).abs() <= next.size[0] + 0.5);
}

#[test]
fn side_continuation_arrives_inside_reciprocal_and_turns_back() {
    let path = std::env::temp_dir().join(format!("hair-side-arrival-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    for index in 0..25 {
        fs::create_dir(fixture.0.join(format!("Child {index:03}"))).unwrap();
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
    let next = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "folder-next")
        .unwrap()
        .clone();
    app.pose = Pose {
        position: [next.at[0] - 1.05, next.at[2]],
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: ((1.0_f32 - 1.65) / 1.05).atan(),
    };
    assert!(!app.themes[0].host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("folder-next")
    );
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().page,
        1,
        "physical side Next must commit the next room with a valid inward arrival: {}",
        app.local_status
    );
    let previous = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "folder-previous")
        .unwrap();
    assert!((app.pose.position[0] - (previous.at[0] + 1.05)).abs() < 0.01);
    assert!((app.pose.position[1] - previous.at[2]).abs() < 0.01);
    assert!(
        app.pose.yaw.sin() > 0.99,
        "arrival from left must face inward +X"
    );
    assert!(!app.themes[0].host.blocked(app.pose.position));
    app.pose.yaw += std::f32::consts::PI;
    app.pose.pitch = ((1.0_f32 - 1.65) / 1.05).atan();
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("folder-previous")
    );
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(app.world.folder_view.as_ref().unwrap().page, 0);
    assert!((app.pose.position[0] - (next.at[0] - 1.05)).abs() < 0.01);
    assert!(
        app.pose.yaw.sin() < -0.99,
        "arrival from right must face inward -X"
    );
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        folder
    );
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 25);
}

#[test]
fn entering_child_arrives_just_inside_rear_parent_facing_front() {
    let path = std::env::temp_dir().join(format!("hair-rear-entry-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    fs::create_dir(fixture.0.join("Child")).unwrap();
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
    app.complete_folder(Ok(Some(fixture.0.canonicalize().unwrap())))
        .unwrap();
    let child = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap()
        .clone();
    app.pose = Pose {
        position: [child.at[0], child.at[2] + 1.05],
        yaw: 0.0,
        pitch: ((1.0_f32 - 1.65) / 1.05).atan(),
    };
    app.world.explicit_target = None;
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("entry-0")
    );
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(
        app.local
            .as_ref()
            .unwrap()
            .explorer
            .current()
            .folder
            .file_name()
            .unwrap(),
        "Child"
    );
    let parent = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "folder-parent")
        .unwrap();
    assert!((app.pose.position[0] - parent.at[0]).abs() < 0.01);
    assert!(
        (app.pose.position[1] - (parent.at[2] - 1.05)).abs() < 0.01,
        "entering a child must arrive beside rear Parent, not unrelated room center: {:?}",
        app.pose
    );
    assert!(app.pose.yaw.cos() > 0.99);
    assert!(!app.themes[0].host.blocked(app.pose.position));
    app.pose.yaw += std::f32::consts::PI;
    app.pose.pitch = ((1.0_f32 - 1.65) / 1.05).atan();
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("folder-parent")
    );
    app.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    app.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        fixture.0.canonicalize().unwrap()
    );
    assert!((app.pose.position[0] - child.at[0]).abs() < 0.01);
}
