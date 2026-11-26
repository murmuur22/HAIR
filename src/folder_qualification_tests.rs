//! Qualification of implemented local navigation. Synthetic App input, not OS input.
//! All disk enumeration is confined to exclusively created fixtures (never `/`).
// Separate ignored semantic proof; the generic matrix below remains unchanged.
#[path = "folder_sparse_tests.rs"]
mod sparse;
use super::*;
use hair::{
    folder_scene,
    local_explorer::{Kind, Snapshot},
};
use std::{collections::BTreeSet, ffi::OsStr, fs, os::unix::fs::MetadataExt};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let owned = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        assert!(
            temp.starts_with(&owned),
            "TMPDIR must be inside this owned milestone workspace"
        );
        let path = temp.join(format!("hair-slice6-{label}-{}", std::process::id()));
        fs::create_dir(&path)
            .expect("exclusive fixture; preserve/reconcile leftovers before rerun");
        Self(path.canonicalize().unwrap())
    }
    fn children(&self, count: usize) {
        for n in 0..count {
            fs::create_dir(self.0.join(format!("Child {n:03}"))).unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn app(folder: &Path) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["office", "archive"]
        .iter()
        .map(|name| {
            hair::theme::Theme::load(&root.join(format!("packs/{name}.toml")), &mut cache).unwrap()
        })
        .collect();
    let mut a = App::new(root.clone(), themes, 0, World::load(&root).unwrap(), false);
    a.complete_folder(Ok(Some(folder.to_path_buf()))).unwrap();
    a
}
fn key(a: &mut App, code: KeyCode, down: bool, repeat: bool) {
    assert!(
        !a.keyboard_input(Some(code), down, repeat, None).unwrap(),
        "unexpected exit: {code:?}"
    );
}
fn tap(a: &mut App, code: KeyCode) {
    key(a, code, true, false);
    key(a, code, false, false);
}
fn view(a: &App) -> &folder_scene::View {
    a.world.folder_view.as_ref().unwrap()
}
fn snapshot(a: &App) -> Snapshot {
    a.local.as_ref().unwrap().explorer.current().clone()
}
fn pose(a: &App) -> ([f32; 2], f32, f32) {
    (a.pose.position, a.pose.yaw, a.pose.pitch)
}
fn aim(a: &mut App, id: &str) {
    let host = &a.themes[a.theme_index].host;
    let anchor = host
        .anchors
        .iter()
        .find(|v| v.id == id)
        .unwrap_or_else(|| panic!("missing {id}"));
    let n = host.portal_inward(id); // documents retain the legacy +Z approach
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [anchor.at[0] + 1.05 * n[0], anchor.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(a.pose.position));
    a.world.explicit_target = None;
    hair::interaction::refresh(host, &a.pose, &mut a.world);
    assert_eq!(
        a.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some(id)
    );
    if let Some(bound) = view(a).bindings.get(id) {
        let target = a.world.interaction.as_ref().unwrap();
        assert_eq!(
            target.destination.as_deref(),
            Some(folder_scene::display(bound.entry.path.as_os_str()).as_str())
        );
        assert!(
            target
                .prompt
                .contains(&folder_scene::display(&bound.entry.name))
        );
    }
}
// An absent boundary has no aperture/anchor from which to infer a normal.
// Use the SAME snapshot's middle-page host as the positive geometric control,
// then resolve against the actual boundary host (which must keep a solid wall).
fn aim_absent_boundary(a: &mut App, id: &str) {
    let (_, control) = folder_scene::build(&snapshot(a), 1, false).unwrap();
    let anchor = control
        .anchors
        .iter()
        .find(|v| v.id == id)
        .expect("middle-page door");
    let n = control.portal_inward(id);
    let host = &a.themes[a.theme_index].host;
    assert!(!host.anchors.iter().any(|v| v.id == id));
    assert!(!host.wall_apertures.iter().any(|v| v.anchor == id));
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [anchor.at[0] + 1.05 * n[0], anchor.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(a.pose.position));
    a.world.explicit_target = None;
    hair::interaction::refresh(host, &a.pose, &mut a.world);
    assert!(
        a.world.interaction.is_none(),
        "absent boundary must not fabricate an action"
    );
}
fn door(a: &mut App, id: &str) {
    aim(a, id);
    tap(a, KeyCode::KeyE);
}
fn find(a: &mut App, name: &OsStr) -> String {
    let index = snapshot(a)
        .entries
        .iter()
        .position(|e| e.name == name)
        .expect("fixture name");
    let page = index / folder_scene::PAGE_SIZE;
    while view(a).page < page {
        door(a, "folder-next");
    }
    while view(a).page > page {
        door(a, "folder-previous");
    }
    let id = view(a)
        .bindings
        .iter()
        .find(|(_, b)| b.index == index)
        .unwrap()
        .0
        .clone();
    aim(a, &id);
    id
}
fn assert_bindings(a: &App) {
    let current = snapshot(a);
    let v = view(a);
    assert_eq!(v.page, a.local.as_ref().unwrap().page);
    assert_eq!(v.source, folder_scene::display(current.folder.as_os_str()));
    assert_eq!(a.world.current, v.source);
    assert_eq!(v.count, current.entries.len());
    let expected: BTreeSet<_> = (v.page * folder_scene::PAGE_SIZE
        ..((v.page + 1) * folder_scene::PAGE_SIZE).min(v.count))
        .collect();
    assert_eq!(
        v.bindings
            .values()
            .map(|b| b.index)
            .collect::<BTreeSet<_>>(),
        expected
    );
    for bound in v.bindings.values() {
        assert_eq!(current.entries.get(bound.index), Some(&bound.entry));
    }
    assert!(!a.themes[a.theme_index].host.blocked(a.pose.position));
}
fn assert_child_arrival(a: &App, name: &str) {
    assert_bindings(a);
    let id = view(a)
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == name && b.entry.kind == Kind::Folder)
        .unwrap()
        .0;
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|v| &v.id == id)
        .unwrap();
    let n = a.themes[a.theme_index].host.portal_inward(id);
    assert!((a.pose.position[0] - (anchor.at[0] + 1.05 * n[0])).abs() < 0.01);
    assert!((a.pose.position[1] - (anchor.at[2] + 1.05 * n[1])).abs() < 0.01);
    assert!(
        a.pose.yaw.sin() * n[0] - a.pose.yaw.cos() * n[1] > 0.99,
        "Parent arrives facing inward"
    );
}
fn inventory(folder: &Path) -> Vec<(PathBuf, u32, Option<Vec<u8>>)> {
    // Only immediate fixture children; never follow links or recurse.
    let mut items = vec![(
        folder.to_path_buf(),
        fs::symlink_metadata(folder).unwrap().mode(),
        None,
    )];
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        assert!(meta.is_file() || meta.is_dir());
        items.push((
            path.clone(),
            meta.mode(),
            if meta.is_file() {
                Some(fs::read(path).unwrap())
            } else {
                None
            },
        ));
    }
    items.sort();
    items
}

