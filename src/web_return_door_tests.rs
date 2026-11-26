//! History return door: real opposite-wall geometry, aim/reach through the
//! shared interaction resolver, typed history action via the App E adapter.
use super::*;

fn facing_rear(at: [f32; 3]) -> Pose {
    Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [at[0], 3.0],
        yaw: std::f32::consts::PI,
        pitch: -0.4,
    }
}
fn target(a: &App) -> Option<hair::interaction::Target> {
    hair::interaction::resolve_with_windows(
        &a.themes[a.theme_index].host,
        &a.pose,
        &a.world,
        &a.web.as_ref().unwrap().window_doors,
    )
}

#[test]
fn physical_return_emerges_from_matching_door_facing_inward_but_r_keeps_saved_view() {
    fn dense(address: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
        let html = (0..64)
            .map(|i| format!("<a href='/child-{i}'>Child {i}</a>"))
            .collect::<String>();
        Page::html(address, html.as_bytes())
    }
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = dense;
    a.request_web("https://example.org/dense-return", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    let original = a.world.current.clone();
    let original_bounds = a.themes[0].host.room;
    let mut directions = std::collections::HashSet::new();
    let cases: Vec<_> = a
        .web
        .as_ref()
        .unwrap()
        .doors
        .iter()
        .filter_map(|d| {
            let host = &a.themes[0].host;
            let n = host.portal_inward(&d.anchor);
            directions.insert([n[0] as i32, n[1] as i32]).then(|| {
                let anchor = host
                    .anchors
                    .iter()
                    .find(|anchor| anchor.id == d.anchor)
                    .unwrap();
                (
                    d.resource.clone(),
                    d.anchor.clone(),
                    anchor.at,
                    anchor.size,
                    n,
                )
            })
        })
        .collect();
    assert_eq!(
        cases.len(),
        4,
        "valid doorway fixture covers front, rear and both side walls"
    );
    for (destination, id, at, size, n) in cases {
        a.web.as_mut().unwrap().fetcher = dense;
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [at[0] + n[0] * 1.05, at[2] + n[1] * 1.05],
            yaw: (-n[0]).atan2(n[1]),
            pitch: ((at[1] + size[1] * 0.5 - 1.65) / 1.05).atan(),
        };
        a.world.explicit_target = None;
        assert!(!a.themes[0].host.blocked(a.pose.position));
        assert_eq!(
            target(&a).unwrap().anchor,
            id,
            "actually aim at departure door"
        );
        a.key(KeyCode::KeyE, true, false).unwrap();
        a.key(KeyCode::KeyE, false, false).unwrap();
        finish(&mut a);
        assert_eq!(a.world.current, destination);
        a.web.as_mut().unwrap().fetcher = |_, _, _| panic!("return must not refetch held page");
        let rear = a.themes[0]
            .host
            .anchors
            .iter()
            .find(|door| door.id == "return-history")
            .unwrap();
        let r = a.themes[0].host.portal_inward(&rear.id);
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [rear.at[0] + r[0] * 1.05, rear.at[2] + r[1] * 1.05],
            yaw: (-r[0]).atan2(r[1]),
            pitch: ((rear.at[1] + rear.size[1] * 0.5 - 1.65) / 1.05).atan(),
        };
        assert_eq!(target(&a).unwrap().anchor, "return-history");
        a.key(KeyCode::KeyE, true, false).unwrap();
        a.key(KeyCode::KeyE, false, false).unwrap();
        assert_eq!(a.world.current, original);
        assert_eq!(a.themes[0].host.room, original_bounds);
        let forward = [a.pose.yaw.sin(), -a.pose.yaw.cos()];
        assert!(
            forward[0] * n[0] + forward[1] * n[1] > 0.999,
            "physical return should face into room from {id}, not back at that door; yaw={}",
            a.pose.yaw
        );
        assert!(!a.themes[0].host.blocked(a.pose.position));
        let offset = [a.pose.position[0] - at[0], a.pose.position[1] - at[2]];
        assert!(
            offset[0] * n[0] + offset[1] * n[1] > 0.5 && offset[0].hypot(offset[1]) < 2.5,
            "return beside matching door"
        );
        assert_ne!(target(&a).map(|t| t.anchor), Some(id));
        // Keyboard history keeps exact viewpoint/reader state independently.
        a.world.reading = true;
        a.scroll = 2;
        let saved = a.pose;
        a.web.as_mut().unwrap().fetcher = dense;
        a.request_web(&destination, web_native::Mode::Follow)
            .unwrap();
        finish(&mut a);
        a.web.as_mut().unwrap().fetcher = |_, _, _| panic!("keyboard Back must not fetch");
        a.key(KeyCode::KeyR, true, false).unwrap();
        a.key(KeyCode::KeyR, false, false).unwrap();
        assert_eq!(
            (a.pose.position, a.pose.yaw, a.pose.pitch),
            (saved.position, saved.yaw, saved.pitch)
        );
        assert!(a.world.reading && a.scroll == 2);
        a.world.reading = false;
    }
}

