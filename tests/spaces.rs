use hair::host_scene::HostScene;
use std::path::Path;
fn example() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/GreenleafReadingRoom-spaces")
}
#[test]
fn explicit_site_mandatory_host_is_not_ignored() {
    let root = std::env::temp_dir().join(format!("hair-spaces-invalid-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("index.md"), "# Local").unwrap();
    std::fs::write(root.join("host-scene.toml"), "version=999\n").unwrap();
    let result = hair::World::load_site(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        result.is_err(),
        "malformed mandatory host must refuse before state load"
    );
}
#[test]
fn explicit_example_loads() {
    let w = hair::World::load_site(&example()).unwrap();
    assert_eq!(w.spaces.as_ref().unwrap().spaces.len(), 3);
    assert_eq!(w.current_space, "reading-room");
}
#[test]
fn explicit_graph_rejects_overlap_unreachable_arrivals_and_second_door() {
    let w = hair::World::load_site(&example()).unwrap();
    let site = w.site.as_ref().unwrap();
    let good = w.spaces.as_ref().unwrap();
    let mut g = good.clone();
    g.spaces[0].scene.geometry.push(hair::host_scene::Geometry {
        at: [0.0, 0.1, 0.1],
        size: [0.3, 0.4, 0.3],
    });
    assert!(
        g.validate(site).is_err(),
        "geometry overlaps reserved document envelope"
    );
    let mut g = good.clone();
    g.spaces[0].scene.geometry.push(hair::host_scene::Geometry {
        at: [0.8, 0.0, 0.0],
        size: [0.1, 3.0, 6.0],
    });
    assert!(
        g.validate(site).is_err(),
        "second door and arrival cut off from first spawn"
    );
}
#[test]
fn explicit_graph_reference_and_budget_matrix() {
    let w = hair::World::load_site(&example()).unwrap();
    let site = w.site.as_ref().unwrap();
    let good = w.spaces.as_ref().unwrap();
    for case in 0..16 {
        let mut g = good.clone();
        match case {
            0 => g.version = 3,
            1 => g.spaces[0].doors[0].space = "missing".into(),
            2 => g.spaces[0].doors[0].page = "missing.md".into(),
            3 => g.spaces[0].doors[0].arrival = "missing".into(),
            4 => g.spaces[1].pages.push("index.md".into()),
            5 => g.spaces[0].documents[0].anchor = "missing".into(),
            6 => g.spaces[0].arrivals[0].yaw_degrees = 0.0,
            7 => g.spaces[0].arrivals[0].at = [0.0, 0.1],
            8 => g.spaces[0].scene.anchors[0].binding = "current-document".into(),
            9 => g.spaces[0].scene.anchors[0].at[0] = f32::NAN,
            10 => {
                g.spaces[0].doors[0].space = "reading-room".into();
                g.spaces[0].doors[0].page = "index.md".into();
                g.spaces[0].doors[0].arrival = "from-notice".into();
                g.spaces[0].doors[1] = g.spaces[0].doors[0].clone();
            }
            11 => g.spaces[0].arrivals = vec![g.spaces[0].arrivals[0].clone(); 17],
            12 => g.spaces = vec![g.spaces[0].clone(); 9],
            13 => g.entry_arrival = "absent".into(),
            14 => g.spaces[0].pages.clear(),
            _ => {
                let a = g.spaces[0].scene.anchors[0].clone();
                g.spaces[0].scene.anchors.push(a)
            }
        }
        assert!(g.validate(site).is_err(), "accepted invalid case {case}");
    }
}
#[test]
fn explicit_host_envelopes_keep_legacy_valid_and_allow_repeated_bound_objects() {
    let old = include_str!("../host-scene.toml");
    assert!(HostScene::parse(old).is_ok());
    let explicit = old
        .replace("version = 1", "version = 2")
        .replace("current-document", "explicit-page")
        .replace("selected-target", "explicit-door");
    assert!(
        HostScene::parse(&explicit).is_ok(),
        "new explicit scene version"
    );
    assert!(HostScene::parse(&explicit.replace("explicit-page", "script")).is_err());
}