#[test]
fn empty_nonroot_has_only_real_parent_and_u_returns_to_owned_parent() {
    let f = Fixture::new("empty");
    let empty = f.0.join("Empty");
    fs::create_dir(&empty).unwrap();
    let mut a = app(&empty);
    assert!(view(&a).bindings.is_empty());
    assert!(view(&a).continuations.is_empty());
    assert_eq!(
        view(&a).parent.as_deref(),
        Some(folder_scene::PARENT_PORTAL)
    );
    assert!(view(&a).summary().contains("Empty folder"));
    let before = snapshot(&a);
    tap(&mut a, KeyCode::KeyE);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(snapshot(&a), before);
    // Empty is not filesystem root: hiding Parent or making U inert here is wrong.
    tap(&mut a, KeyCode::KeyU);
    assert_eq!(snapshot(&a).folder, f.0);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(snapshot(&a), before);
}

#[test]
fn synthetic_root_has_no_parent_continuation_or_eligible_false_action() {
    // In-memory root only. Explorer::new("/") would enumerate real directories
    // and violates this slice. This is NOT an App U / root-handle proof.
    let root = Snapshot {
        folder: "/".into(),
        entries: Vec::new(),
    };
    for collapsed in [false, true] {
        let (world, host) = folder_scene::build(&root, usize::MAX, collapsed).unwrap();
        let v = world.folder_view.as_ref().unwrap();
        assert!(v.parent.is_none() && v.bindings.is_empty() && v.continuations.is_empty());
        assert_eq!((v.page, v.pages()), (0, 1));
        assert!(
            !host
                .anchors
                .iter()
                .any(|a| a.id == folder_scene::PARENT_PORTAL)
        );
        // Positive control supplies current rear/side normals even though the
        // synthetic root has no such actions/apertures. No root enumeration.
        let control_snapshot = Snapshot {
            folder: "/SyntheticBoundaryControl".into(),
            entries: (0..25)
                .map(|i| hair::local_explorer::Entry {
                    name: format!("Child{i:03}").into(),
                    path: format!("/SyntheticBoundaryControl/Child{i:03}").into(),
                    kind: Kind::Folder,
                })
                .collect(),
        };
        let (_, control) = folder_scene::build(&control_snapshot, 1, false).unwrap();
        for id in [
            folder_scene::PARENT_PORTAL,
            "folder-previous",
            "folder-next",
        ] {
            assert!(!host.anchors.iter().any(|a| a.id == id));
            assert!(!host.wall_apertures.iter().any(|a| a.anchor == id));
            let anchor = control.anchors.iter().find(|a| a.id == id).unwrap();
            let n = control.portal_inward(id);
            let p = Pose {
                eye_offset: 1.65,
                feet_y: 0.0,
                // Project the control portal's boundary-relative location into
                // this empty root's own bounds, not the larger control room.
                position: [
                    anchor.at[0] + 1.05 * n[0] + (control.room[2] - host.room[2]) * n[0],
                    anchor.at[2] + 1.05 * n[1] + (control.room[3] - host.room[3]) * n[1],
                ],
                yaw: (-n[0]).atan2(n[1]),
                pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
            };
            assert!(!host.blocked(p.position));
            assert!(hair::interaction::resolve(&host, &p, &world).is_none());
        }
    }
}

#[test]
fn physical_continuations_cover_every_binding_and_stop_at_both_boundaries() {
    let f = Fixture::new("coverage");
    f.children(37);
    let before = inventory(&f.0);
    let mut a = app(&f.0);
    let original = snapshot(&a);
    let pages = view(&a).pages();
    assert_eq!(pages, 4);
    assert!(!view(&a).continuations.contains_key("folder-previous"));
    tap(&mut a, KeyCode::PageUp);
    assert_eq!(view(&a).page, 0);
    let mut seen = BTreeSet::new();
    for page in 0..pages {
        assert_eq!(view(&a).page, page);
        assert_bindings(&a);
        assert_eq!(
            view(&a).continuations.contains_key("folder-previous"),
            page > 0
        );
        assert_eq!(
            view(&a).continuations.contains_key("folder-next"),
            page + 1 < pages
        );
        for (id, bound) in view(&a).bindings.clone() {
            aim(&mut a, &id);
            assert!(seen.insert(bound.index), "entry duplicated across rooms");
            assert_eq!(original.entries[bound.index], bound.entry);
        }
        if page + 1 < pages {
            door(&mut a, "folder-next");
        }
    }
    assert_eq!(seen, (0..original.entries.len()).collect());
    // Last room has no real right-wall Next; the inert front vocabulary portal
    // is not a substitute. Aim where the middle page's actual side door was.
    aim_absent_boundary(&mut a, "folder-next");
    tap(&mut a, KeyCode::KeyE);
    tap(&mut a, KeyCode::PageDown);
    assert_eq!(view(&a).page, pages - 1);
    for page in (0..pages - 1).rev() {
        door(&mut a, "folder-previous");
        assert_eq!(view(&a).page, page);
        assert_eq!(snapshot(&a), original);
        assert_bindings(&a);
    }
    aim_absent_boundary(&mut a, "folder-previous");
    tap(&mut a, KeyCode::KeyE);
    tap(&mut a, KeyCode::KeyR); // continuation rooms never add filesystem history
    assert_eq!(snapshot(&a), original);
    assert_eq!(view(&a).page, 0);
    assert_eq!(inventory(&f.0), before);
}