#[test]
fn return_door_is_an_aimable_rear_wall_aperture_opposite_link_doors() {
    let a = app();
    let host = &a.themes[0].host;
    let door = host
        .anchors
        .iter()
        .find(|x| x.id == "return-history")
        .expect("return landmark");
    let links: Vec<_> = host
        .anchors
        .iter()
        .filter(|x| x.id.starts_with("web-slot-"))
        .collect();
    assert!(!links.is_empty());
    for link in &links {
        assert!(link.at[2] < host.room[1], "link doors on the open front");
    }
    assert!(
        door.at[2] > host.room[3],
        "return door on the opposite (rear) wall"
    );
    assert_eq!(
        host.portal_inward("return-history"),
        [0.0, -1.0],
        "faces inward"
    );
    assert!(
        host.wall_apertures
            .iter()
            .any(|w| w.anchor == "return-history" && w.wall == hair::host_scene::Wall::Rear),
        "real opening in the rear wall, not a door painted behind it"
    );
    let door = door.at;
    let mut a = a;
    a.pose = facing_rear(door);
    assert!(!a.themes[0].host.blocked(a.pose.position));
    let t = target(&a).expect("aimable and reachable from inside the room");
    assert_eq!(t.anchor, "return-history");
    // Initial page: honestly unavailable, never a fabricated destination.
    assert!(t.destination.is_none());
    assert!(
        t.prompt.to_ascii_lowercase().contains("no previous page"),
        "{}",
        t.prompt
    );
    // The default respawn faces the link doors, not the return door.
    let mut b = app();
    b.themes[0].respawn(&mut b.pose);
    assert_ne!(target(&b).map(|t| t.anchor), Some("return-history".into()));
}

#[test]
fn return_door_labels_previous_page_and_e_runs_exact_history_return() {
    let mut a = app();
    let first = a.world.current.clone();
    a.world.reading = true;
    a.scroll = 2;
    a.world.cycle_reader_link(false);
    a.key(KeyCode::KeyE, true, false).unwrap();
    a.key(KeyCode::KeyE, false, false).unwrap();
    finish(&mut a);
    assert_ne!(a.world.current, first, "followed a real link");
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    let door = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|x| x.id == "return-history")
        .unwrap()
        .at;
    a.pose = facing_rear(door);
    let t = target(&a).expect("aimed return door");
    assert_eq!(t.anchor, "return-history");
    assert!(t.prompt.contains("Static"), "previous title: {}", t.prompt);
    a.world.interaction = Some(t);
    // The full previous address is shown literally in the bottom panel.
    let panel = hair::text::portal_overlay(&a.world, 960, 640);
    assert!(!panel.is_empty(), "return door context panel");
    let other = {
        let mut w = a.world.pocket.take();
        if let Some(p) = &mut w {
            p.previous = p.previous.take().map(|mut p| {
                p.address = p.address.to_ascii_uppercase();
                p
            });
        }
        a.world.pocket = w;
        let o = hair::text::portal_overlay(&a.world, 960, 640);
        a.world
            .pocket
            .as_mut()
            .unwrap()
            .previous
            .as_mut()
            .unwrap()
            .address = first.clone();
        o
    };
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&panel),
        bytemuck::cast_slice::<_, u8>(&other),
        "address drawn case-preserving"
    );
    let current = a.world.current.clone();
    a.key(KeyCode::KeyE, true, false).unwrap();
    a.key(KeyCode::KeyE, false, false).unwrap();
    assert_eq!(
        a.world.current, first,
        "typed history return, not URL parent"
    );
    assert!(a.world.reading && a.scroll == 2, "exact snapshot restored");
    assert_eq!(a.web.as_ref().unwrap().history.len(), 0);
    assert_ne!(a.world.current, current);
}
