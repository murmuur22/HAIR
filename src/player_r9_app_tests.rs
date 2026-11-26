use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        let workspace = PathBuf::from(
            std::env::var_os("HAIR_TEST_WORKSPACE").expect("explicit owned workspace"),
        )
        .canonicalize()
        .unwrap();
        let temp =
            PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned sibling TMPDIR"))
                .canonicalize()
                .unwrap();
        assert_eq!(source.parent(), Some(workspace.as_path()));
        assert_eq!(temp.parent(), Some(workspace.as_path()));
        assert!(!temp.starts_with(&source) && !source.starts_with(&temp));
        let fixture_path = temp.join(format!("r9-refresh-{}", std::process::id()));
        std::fs::create_dir(&fixture_path).unwrap();
        let fixture = Self(fixture_path);
        std::fs::write(fixture.0.join("File.txt"), b"R9 immutable source\n").unwrap();
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn r9_actual_app_f5_refresh_retains_pose_inside_generated_file_slot() {
    let fixture = Fixture::new();
    let mut a = app();
    a.complete_folder(Ok(Some(fixture.0.clone()))).unwrap();
    let host = &a.themes[0].host;
    let id = host.file_anchors.iter().next().unwrap();
    let anchor = host.anchors.iter().find(|item| &item.id == id).unwrap();
    a.pose = Pose {
        position: [anchor.at[0], anchor.at[2]],
        yaw: 0.37,
        pitch: -0.21,
        ..Default::default()
    };
    assert!(!hair::player::blocked_at(host, a.pose.position, 0.0));
    let before = a.pose;
    // Real keyboard -> local_change(Refresh) -> placement install -> retained pose.
    std::fs::write(fixture.0.join("Z-new.txt"), b"new listing entry\n").unwrap();
    tap(&mut a, KeyCode::F5);
    assert_eq!(
        a.world.folder_view.as_ref().unwrap().count,
        2,
        "actual refresh committed new snapshot"
    );
    assert_eq!(
        a.pose.position, before.position,
        "refresh must not respawn from a walkable file slot"
    );
    assert_eq!(a.pose.yaw, before.yaw);
    assert_eq!(a.pose.pitch, before.pitch);
    assert!(!a.themes[0].host.blocked(a.pose.position));
    a.themes[0].host.check_routes(&[a.pose.position]).unwrap();
    a.input.captured = true;
    a.advance_motion(0.01);
    assert_eq!(
        a.pose.position, before.position,
        "no recovery after refresh"
    );
    assert_eq!(
        std::fs::read(fixture.0.join("File.txt")).unwrap(),
        b"R9 immutable source\n"
    );
    assert!(!fixture.0.join(".hair-layout").exists());
}

#[test]
fn r9_publication_accepts_visitor_in_file_slot_without_weakening_overlap_safety() {
    use hair::{
        placement_scene::validate_publication,
        placement_store::{LayoutRead, Position},
    };
    let fixture = Fixture::new();
    let explorer = hair::local_explorer::Explorer::new(&fixture.0).unwrap();
    let (_, host) = hair::folder_scene::build(explorer.current(), 0, false).unwrap();
    let anchor = host
        .anchors
        .iter()
        .find(|a| host.file_anchors.contains(&a.id))
        .unwrap();
    let visitor = [anchor.at[0], anchor.at[2]];
    let layout = LayoutRead {
        positions: vec![],
        decorations: vec![],
        retained_slots: vec![],
        incomplete_records: 0,
        warnings: vec![],
    };
    let position = Position {
        properties: Default::default(),
        at: [-1.5, 0.0],
        yaw: 0.0,
        size: [0.8; 3],
        allow_overlap: false,
        surface: None,
    };
    assert_eq!(
        validate_publication(
            &explorer,
            &layout,
            std::ffi::OsStr::new("New.txt"),
            &position,
            visitor,
            0
        )
        .unwrap(),
        0
    );
    let mut overlapping = position.clone();
    overlapping.at = visitor;
    assert!(
        validate_publication(
            &explorer,
            &layout,
            std::ffi::OsStr::new("New.txt"),
            &overlapping,
            visitor,
            0
        )
        .is_err(),
        "Prevent still rejects overlap with a generated file"
    );
    let saved = LayoutRead {
        positions: vec![("File.txt".into(), position.clone())],
        decorations: vec![],
        retained_slots: vec![],
        incomplete_records: 0,
        warnings: vec![],
    };
    assert!(
        validate_publication(
            &explorer,
            &saved,
            std::ffi::OsStr::new("New.txt"),
            &position,
            visitor,
            0
        )
        .is_err(),
        "Prevent still rejects overlap with a saved file"
    );
    for at in [host.spawn, [host.room[2], 0.0]] {
        let unsafe_position = Position {
            at,
            allow_overlap: true,
            ..position.clone()
        };
        assert!(
            validate_publication(
                &explorer,
                &layout,
                std::ffi::OsStr::new("New.txt"),
                &unsafe_position,
                visitor,
                0
            )
            .is_err(),
            "Allow cannot bypass spawn/platform reservations"
        );
    }
    assert!(!fixture.0.join("New.txt").exists());
    assert!(!fixture.0.join(".hair-layout").exists());
    assert_eq!(
        std::fs::read(fixture.0.join("File.txt")).unwrap(),
        b"R9 immutable source\n"
    );
}