#[test]
fn held_e_focus_and_reader_do_not_chain_world_actions() {
    let f = Fixture::new("isolation");
    f.children(25);
    fs::write(f.0.join("Read Case.txt"), b"LITERAL SYNTHETIC TEXT\n").unwrap();
    let before = inventory(&f.0);
    let mut a = app(&f.0);
    aim(&mut a, "folder-next");
    key(&mut a, KeyCode::KeyE, true, false);
    assert_eq!(view(&a).page, 1);
    aim(&mut a, "folder-next"); // eligible again; not merely inward-facing suppression
    for repeated in [true, false, true] {
        key(&mut a, KeyCode::KeyE, true, repeated);
        assert_eq!(view(&a).page, 1);
    }
    key(&mut a, KeyCode::KeyE, false, false);
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(view(&a).page, 2);
    aim(&mut a, "folder-previous");
    key(&mut a, KeyCode::KeyW, true, false);
    a.focus_changed(false);
    assert!(!a.input.forward && !a.input.captured && !a.interact_held && a.local_held.is_empty());
    let frozen = (snapshot(&a), view(&a).clone(), pose(&a));
    for code in [
        KeyCode::KeyE,
        KeyCode::KeyU,
        KeyCode::KeyR,
        KeyCode::F5,
        KeyCode::KeyO,
        KeyCode::PageUp,
    ] {
        tap(&mut a, code);
    }
    assert_eq!((snapshot(&a), view(&a).clone(), pose(&a)), frozen);
    assert!(!a.chooser_pending);
    a.focus_changed(true);
    aim(&mut a, "folder-previous");
    key(&mut a, KeyCode::KeyE, true, true);
    assert_eq!(
        view(&a).page,
        2,
        "synthetic repeat after focus regain is not a new press"
    );
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(view(&a).page, 1);
    find(&mut a, OsStr::new("Read Case.txt"));
    tap(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    let doc = a.world.document().unwrap();
    let body = doc
        .split_once("\nTEXT (links inactive; escapes shown; range only):\n")
        .unwrap()
        .1
        .split_once("\n\nFULL SOURCE / METADATA:\n")
        .unwrap()
        .0;
    assert_eq!(body, "LITERAL SYNTHETIC TEXT\n");
    assert!(doc.contains("RANGE [0x0000000000000000, 0x0000000000000017)"));
    assert!(doc.contains("Observed length: 23 bytes | EOF (next inert)"));
    assert_eq!(
        fs::read(f.0.join("Read Case.txt")).unwrap(),
        b"LITERAL SYNTHETIC TEXT\n"
    );
    let frozen = (snapshot(&a), view(&a).clone(), pose(&a));
    for code in [
        KeyCode::KeyE,
        KeyCode::KeyU,
        KeyCode::KeyR,
        KeyCode::F5,
        KeyCode::KeyO,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::KeyL,
        KeyCode::Tab,
        KeyCode::KeyW,
    ] {
        tap(&mut a, code);
    }
    assert_eq!((snapshot(&a), view(&a).clone(), pose(&a)), frozen);
    assert!(a.world.reading && !a.chooser_pending && !a.input.forward && a.address.is_none());
    tap(&mut a, KeyCode::Escape);
    assert!(!a.world.reading);
    assert_eq!(inventory(&f.0), before);
}

#[test]
fn u_and_back_keep_history_pages_and_distinct_arrival_policies_aligned() {
    let f = Fixture::new("history");
    f.children(30);
    let before = inventory(&f.0);
    let mut a = app(&f.0);
    // Listing moved from M to Settings; M is now a modal directory map.
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::ArrowDown);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter); // Settings > Player Settings
    for _ in 0..3 {
        tap(&mut a, KeyCode::ArrowDown);
    }
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    let collapsed = a.world.tree_collapsed;
    find(&mut a, OsStr::new("Child 024"));
    let departure = pose(&a);
    tap(&mut a, KeyCode::KeyE);
    let child = snapshot(&a);
    assert_eq!(child.folder, f.0.join("Child 024"));
    let child_pose = pose(&a);
    key(&mut a, KeyCode::KeyU, true, false);
    assert_eq!(snapshot(&a).folder, f.0);
    assert_eq!(view(&a).page, 2);
    assert_child_arrival(&a, "Child 024");
    let parent_arrival = pose(&a);
    for repeat in [true, false] {
        key(&mut a, KeyCode::KeyU, true, repeat);
    }
    assert_eq!(
        (snapshot(&a).folder, pose(&a)),
        (f.0.clone(), parent_arrival)
    );
    key(&mut a, KeyCode::KeyU, false, false);
    key(&mut a, KeyCode::KeyR, true, false);
    assert_eq!(
        snapshot(&a),
        child,
        "Back after U consumes the child visit, not the older parent"
    );
    assert_eq!(pose(&a), child_pose);
    for repeat in [false, true] {
        key(&mut a, KeyCode::KeyR, true, repeat);
    }
    assert_eq!(snapshot(&a), child);
    key(&mut a, KeyCode::KeyR, false, false);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(snapshot(&a).folder, f.0);
    assert_eq!(view(&a).page, 2);
    assert_eq!(
        pose(&a),
        departure,
        "Back restores departure pose, not Parent's inward pose"
    );
    assert_bindings(&a);
    let stable = (snapshot(&a), view(&a).clone(), pose(&a));
    tap(&mut a, KeyCode::KeyR);
    assert_eq!((snapshot(&a), view(&a).clone(), pose(&a)), stable);
    assert_eq!(a.world.tree_collapsed, collapsed);
    assert_eq!(inventory(&f.0), before);
}

#[test]
fn lf01_direct_back_rebinds_child_after_page_and_slot_changes() {
    for removed in [12, 1] {
        let f = Fixture::new(if removed == 12 {
            "back-page"
        } else {
            "back-slot"
        });
        f.children(37);
        let mut a = app(&f.0);
        find(&mut a, OsStr::new("Child 024"));
        let departed = a.pose;
        tap(&mut a, KeyCode::KeyE);
        for n in 0..removed {
            fs::remove_dir(f.0.join(format!("Child {n:03}"))).unwrap();
        }
        let mutated = inventory(&f.0);
        tap(&mut a, KeyCode::KeyR);
        assert_eq!(
            view(&a).page,
            (24 - removed) / hair::folder_scene::PAGE_SIZE,
            "LF-01 direct Back must rebind raw originating child after fresh enumeration"
        );
        let id = view(&a)
            .bindings
            .iter()
            .find(|(_, b)| b.entry.name == "Child 024")
            .unwrap()
            .0;
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|x| x.id == *id)
            .unwrap();
        assert!(
            (a.pose.position[0] - anchor.at[0]).abs() < 0.01,
            "Back must follow changed child slot rather than aim at a different sibling"
        );
        assert_eq!(
            a.pose.yaw, departed.yaw,
            "Back keeps departure orientation, not Parent inward policy"
        );
        assert_eq!(a.pose.pitch, departed.pitch);
        assert_bindings(&a);
        assert_eq!(inventory(&f.0), mutated);
    }
}

