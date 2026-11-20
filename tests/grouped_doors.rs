use hair::web::{Address, Page};
use sha2::{Digest, Sha256};
fn parse(s: &str) -> Page {
    Page::html(
        Address::parse("https://example.org/start").unwrap(),
        s.as_bytes(),
    )
    .unwrap()
}
#[test]
fn ordinary_collision_page_groups_every_destination_instead_of_refusing() {
    let mut slots = std::collections::BTreeMap::new();
    for i in 0..100 {
        let raw = format!("https://example.org/destination-{i}");
        let slot = Sha256::digest(raw.as_bytes())[0] % 8;
        if let Some(other) = slots.insert(slot, raw.clone()) {
            let p = parse(&format!(
                "<p>Visible</p><a href='{other}'>First place</a><a href='{raw}'>Second place</a>"
            ));
            let g = p
                .generate()
                .expect("ordinary hash collisions must group, not reject the page");
            assert_eq!(g.doors.len(), 2);
            assert!(g.doors.iter().all(|d| d.destinations.len() == 1));
            let targets = g
                .doors
                .iter()
                .flat_map(|d| d.destinations.iter())
                .collect::<Vec<_>>();
            assert!(targets.contains(&&other));
            assert!(targets.contains(&&raw));
            return;
        }
    }
    panic!("fixture needs collision");
}
#[test]
fn aimed_group_cycles_fragments_forward_reverse_and_activation_matches_prompt() {
    let mut g=parse("<p>Visible</p><a href='/b?q=AB%2Fcd#one'>First place</a><a href='/b?q=AB%2Fcd#two'>Second place</a>").generate().unwrap();
    let pose = hair::Pose {
        position: [-3.6, g.host.room[1] + 0.5],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::cycle(&g.host, &pose, &mut g.world, false);
    let first = hair::interaction::resolve(&g.host, &pose, &g.world).unwrap();
    hair::interaction::cycle(&g.host, &pose, &mut g.world, false);
    let second = hair::interaction::resolve(&g.host, &pose, &g.world).unwrap();
    assert_ne!(
        first.destination, second.destination,
        "Tab must select another destination within the aimed door"
    );
    assert!(second.prompt.contains(second.destination.as_ref().unwrap()));
    let expected_label = if second.destination.as_deref().unwrap().ends_with("#one") {
        "First place"
    } else {
        "Second place"
    };
    assert!(second.prompt.contains(expected_label));
    assert_ne!(first.anchor, second.anchor);
    assert_eq!(
        [
            first.destination.clone().unwrap(),
            second.destination.clone().unwrap()
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>(),
        [
            "https://example.org/b?q=AB%2Fcd#one".to_owned(),
            "https://example.org/b?q=AB%2Fcd#two".to_owned()
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        hair::web::door_destination(&g.world, &second.anchor),
        second.destination
    );
    hair::interaction::cycle(&g.host, &pose, &mut g.world, true);
    assert_eq!(
        hair::interaction::resolve(&g.host, &pose, &g.world)
            .unwrap()
            .destination,
        first.destination
    );
}
#[test]
fn all_sixty_four_supported_targets_are_reachable_and_mutations_keep_geometry() {
    let raws = (0..64)
        .map(|i| format!("https://example.org/place-{i}?q=AB%2Fcd#part"))
        .collect::<Vec<_>>();
    let make = |raws: &[String]| {
        parse(&format!(
            "<p>Visible</p>{}",
            raws.iter()
                .map(|r| format!("<a href='{r}'>Label</a>"))
                .collect::<String>()
        ))
        .generate()
        .unwrap()
    };
    let mut full = make(&raws);
    assert_eq!(
        full.doors
            .iter()
            .map(|d| d.destinations.len())
            .sum::<usize>(),
        64
    );
    assert_eq!(full.doors.len(), 64);
    let mut seen = std::collections::BTreeSet::new();
    for d in &full.doors {
        let a = full.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        let n = full.host.portal_inward(&d.anchor);
        let p = hair::Pose {
            position: [a.at[0] + n[0], a.at[2] + n[1]],
            yaw: (-n[0]).atan2(n[1]),
            pitch: -0.2,
        };
        for _ in 0..d.destinations.len() {
            let t = hair::interaction::resolve(&full.host, &p, &full.world).unwrap();
            let raw = t.destination.unwrap();
            assert!(seen.insert(raw.clone()));
            assert!(t.prompt.contains(&raw));
            hair::interaction::cycle(&full.host, &p, &mut full.world, false);
        }
        full.world.explicit_target = None;
    }
    assert_eq!(seen, raws.iter().cloned().collect());
    let mut reverse = raws.clone();
    reverse.reverse();
    let reordered = make(&reverse);
    assert_eq!(full.host, reordered.host);
    assert_eq!(full.doors, reordered.doors);
    for index in [0, 31, 63] {
        let mut subset = raws.clone();
        subset.remove(index);
        let small = make(&subset);
        small.host.check_routes(&[]).unwrap();
        for d in &small.doors {
            assert!(full.doors.contains(d));
        }
    }
    let html = format!(
        "<p>Budget</p>{}",
        (0..65)
            .map(|i| format!("<a href='/x{i}'>Link</a>"))
            .collect::<String>()
    );
    // Independently approved migration: 65 ordinary links span bounded rooms;
    // the generator still refuses an individual Page mutated beyond 64 links.
    use hair::{
        content::Destination,
        web::{Completeness, WindowAction, WindowLimit},
    };
    use std::sync::Arc;
    let address = Address::parse("https://example.org/start").unwrap();
    let first = Page::html(address.clone(), html.as_bytes()).unwrap();
    assert_eq!(first.links.len(), 64);
    assert!(matches!(
        first.completeness,
        Completeness::FirstWindow {
            limit: WindowLimit::Links,
            ..
        }
    ));
    assert!(first.previous_cursor().unwrap().is_none());
    let next = first.next_cursor().unwrap().unwrap();
    assert_eq!(next.index(), 1);
    let last = first.next_window(None).unwrap().unwrap();
    assert_eq!(last.links.len(), 1);
    let expected = (0..65)
        .map(|i| format!("https://example.org/x{i}"))
        .collect::<Vec<_>>();
    let urls = |page: &Page| {
        page.links
            .iter()
            .map(|link| match &link.destination {
                Destination::External(url) => url.clone(),
                other => panic!("expected every record to be External: {other:?}"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(urls(&first), expected[..64]);
    assert_eq!(urls(&last), expected[64..]);
    let joined = [urls(&first), urls(&last)].concat();
    assert_eq!(joined, expected);
    assert_eq!(
        joined
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        65
    );
    let held = first.source.as_ref().unwrap();
    assert_eq!(held.encoded(), html.as_bytes());
    assert_eq!(held.address(), &address);
    assert_eq!(
        held.sha256(),
        <[u8; 32]>::from(Sha256::digest(html.as_bytes()))
    );
    assert_eq!(last.address, first.address);
    assert!(Arc::ptr_eq(held, last.source.as_ref().unwrap()));
    assert_eq!(last.source.as_ref().unwrap().sha256(), held.sha256());
    let first_position = first.window.as_ref().unwrap();
    let last_position = last.window.as_ref().unwrap();
    assert_eq!((first_position.index(), last_position.index()), (0, 1));
    assert_eq!(
        first_position.event_range().end,
        last_position.event_range().start
    );
    for (page, wanted, action, anchor) in [
        (
            &first,
            &expected[..64],
            WindowAction::Next(next.clone()),
            "web-next-window",
        ),
        (
            &last,
            &expected[64..],
            WindowAction::Previous(last.previous_cursor().unwrap().unwrap()),
            "web-previous-window",
        ),
    ] {
        let generated = page.clone().generate().unwrap();
        let mut actual = generated
            .doors
            .iter()
            .flat_map(|door| door.destinations.iter().cloned())
            .collect::<Vec<_>>();
        let mut wanted = wanted.to_vec();
        actual.sort();
        wanted.sort();
        assert_eq!(actual, wanted); // Vec equality preserves multiplicity.
        assert_eq!(generated.doors.len(), wanted.len());
        assert_eq!(generated.world.reader_links(), page.links);
        assert_eq!(generated.window_doors.len(), 1);
        assert_eq!(generated.window_doors[0].anchor, anchor);
        assert_eq!(generated.window_doors[0].action, action);
        assert!(generated.world.document().unwrap().contains(
            if page.window.as_ref().unwrap().index() == 0 {
                "more content"
            } else {
                "EOF"
            }
        ));
    }
    assert_eq!(last.completeness, Completeness::Complete);
    assert!(last.next_cursor().unwrap().is_none());
    assert!(last.next_window(None).unwrap().is_none());
    let back = last.previous_window(None).unwrap().unwrap();
    assert_eq!(back.links, first.links);
    assert_eq!(back.address, first.address);
    assert_eq!(back.window, first.window);
    assert_eq!(back.completeness, first.completeness);
    assert_eq!(back.source.as_ref().unwrap().sha256(), held.sha256());
    assert!(Arc::ptr_eq(back.source.as_ref().unwrap(), held));
    assert!(back.previous_window(None).unwrap().is_none());
    assert_eq!(back.next_cursor().unwrap().unwrap(), next);
}
#[test]
fn maximum_literal_address_wraps_inside_minimum_view_and_missing_images_do_not_block() {
    let prefix = "https://example.org/CaseSensitive?q=AB%2Fcd&long=";
    let raw = format!("{prefix}{}", "a".repeat(2048 - prefix.len()));
    let mut g = parse(&format!(
        "<p>Visible</p><img src='/unacquired.png' alt='Not loaded'><a href='{raw}'>{}</a>",
        "Long label ".repeat(70)
    ))
    .generate()
    .unwrap();
    let d = &g.doors[0];
    let a = g.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
    let pose = hair::Pose {
        position: [a.at[0], -2.8],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::refresh(&g.host, &pose, &mut g.world);
    assert_eq!(
        hair::web::door_destination(&g.world, &d.anchor).as_deref(),
        Some(raw.as_str())
    );
    assert!(g.world.site.as_ref().unwrap().images.is_empty());
    for (w, h) in [(640, 480), (960, 640)] {
        let panel = hair::text::portal_overlay(&g.world, w, h);
        assert!(!panel.is_empty());
        for v in panel {
            assert!(
                (-1.0..=1.0).contains(&v.position[0]) && (-1.0..=1.0).contains(&v.position[1]),
                "panel clipping at {w}x{h}"
            );
        }
    }
    g.world.reading = true;
    assert!(hair::text::portal_overlay(&g.world, 640, 480).is_empty());
}
