use hair::web::{Address, Page};
use hair::{Input, Pose};
fn pocket(html: &str) -> hair::web::Generated {
    Page::html(
        Address::parse("https://example.org/connected").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
}
fn walk(theme: &hair::theme::Theme, pose: &mut Pose, target: [f32; 2]) {
    let input = Input {
        forward: true,
        captured: true,
        ..Default::default()
    };
    for _ in 0..250 {
        let dx = target[0] - pose.position[0];
        let dz = target[1] - pose.position[1];
        if dx.hypot(dz) < 0.06 {
            return;
        }
        pose.yaw = dx.atan2(-dz);
        theme.locomote(pose, &input, 0.02);
        assert!(!theme.blocked(pose.position));
    }
    panic!(
        "actual locomotion could not reach {:?} from {:?}",
        target, pose.position
    );
}
#[test]
fn actual_locomotion_crosses_passage_both_directions_and_walls_stop_it_in_both_packs() {
    let g = pocket("<h1>Orchard</h1><p>Apples.</p><h2>Harbour</h2><p>Boats.</p>");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cache = hair::theme::AssetCache::default();
    let mut carried_pose = None;
    for pack in ["office", "archive"] {
        let theme = hair::theme::Theme::load_for_scene(
            &root.join(format!("packs/{pack}.toml")),
            &mut cache,
            &g.host,
        )
        .unwrap();
        assert_eq!(theme.host, g.host);
        let mut pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: g.host.spawn,
            yaw: 0.0,
            pitch: 0.0,
        };
        for target in [
            [1.5, 3.0],
            [1.5, -1.0],
            [-4.0, -1.0],
            [1.5, -1.0],
            [1.5, 3.0],
        ] {
            walk(&theme, &mut pose, target);
        }
        if let Some(previous) = carried_pose {
            assert_eq!(pose.position, previous);
        }
        carried_pose = Some(pose.position);
        pose.position = [-3.0, 2.4];
        pose.yaw = std::f32::consts::FRAC_PI_2;
        let input = Input {
            forward: true,
            captured: true,
            ..Default::default()
        };
        for _ in 0..50 {
            theme.locomote(&mut pose, &input, 0.25);
        }
        assert!(pose.position[0] < -1.7, "must not tunnel through partition");
        assert_eq!(g.world.current, "https://example.org/connected");
    }
}
#[test]
fn maximum_grouped_portals_stay_reachable_with_partitions_and_many_reading_objects() {
    let mut html = (0..8)
        .map(|i| format!("<h2>Section{i}</h2><p>Text{i}</p>"))
        .collect::<String>();
    for i in 0..64 {
        html.push_str(&format!("<a href='/destination{i}'>Link{i}</a>"));
    }
    let g = pocket(&html);
    assert_eq!(
        g.doors.iter().map(|d| d.destinations.len()).sum::<usize>(),
        64
    );
    assert_eq!(g.world.pocket.as_ref().unwrap().documents.len(), 8);
    g.host.check_routes(&[[-4.0, -1.0], [4.0, 0.0]]).unwrap();
}
#[test]
fn reading_a_section_does_not_erase_the_pockets_portal_graph() {
    let mut g = pocket("<h2>Orchard</h2>Apples<h2>Harbour</h2>Boats<a href='/next'>Next</a>");
    let anchor = g.doors[0].anchor.clone();
    let before = hair::web::portals::view(&g.world, &anchor).unwrap();
    g.world.reading = true;
    g.world.pocket.as_mut().unwrap().reading = g
        .world
        .pocket
        .as_ref()
        .unwrap()
        .documents
        .keys()
        .next()
        .cloned();
    assert_eq!(hair::web::portals::view(&g.world, &anchor), Some(before));
}

#[test]
fn additional_sections_do_not_rearrange_partition_or_existing_portals() {
    let a = pocket("<h2>Orchard</h2>Apples<h2>Harbour</h2>Boats<a href='/next'>Next</a>");
    let b = pocket(
        "<h2>Mountain</h2>Stones<h2>Harbour</h2>Boats<h2>Orchard</h2>Apples<a href='/next'>Next</a>",
    );
    assert!(
        b.host.room[2] - b.host.room[0] > a.host.room[2] - a.host.room[0],
        "reading sections independently add width"
    );
    assert_eq!(
        a.host.room[3] - a.host.room[1],
        b.host.room[3] - b.host.room[1]
    );
    assert_eq!(a.host.spawn, b.host.spawn);
    assert_eq!(a.host.geometry, b.host.geometry);
    for anchor in a.host.anchors.iter().filter(|a| a.role == "portal") {
        assert_eq!(
            b.host.anchors.iter().find(|b| b.id == anchor.id),
            Some(anchor)
        );
    }
}