#[test]
fn lf01_direct_back_preserves_departure_offset_and_consumes_one_visit() {
    for removed in [0, 12, 1] {
        let f = Fixture::new(&format!("back-offset-{removed}"));
        f.children(37);
        let mut a = app(&f.0);
        let id = find(&mut a, OsStr::new("Child 024"));
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|anchor| anchor.id == id)
            .unwrap();
        let old_door = [anchor.at[0], anchor.at[2]];
        a.pose.position[0] += 0.05;
        a.pose.position[1] += 0.10;
        a.pose.yaw = 0.04;
        a.pose.pitch += 0.02;
        let departed = a.pose;
        hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
        assert_eq!(a.world.interaction.as_ref().unwrap().anchor, id);
        tap(&mut a, KeyCode::KeyE);
        assert_eq!(snapshot(&a).folder, f.0.join("Child 024"));
        for n in 0..removed {
            fs::remove_dir(f.0.join(format!("Child {n:03}"))).unwrap();
        }
        let mutated = inventory(&f.0);
        tap(&mut a, KeyCode::KeyR);
        assert_eq!(snapshot(&a).folder, f.0);
        assert_eq!(view(&a).page, (24 - removed) / folder_scene::PAGE_SIZE);
        let id = view(&a)
            .bindings
            .iter()
            .find(|(_, bound)| bound.entry.name == "Child 024")
            .unwrap()
            .0;
        let anchor = a.themes[a.theme_index]
            .host
            .anchors
            .iter()
            .find(|anchor| &anchor.id == id)
            .unwrap();
        for (axis, destination) in [anchor.at[0], anchor.at[2]].into_iter().enumerate() {
            let expected = departed.position[axis] + (destination - old_door[axis]);
            assert!((a.pose.position[axis] - expected).abs() < 0.0001);
        }
        assert_eq!(a.pose.yaw, departed.yaw);
        assert_eq!(a.pose.pitch, departed.pitch);
        if removed == 0 {
            assert_eq!(pose(&a), (departed.position, departed.yaw, departed.pitch));
        }
        assert_bindings(&a);
        let restored = (snapshot(&a), view(&a).clone(), pose(&a));
        tap(&mut a, KeyCode::KeyR);
        assert_eq!((snapshot(&a), view(&a).clone(), pose(&a)), restored);
        assert_eq!(a.local_status, "No previous visited directory");
        assert_eq!(inventory(&f.0), mutated);
    }
}

#[test]
fn lf01_direct_back_missing_or_nonfolder_child_uses_explicit_safe_spawn() {
    for replacement in [false, true] {
        let f = Fixture::new(if replacement {
            "back-replace-child"
        } else {
            "back-remove-child"
        });
        f.children(37);
        let mut a = app(&f.0);
        find(&mut a, OsStr::new("Child 024"));
        tap(&mut a, KeyCode::KeyE);
        fs::remove_dir(f.0.join("Child 024")).unwrap();
        if replacement {
            fs::write(f.0.join("Child 024"), b"not a child doorway").unwrap();
        }
        let mutated = inventory(&f.0);
        tap(&mut a, KeyCode::KeyR);
        assert_eq!(snapshot(&a).folder, f.0);
        assert_eq!(view(&a).page, 0);
        assert_eq!(a.pose.position, a.themes[a.theme_index].host.spawn);
        assert!(a.local_status.contains("Back; no matching child context"));
        assert_bindings(&a);
        let restored = (snapshot(&a), view(&a).clone(), pose(&a));
        tap(&mut a, KeyCode::KeyR);
        assert_eq!((snapshot(&a), view(&a).clone(), pose(&a)), restored);
        assert_eq!(a.local_status, "No previous visited directory");
        assert_eq!(inventory(&f.0), mutated);
    }
}

#[test]
fn parent_rebinds_reordered_child_to_its_new_page_and_real_target() {
    let f = Fixture::new("reorder");
    f.children(30);
    let mut a = app(&f.0);
    find(&mut a, OsStr::new("Child 024"));
    assert_eq!(view(&a).page, 2);
    tap(&mut a, KeyCode::KeyE);
    // Test-owned post-selection mutation, never an application write.
    for n in 0..12 {
        fs::remove_dir(f.0.join(format!("Child {n:03}"))).unwrap();
    }
    let mutated = inventory(&f.0);
    tap(&mut a, KeyCode::KeyU);
    assert_eq!(
        view(&a).page,
        1,
        "raw Child024 moved from index24 to index12"
    );
    assert_child_arrival(&a, "Child 024");
    tap(&mut a, KeyCode::F5);
    assert_bindings(&a);
    find(&mut a, OsStr::new("Child 024"));
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(
        snapshot(&a).folder,
        f.0.join("Child 024"),
        "E must not reuse old index24 or slot"
    );
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(view(&a).page, 1);
    assert_bindings(&a);
    assert_eq!(inventory(&f.0), mutated);
}

#[test]
fn parent_with_removed_or_file_replaced_child_uses_safe_default_not_wrong_door() {
    for replacement in [false, true] {
        let f = Fixture::new(if replacement {
            "replace-child"
        } else {
            "remove-child"
        });
        f.children(30);
        let mut a = app(&f.0);
        find(&mut a, OsStr::new("Child 024"));
        tap(&mut a, KeyCode::KeyE);
        fs::remove_dir(f.0.join("Child 024")).unwrap();
        if replacement {
            fs::write(
                f.0.join("Child 024"),
                b"replacement is a file, not that doorway",
            )
            .unwrap();
        }
        let mutated = inventory(&f.0);
        tap(&mut a, KeyCode::KeyU);
        assert_eq!(snapshot(&a).folder, f.0);
        assert_eq!(view(&a).page, 0);
        assert_eq!(a.pose.position, a.themes[a.theme_index].host.spawn);
        assert!(a.local_status.contains("no matching child context"));
        assert!(
            !snapshot(&a)
                .entries
                .iter()
                .any(|e| e.name == "Child 024" && e.kind == Kind::Folder)
        );
        tap(&mut a, KeyCode::F5);
        assert_bindings(&a);
        find(&mut a, OsStr::new("Child 025"));
        tap(&mut a, KeyCode::KeyE);
        assert_eq!(snapshot(&a).folder, f.0.join("Child 025"));
        assert_eq!(inventory(&f.0), mutated);
    }
}

