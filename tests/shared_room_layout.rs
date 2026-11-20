use hair::web::{Address, Generated, Page};
#[test]
fn repeated_links_do_not_lengthen_the_physical_room() {
    let a = generated(0, "<a href='/same'>Same</a>");
    let b = generated(0, &"<a href='/same'>Same</a>".repeat(64));
    assert_eq!(a.doors, b.doors);
    assert_eq!(a.host.room, b.host.room);
}
#[test]
fn every_distinct_target_has_its_own_physical_door() {
    let g = generated(64, "");
    assert_eq!(
        g.doors.len(),
        64,
        "physical doors must not hash multiple links into eight slots"
    );
    for d in &g.doors {
        assert_eq!(d.destinations.len(), 1);
        assert_eq!(
            hair::web::door_destination(&g.world, &d.anchor),
            Some(d.destinations[0].clone())
        );
    }
    let actual = g
        .doors
        .iter()
        .flat_map(|d| d.destinations.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = (0..64)
        .map(|i| format!("https://example.org/target-{i}"))
        .collect();
    assert_eq!(
        actual, expected,
        "doors must cover the independently specified input destinations"
    );
    let mut ids = std::collections::HashSet::new();
    for d in &g.doors {
        assert!(ids.insert(&d.anchor));
    }
    g.host.check_routes(&[]).unwrap();
}
#[test]
fn every_door_is_aimable_at_its_actual_side_and_restores_its_exact_target() {
    let g = generated(64, "");
    let mut sides = std::collections::HashSet::new();
    for d in &g.doors {
        let a = g.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        let inward = g.host.portal_inward(&d.anchor);
        sides.insert([inward[0] as i32, inward[1] as i32]);
        let pose = hair::Pose {
            position: [a.at[0] + inward[0], a.at[2] + inward[1]],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: -0.2,
        };
        assert!(!g.host.blocked(pose.position));
        let target = hair::interaction::resolve(&g.host, &pose, &g.world)
            .expect("each real door needs unobstructed aim/reach");
        assert_eq!(target.anchor, d.anchor);
        assert_eq!(target.destination, Some(d.destinations[0].clone()));
    }
    assert_eq!(
        sides.len(),
        4,
        "dense rooms should distribute doors over all sides"
    );
}
#[test]
fn fragments_and_queries_have_separate_doors_but_duplicate_hrefs_do_not() {
    let g = generated(
        0,
        "<a href='/doc?q=AB%2Fcd#one'>One</a><a href='/doc?q=AB%2Fcd#two'>Two</a><a href='/doc?q=AB%2Fcd#one'>Duplicate</a>",
    );
    assert_eq!(g.doors.len(), 2);
    assert_ne!(g.doors[0].anchor, g.doors[1].anchor);
    for d in &g.doors {
        assert_eq!(d.destinations.len(), 1);
        assert!(d.destinations[0].contains("?q=AB%2Fcd#"));
    }
}
#[test]
fn internal_generated_profile_is_not_accepted_as_an_authored_host() {
    let g = generated(64, "");
    assert_eq!(g.host.version, 3);
    assert!(hair::host_scene::HostScene::parse(&toml::to_string(&g.host).unwrap()).is_err());
    let old = hair::host_scene::HostScene::shared_default();
    assert_eq!(
        hair::host_scene::HostScene::parse(&toml::to_string(&old).unwrap()).unwrap(),
        old
    );
}
#[test]
fn tab_moves_between_nearby_individual_doors_instead_of_reselecting_one() {
    let mut g = generated(2, "");
    let pose = hair::Pose {
        position: [-3.6, g.host.room[1] + 0.5],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::cycle(&g.host, &pose, &mut g.world, false);
    let first = g
        .world
        .interaction
        .as_ref()
        .expect("first eligible door")
        .anchor
        .clone();
    hair::interaction::cycle(&g.host, &pose, &mut g.world, false);
    let second = g
        .world
        .interaction
        .as_ref()
        .expect("second eligible door")
        .anchor
        .clone();
    assert_ne!(
        first, second,
        "single-destination doors must not swallow Tab"
    );
    hair::interaction::cycle(&g.host, &pose, &mut g.world, true);
    assert_eq!(g.world.interaction.as_ref().unwrap().anchor, first);
}
#[test]
fn maximum_link_load_and_reading_objects_keep_both_continuations_reachable() {
    let links = |start: usize| {
        (start..start + 64)
            .map(|i| format!("<a href='/item-{i}'>Item {i}</a>"))
            .collect::<String>()
    };
    let prose = (0..64)
        .map(|i| format!("<h2>Section {i}</h2><p>Content {i}</p>"))
        .collect::<String>();
    let html = format!(
        "{}{}{}{}{}",
        links(0),
        links(64),
        prose,
        links(128),
        links(192)
    );
    let first = Page::html(
        Address::parse("https://example.org/max").unwrap(),
        html.as_bytes(),
    )
    .unwrap();
    let middle = first.next_window(None).unwrap().unwrap();
    let g = middle.generate().unwrap();
    assert_eq!(g.doors.len(), 64);
    assert_eq!(g.window_doors.len(), 2);
    g.host.check_routes(&[]).unwrap();
    for d in &g.window_doors {
        let a = g.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        let inward = g.host.portal_inward(&d.anchor);
        let pose = hair::Pose {
            position: [a.at[0] + inward[0], a.at[2] + inward[1]],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: -0.2,
        };
        let target =
            hair::interaction::resolve_with_windows(&g.host, &pose, &g.world, &g.window_doors)
                .unwrap();
        assert_eq!(target.anchor, d.anchor);
    }
}
#[test]
fn map_layout_is_a_literal_snapshot_of_committed_physical_doors() {
    let g = generated(64, "");
    let layout = hair::exploration::RoomLayout::observe(&g.host, &g.world).unwrap();
    assert_eq!(layout.bounds, g.host.room);
    assert_eq!(
        layout.ports.len(),
        65,
        "all link doors plus typed history door"
    );
    for d in &g.doors {
        let physical = g.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        let port = layout.ports.iter().find(|p| p.anchor == d.anchor).unwrap();
        assert_eq!(port.position, [physical.at[0], physical.at[2]]);
        assert_eq!(port.inward, g.host.portal_inward(&d.anchor));
        assert_eq!(
            port.target,
            Some(hair::exploration::RoomId::new(&d.destinations[0], 0))
        );
    }
}
#[test]
fn wall_images_never_cover_individual_door_openings() {
    let g = generated(64, "");
    for image in g.host.anchors.iter().filter(|a| a.role == "image") {
        for opening in g
            .host
            .wall_apertures
            .iter()
            .filter(|o| o.wall == hair::host_scene::Wall::Left)
        {
            let door = g
                .host
                .anchors
                .iter()
                .find(|a| a.id == opening.anchor)
                .unwrap();
            let image_lo = image.at[2] - image.size[2] * 0.5;
            let image_hi = image.at[2] + image.size[2] * 0.5;
            let door_lo = door.at[2] - opening.width * 0.5;
            let door_hi = door.at[2] + opening.width * 0.5;
            assert!(
                image_hi <= door_lo || image_lo >= door_hi,
                "image {} covers opening {}",
                image.id,
                door.id
            );
        }
    }
}
fn generated(links: usize, prose: &str) -> Generated {
    let html = format!(
        "<title>Shared layout</title>{prose}{}",
        (0..links)
            .map(|i| format!("<a href='/target-{i}'>Link {i}</a>"))
            .collect::<String>()
    );
    Page::html(
        Address::parse("https://example.org/layout").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
}
#[test]
fn link_density_extends_one_axis_without_forcing_square_rooms() {
    let small = generated(1, "");
    let hall = generated(64, "");
    let dimensions = |g: &Generated| {
        [
            g.host.room[2] - g.host.room[0],
            g.host.room[3] - g.host.room[1],
        ]
    };
    let a = dimensions(&small);
    let b = dimensions(&hall);
    assert!(
        b[1] > a[1],
        "more links should extend the hallway length: {a:?} -> {b:?}"
    );
    assert_eq!(
        b[0], a[0],
        "link-only growth must not force both axes to grow"
    );
    assert!(
        b[1] > b[0],
        "a link-heavy page should be allowed to form a long hallway"
    );
    assert_eq!(
        hall.doors
            .iter()
            .map(|d| d.destinations.len())
            .sum::<usize>(),
        64,
        "layout never silently drops destinations"
    );
    hall.host.check_routes(&[]).unwrap();
}
#[test]
fn reading_objects_add_width_without_changing_link_hallway_length() {
    let hall = generated(24, "");
    let reading = generated(
        24,
        "<h1>First</h1><p>Reading content.</p><h2>Second</h2><p>More content.</p>",
    );
    let dimensions = |g: &Generated| {
        [
            g.host.room[2] - g.host.room[0],
            g.host.room[3] - g.host.room[1],
        ]
    };
    assert!(
        dimensions(&reading)[0] > dimensions(&hall)[0],
        "reading objects should get additional open width"
    );
    assert_eq!(
        dimensions(&reading)[1],
        dimensions(&hall)[1],
        "adding reading space must not force proportional growth"
    );
    reading.host.check_routes(&[]).unwrap();
}