#[test]
fn refresh_clamps_removed_last_page_and_e_uses_fresh_binding() {
    let f = Fixture::new("refresh");
    f.children(25);
    let mut a = app(&f.0);
    find(&mut a, OsStr::new("Child 024"));
    assert_eq!(view(&a).page, 2);
    for n in 0..14 {
        fs::remove_dir(f.0.join(format!("Child {n:03}"))).unwrap();
    }
    let mutated = inventory(&f.0);
    tap(&mut a, KeyCode::F5);
    assert_eq!(view(&a).page, 0);
    assert_eq!(view(&a).pages(), 1);
    assert!(view(&a).continuations.is_empty());
    assert_bindings(&a);
    let fresh = view(&a).bindings.get("entry-0").unwrap().entry.clone();
    assert_eq!(fresh.name, "Child 014");
    door(&mut a, "entry-0");
    assert_eq!(snapshot(&a).folder, fresh.path);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(view(&a).page, 0);
    assert_bindings(&a);
    assert_eq!(inventory(&f.0), mutated);
}

const MKV_NAME: &str = "Video Case\n\t\"Tail.MkV";
const MKV_BYTES: &[u8] = b"NOT A MEDIA STREAM\0\xffNO-DECODE-MARKER\n";
fn assert_mkv(a: &App, file: &Path) {
    assert!(a.world.reading);
    let doc = a.world.document().unwrap();
    assert!(doc.contains(&format!(
        "NAME (escaped OS bytes):\n{}\n",
        folder_scene::display(file.file_name().unwrap())
    )));
    assert!(doc.contains(&format!(
        "SOURCE PATH (display only; may become stale):\n{}\n",
        folder_scene::display(file.as_os_str())
    )));
    assert!(doc.contains("TYPE: MKV video (playback unavailable)\n"));
    assert!(doc.contains("No writes, execution, default-open or link following."));
    assert!(doc.starts_with("RAW STORED BYTES\n"));
    assert!(doc.contains(&format!(
        "RANGE [0x{:016X}, 0x{:016X})\nObserved length: {} bytes | EOF (next inert)",
        0,
        MKV_BYTES.len(),
        MKV_BYTES.len()
    )));
    assert!(doc.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
    let actual = doc
        .split_once("\nBYTE GROUPS (8 bytes max)\n")
        .unwrap()
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")
        .unwrap()
        .0;
    let mut expected = String::new();
    for (row, bytes) in MKV_BYTES.chunks(8).enumerate() {
        expected.push_str(&format!("OFFSET {:016X}\nHEX ", row * 8));
        for (column, byte) in bytes.iter().enumerate() {
            if column != 0 {
                expected.push(' ');
            }
            expected.push_str(&format!("{byte:02X}"));
        }
        expected.push_str("\nASCII ");
        for &byte in bytes {
            expected.push(if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            });
        }
        expected.push_str("\n\n");
    }
    assert_eq!(
        actual, expected,
        "ASCII marker is stored bytes, not decoded media"
    );
    assert!(
        actual.starts_with(
            "OFFSET 0000000000000000\nHEX 4E 4F 54 20 41 20 4D 45\nASCII NOT A ME\n\n"
        )
    );
    assert!(doc.contains("Stored bytes only: no decoding media"));
    for invented in [
        "Body/preview unsupported for this kind.",
        "STALE / READ ERROR:",
        "NO BODY READ:",
        "TEXT (links inactive",
        "Codec:",
        "Duration:",
    ] {
        assert!(!doc.contains(invented));
    }
    assert!(a.web.is_none() && a.world.reader_link.is_none() && a.address.is_none());
}
#[test]
fn aimed_app_e_mkv_reads_bounded_stored_bytes_preserving_escaped_identity_and_inputs() {
    let f = Fixture::new("mkv");
    fs::write(f.0.join(MKV_NAME), MKV_BYTES).unwrap();
    let before = inventory(&f.0);
    let mut a = app(&f.0);
    let original = snapshot(&a);
    let id = find(&mut a, OsStr::new(MKV_NAME));
    let bound = view(&a).bindings[&id].clone();
    assert_eq!(bound.entry.path, f.0.join(MKV_NAME));
    assert!(folder_scene::display(&bound.entry.name).contains("\\n\\t\\\"Tail.MkV"));
    tap(&mut a, KeyCode::KeyE); // actual production keyboard -> resolver -> inspector
    assert_mkv(&a, &bound.entry.path);
    assert_eq!(snapshot(&a), original);
    assert_eq!(view(&a).bindings[&id], bound);
    tap(&mut a, KeyCode::Escape);
    // Keep the retained snapshot, but remove ONLY this owned payload. A new E
    // must report failed acquisition, never recycle the previous body's bytes.
    let expected = view(&a).reader.clone();
    let permissions = fs::metadata(&bound.entry.path).unwrap().permissions();
    fs::remove_file(&bound.entry.path).unwrap();
    let removed = inventory(&f.0);
    aim(&mut a, &id);
    tap(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    let failed = a.world.document().unwrap().to_string();
    assert_ne!(failed, expected);
    assert!(failed.contains("STALE / READ ERROR: open reader:"));
    assert!(failed.contains("NO BODY READ: no successful byte range; encoding unknown."));
    assert!(
        failed
            .contains("No safe handle acquired. Escape then E explicitly tries acquisition again.")
    );
    assert!(failed.contains("Last good bytes/range retained; paging suspended."));
    assert!(failed.contains("TYPE: MKV video (playback unavailable)\n"));
    assert!(failed.contains(&format!(
        "NAME (escaped OS bytes):\n{}\n",
        folder_scene::display(&bound.entry.name)
    )));
    assert!(failed.contains(&format!(
        "SOURCE PATH (display only; may become stale):\n{}\n",
        folder_scene::display(bound.entry.path.as_os_str())
    )));
    for stale_body in [
        "RANGE [",
        "\nBYTE GROUPS (8 bytes max)\n",
        "\nOFFSET ",
        "4E 4F 54 20",
        "ASCII NOT A ME",
        "Codec:",
        "Duration:",
    ] {
        assert!(
            !failed.contains(stale_body),
            "no last-session body presented as fresh"
        );
    }
    for code in [KeyCode::ArrowRight, KeyCode::ArrowLeft, KeyCode::KeyC] {
        tap(&mut a, code);
        assert_eq!(a.world.document().unwrap(), failed);
    }
    assert!(a.web.is_none() && a.world.reader_link.is_none() && a.address.is_none());
    assert_eq!(inventory(&f.0), removed);
    assert_eq!(snapshot(&a), original);
    assert_eq!(view(&a).bindings[&id], bound);
    // Restore owned bytes/mode. C must not silently acquire even now; only E
    // after close is a new acquisition. Both inventories bracket App reads.
    fs::write(&bound.entry.path, MKV_BYTES).unwrap();
    fs::set_permissions(&bound.entry.path, permissions).unwrap();
    tap(&mut a, KeyCode::KeyC);
    assert_eq!(a.world.document().unwrap(), failed);
    assert_eq!(inventory(&f.0), before);
    tap(&mut a, KeyCode::Escape);
    aim(&mut a, &id);
    tap(&mut a, KeyCode::KeyE);
    assert_mkv(&a, &bound.entry.path);
    assert_eq!(snapshot(&a), original);
    assert_eq!(inventory(&f.0), before);
}

fn separate(a: &Path, b: &Path) -> bool {
    !a.starts_with(b) && !b.starts_with(a)
}
fn capture_group(
    root: &Path,
    fixture: &Path,
    output: &Path,
    label: &str,
) -> resources::SmokeOutput {
    let path = output.join(label);
    fs::create_dir(&path).expect("each capture group must be absent, never reused");
    let canonical = path.canonicalize().unwrap();
    // Same bidirectional canonical overlap policy as the executable, including
    // separately selected folder input. Bundled packs are already below root.
    assert!(separate(&canonical, &fixture.canonicalize().unwrap()));
    resources::smoke_output(&root.canonicalize().unwrap(), &canonical).unwrap()
}
fn tree_pixels(a: &App, file: &Path, size: [u32; 2]) {
    let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(file).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!([frame.width, frame.height], size);
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    assert_eq!(frame.buffer_size(), size[0] as usize * size[1] as usize * 4);
    assert!(
        pixels[..frame.buffer_size()]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 255)
    );
    let overlay = hair::text::overlay(&a.world, size[0], size[1], 0);
    let screen = |q: &[hair::assets::Vertex]| {
        let xs: Vec<_> = q
            .iter()
            .map(|v| ((v.position[0] + 1.0) * 0.5 * size[0] as f32).round() as i32)
            .collect();
        let ys: Vec<_> = q
            .iter()
            .map(|v| ((1.0 - v.position[1]) * 0.5 * size[1] as f32).round() as i32)
            .collect();
        (
            *xs.iter().min().unwrap(),
            *ys.iter().min().unwrap(),
            *xs.iter().max().unwrap(),
            *ys.iter().max().unwrap(),
        )
    };
    // tree_tests::capture assumes min(6, rows), which is invalid for deep local
    // ancestry at 640x480 (current viewport budget is five rows). Derive the
    // actual panel bounds from its unique tree-background quad instead.
    let panel: Vec<_> = overlay
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| q[0].color == [0.012; 3])
        .collect();
    assert_eq!(panel.len(), 1);
    let (left, top, right, bottom) = screen(panel[0]);
    assert!(left >= 0 && top >= 0 && right <= size[0] as i32 && bottom <= size[1] as i32);
    let (w, h) = ((right - left) as usize, (bottom - top) as usize);
    assert!(w > 0 && h > 0);
    let rgb = |c: f32| {
        let srgb = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    let mut expected = vec![[0u8; 3]; w * h];
    for q in overlay.as_chunks::<6>().0 {
        let (x0, y0, x1, y1) = screen(q);
        for y in y0.max(top)..y1.min(bottom) {
            for x in x0.max(left)..x1.min(right) {
                expected[(y - top) as usize * w + (x - left) as usize] = q[0].color.map(rgb);
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let at = ((y + top as usize) * size[0] as usize + x + left as usize) * 4;
            for c in 0..3 {
                assert!(
                    pixels[at + c].abs_diff(expected[y * w + x][c]) <= 1,
                    "local tree raster {} x={x} y={y} channel={c}",
                    file.display()
                );
            }
        }
    }
    println!(
        "QUALIFICATION_TREE_RASTER {} checked_pixels={}",
        file.display(),
        w * h
    );
}
fn retain_capture(
    a: &mut App,
    output: &resources::SmokeOutput,
    name: &str,
    size: [u32; 2],
    meaning: &str,
    errors: &mut Vec<String>,
) {
    assert_bindings(a);
    assert!(a.world.navigation.current_index(&a.world.current).is_some());
    a.update_hud();
    let vertices = a.themes[a.theme_index].vertices_for(&a.world);
    let renderer = a.renderer.as_mut().unwrap();
    assert_eq!(renderer.size, size);
    renderer.set_scene(&vertices);
    let result = renderer.capture(output, name, &a.pose, &a.world);
    println!(
        "QUALIFICATION_CAPTURE {} meaning={meaning:?} size={size:?} pack={} page={} reading={} collapsed={} source={:?} pose={:?} protected_writer_ok={}",
        output.path().join(name).display(),
        a.themes[a.theme_index].name,
        view(a).page,
        a.world.reading,
        a.world.tree_collapsed,
        a.world.current,
        pose(a),
        result.is_ok()
    );
    match result {
        Ok(()) => tree_pixels(a, &output.path().join(name), size),
        Err(error) => errors.push(format!("{}: {error}", output.path().join(name).display())),
    }
}

// Visual evidence uses a room-context camera, NEVER the 1.05m activation
// close-up. Navigation/aim proof is recorded before this temporary camera swap.
// Deliberate current-layout audit: front child slots and middle file positions
// did not move. Retain this overview rather than invent new GPU margins; the
// CPU framing test below checks it against the CURRENT rear inward direction.
fn context_pose() -> Pose {
    Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [-3.0, 5.0],
        yaw: 0.0,
        pitch: -0.08,
    }
}
fn retain_world_context(
    a: &mut App,
    output: &resources::SmokeOutput,
    name: &str,
    size: [u32; 2],
    meaning: &str,
    errors: &mut Vec<String>,
) {
    assert!(!a.world.reading);
    let saved_pose = a.pose;
    let saved_target = a.world.interaction.clone();
    let saved_explicit = a.world.explicit_target.clone();
    let stable = (
        snapshot(a),
        view(a).clone(),
        a.world.navigation.clone(),
        a.world.tree_collapsed,
    );
    a.pose = context_pose();
    assert!(!a.themes[a.theme_index].host.blocked(a.pose.position));
    a.world.explicit_target = None;
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
    assert!(
        a.world.interaction.is_none(),
        "room overview is not aimed/reachable E proof"
    );
    retain_capture(a, output, name, size, meaning, errors);
    a.pose = saved_pose;
    a.world.explicit_target = saved_explicit;
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
    assert_eq!(
        a.world.interaction, saved_target,
        "restore real resolver state before E"
    );
    assert_eq!(
        (
            snapshot(a),
            view(a).clone(),
            a.world.navigation.clone(),
            a.world.tree_collapsed
        ),
        stable
    );
    assert_eq!(
        pose(a),
        (saved_pose.position, saved_pose.yaw, saved_pose.pitch)
    );
}
#[test]
fn context_camera_frames_last_page_folder_and_real_file_objects_without_claiming_pixels() {
    use glam::{Mat4, Vec3};
    let f = Fixture::new("context-camera");
    f.children(25);
    fs::write(f.0.join("Read Case.txt"), b"CONTEXT CAMERA FIXTURE\n").unwrap();
    fs::write(f.0.join(MKV_NAME), MKV_BYTES).unwrap();
    let mut a = app(&f.0);
    find(&mut a, OsStr::new("Child 024"));
    let aimed = a.world.interaction.clone();
    assert!(aimed.is_some());
    let camera = context_pose();
    let host = &a.themes[a.theme_index].host;
    // 25 immediate folders enlarge only X; the two files keep minimum depth.
    assert_eq!(host.room, [-14.0, -6.0, 14.0, 6.0]);
    let inward = host.portal_inward(folder_scene::PARENT_PORTAL);
    assert_eq!(
        inward,
        [0.0, -1.0],
        "current rear entry, not old front Parent"
    );
    assert!(
        camera.yaw.sin() * inward[0] - camera.yaw.cos() * inward[1] > 0.99,
        "context camera faces from rear into room"
    );
    assert!(camera.position[1] < host.spawn[1]);
    assert!(!host.blocked(camera.position));
    assert!(hair::interaction::resolve(host, &camera, &a.world).is_none());
    let eye = Vec3::new(camera.position[0], 1.65, camera.position[1]);
    let forward = Vec3::new(
        camera.yaw.sin() * camera.pitch.cos(),
        camera.pitch.sin(),
        -camera.yaw.cos() * camera.pitch.cos(),
    );
    for size in [[960, 640], [640, 480]] {
        let matrix = Mat4::perspective_rh(
            65.0_f32.to_radians(),
            size[0] as f32 / size[1] as f32,
            0.05,
            50.0,
        ) * Mat4::look_to_rh(eye, forward, Vec3::Y);
        for name in ["Child 024", "Read Case.txt", MKV_NAME] {
            let id = view(&a)
                .bindings
                .iter()
                .find(|(_, b)| b.entry.name == name)
                .unwrap()
                .0;
            let anchor = host.anchors.iter().find(|v| &v.id == id).unwrap();
            let at = Vec3::new(
                anchor.at[0],
                anchor.at[1] + anchor.size[1] * 0.5,
                anchor.at[2],
            );
            assert!((at - eye).length() > 5.0, "not a doorway close-up");
            let clip = matrix * at.extend(1.0);
            assert!(clip.w > 0.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() < 0.92 && ndc.y.abs() < 0.8 && (0.0..1.0).contains(&ndc.z),
                "source anchor centre outside context framing: {name:?} {size:?} {ndc:?}"
            );
        }
    }
    assert_eq!(
        a.world.interaction, aimed,
        "framing calculation cannot replace aimed proof"
    );
    println!(
        "CONTEXT_CAMERA_CPU source anchor centres only; not mesh visibility, surfaces, color count or pixels"
    );
}

#[test]
#[ignore = "retained serialized Metal proof; requires absent HAIR_QUALIFICATION_ROOT inside owned workspace"]
fn retained_current_source_local_metal_matrix() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let owned = root.parent().unwrap().canonicalize().unwrap();
    let requested = PathBuf::from(
        std::env::var_os("HAIR_QUALIFICATION_ROOT").expect("explicit absent retained output root"),
    );
    assert!(requested.is_absolute());
    let parent = requested.parent().unwrap().canonicalize().unwrap();
    assert!(
        parent.starts_with(&owned),
        "only owned milestone output paths"
    );
    let output = parent.join(requested.file_name().unwrap());
    assert!(
        separate(&root, &output),
        "output cannot overlap resources, including aliases"
    );
    fs::create_dir(&output)
        .expect("output root must be absent; never overwrite/retry over retained evidence");
    // Created at TEST EXECUTION, after parent built this exact test executable.
    // Retained along with captures; deliberately not a temporary Fixture guard.
    let fixture = output.join("fixture");
    fs::create_dir(&fixture).unwrap();
    for n in 0..25 {
        fs::create_dir(fixture.join(format!("Child {n:03}"))).unwrap();
    }
    fs::write(
        fixture.join("Read Case.txt"),
        b"POST-BUILD SYNTHETIC TEXT\nCase and spaces are literal.\nNo recompilation needed.\n",
    )
    .unwrap();
    fs::write(fixture.join(MKV_NAME), MKV_BYTES).unwrap();
    let before = inventory(&fixture);
    assert_eq!(fs::read_dir(&fixture).unwrap().count(), 27);
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(
        adapter.get_info().backend,
        wgpu::Backend::Metal,
        "this retained gate is specifically Metal"
    );
    let mut errors = Vec::new();
    for (label, size) in [("normal", [960, 640]), ("small", [640, 480])] {
        let mut a = app(&fixture);
        assert_eq!(snapshot(&a).entries.len(), 27);
        assert_eq!(view(&a).pages(), 3);
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &a.themes[a.theme_index].vertices_for(&a.world),
        ))
        .unwrap();
        renderer
            .set_materials(&a.themes[a.theme_index].textures)
            .unwrap();
        pollster::block_on(renderer.error_probe()).unwrap();
        a.renderer = Some(renderer);
        for (theme_index, pack) in ["office", "archive"].iter().enumerate() {
            if theme_index > 0 {
                let stable = (
                    snapshot(&a),
                    view(&a).clone(),
                    pose(&a),
                    a.world.tree_collapsed,
                    a.world.navigation.clone(),
                );
                tap(&mut a, KeyCode::KeyT); // renderer-backed switch, not index assignment
                assert_eq!(
                    (
                        snapshot(&a),
                        view(&a).clone(),
                        pose(&a),
                        a.world.tree_collapsed,
                        a.world.navigation.clone()
                    ),
                    stable
                );
            }
            assert_eq!(a.theme_index, theme_index);
            assert!(
                a.themes[a.theme_index]
                    .source
                    .ends_with(format!("{pack}.toml"))
            );
            assert!(!a.themes[a.theme_index].load_failed);
            let world = capture_group(&root, &fixture, &output, &format!("{label}-{pack}-world"));
            let text = capture_group(&root, &fixture, &output, &format!("{label}-{pack}-text"));
            let mkv = capture_group(&root, &fixture, &output, &format!("{label}-{pack}-mkv"));
            while view(&a).page > 0 {
                door(&mut a, "folder-previous");
            }
            if a.world.tree_collapsed {
                tap(&mut a, KeyCode::KeyM);
            }
            a.pose = Pose {
                eye_offset: 1.65,
                feet_y: 0.0,
                position: a.themes[a.theme_index].host.spawn,
                yaw: 0.0,
                pitch: -0.13,
            };
            a.world.explicit_target = None;
            hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
            assert!(!a.world.reading && !a.world.tree_collapsed);
            retain_world_context(
                &mut a,
                &world,
                "office.png",
                size,
                "page0 room-context view; closed reader; expanded local tree; NOT aimed target proof",
                &mut errors,
            );
            let child_id = find(&mut a, OsStr::new("Child 024"));
            assert_eq!(view(&a).page, 2);
            assert!(!view(&a).continuations.contains_key("folder-next"));
            let aimed = a.world.interaction.clone();
            println!(
                "QUALIFICATION_AIM_PROOF pack={pack} size={size:?} anchor={child_id} pose={:?} target={aimed:?}; following captures use a separate room-context camera",
                pose(&a)
            );
            retain_world_context(
                &mut a,
                &world,
                "reader-welcome.png",
                size,
                "last continuation room context with Child024 and real text/MKV objects; closed reader; expanded tree; NOT aimed target proof",
                &mut errors,
            );
            assert_eq!(a.world.interaction, aimed);
            tap(&mut a, KeyCode::KeyM);
            assert!(a.world.tree_collapsed && !a.world.reading);
            retain_world_context(
                &mut a,
                &world,
                "reader-workshop.png",
                size,
                "same last-room context; closed reader; collapsed tree; NOT aimed target proof",
                &mut errors,
            );
            assert_eq!(a.world.interaction, aimed);
            aim(&mut a, &child_id); // explicit fresh real resolver assertion before E
            tap(&mut a, KeyCode::KeyE);
            assert_eq!(snapshot(&a).folder, fixture.join("Child 024"));
            door(&mut a, folder_scene::PARENT_PORTAL);
            assert_eq!(snapshot(&a).folder, fixture);
            assert_eq!(view(&a).page, 2);
            assert_child_arrival(&a, "Child 024");
            assert!(a.world.tree_collapsed);
            tap(&mut a, KeyCode::KeyM);
            find(&mut a, OsStr::new("Read Case.txt"));
            tap(&mut a, KeyCode::KeyE);
            assert!(a.world.reading && !a.world.tree_collapsed);
            let document = a.world.document().unwrap();
            let body = document
                .split_once("\nTEXT (links inactive; escapes shown; range only):\n")
                .unwrap()
                .1
                .split_once("\n\nFULL SOURCE / METADATA:\n")
                .unwrap()
                .0;
            let original_body = "POST-BUILD SYNTHETIC TEXT\nCase and spaces are literal.\nNo recompilation needed.\n";
            assert_eq!(body, original_body);
            assert!(document.contains(&format!(
                "RANGE [0x{:016X}, 0x{:016X})\nObserved length: {} bytes | EOF (next inert)",
                0,
                original_body.len(),
                original_body.len()
            )));
            let reader = a.world.document().unwrap().to_string();
            retain_capture(
                &mut a,
                &text,
                "office.png",
                size,
                "actual E text reader; expanded local tree",
                &mut errors,
            );
            tap(&mut a, KeyCode::KeyM);
            assert!(a.world.reading && a.world.tree_collapsed);
            assert_eq!(a.world.document().unwrap(), reader);
            retain_capture(
                &mut a,
                &text,
                "reader-welcome.png",
                size,
                "same literal text reader; collapsed tree",
                &mut errors,
            );
            tap(&mut a, KeyCode::Escape);
            tap(&mut a, KeyCode::KeyM);
            find(&mut a, OsStr::new(MKV_NAME));
            tap(&mut a, KeyCode::KeyE);
            assert_mkv(&a, &fixture.join(MKV_NAME));
            assert!(!a.world.tree_collapsed);
            let reader = a.world.document().unwrap().to_string();
            retain_capture(
                &mut a,
                &mkv,
                "office.png",
                size,
                "actual E MKV stored bytes and metadata; expanded local tree; playback unavailable",
                &mut errors,
            );
            tap(&mut a, KeyCode::KeyM);
            assert!(a.world.reading && a.world.tree_collapsed);
            assert_eq!(a.world.document().unwrap(), reader);
            retain_capture(
                &mut a,
                &mkv,
                "reader-welcome.png",
                size,
                "same MKV stored-bytes reader; collapsed tree; no media decoding",
                &mut errors,
            );
            tap(&mut a, KeyCode::Escape);
            assert_eq!(
                inventory(&fixture),
                before,
                "product must preserve fixture bytes/modes/membership"
            );
            pollster::block_on(a.renderer.as_ref().unwrap().error_probe()).unwrap();
        }
        a.renderer.as_ref().unwrap().wait().unwrap();
    }
    assert_eq!(inventory(&fixture), before);
    println!(
        "QUALIFICATION_MATRIX state assertions completed; retained root={}; protected capture failures={errors:?}; synthetic App input + offscreen GPU only, NOT native input or pixel approval",
        output.display()
    );
    assert!(
        errors.is_empty(),
        "retain failed capture evidence; never lower generic guard or overwrite: {errors:?}"
    );
    let mut retained_pngs = 0;
    for label in ["normal", "small"] {
        for pack in ["office", "archive"] {
            for (category, names) in [
                (
                    "world",
                    vec!["office.png", "reader-welcome.png", "reader-workshop.png"],
                ),
                ("text", vec!["office.png", "reader-welcome.png"]),
                ("mkv", vec!["office.png", "reader-welcome.png"]),
            ] {
                let group = output.join(format!("{label}-{pack}-{category}"));
                let actual: BTreeSet<_> = fs::read_dir(&group)
                    .unwrap()
                    .map(|entry| {
                        let entry = entry.unwrap();
                        assert!(entry.file_type().unwrap().is_file());
                        entry.file_name()
                    })
                    .collect();
                let expected: BTreeSet<_> = names
                    .iter()
                    .map(|name| std::ffi::OsString::from(*name))
                    .collect();
                assert_eq!(
                    actual,
                    expected,
                    "exact retained PNG group {}",
                    group.display()
                );
                retained_pngs += actual.len();
            }
        }
    }
    println!(
        "QUALIFICATION_RETAINED_MEMBERSHIP pngs={retained_pngs}; parent must independently inspect these exact pixels before any visual acceptance"
    );
}
