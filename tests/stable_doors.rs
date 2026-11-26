//! Individual-door successor oracles. Original grouped/fixed-position REDs are
//! retained in physical-gates-r2; reflow on explicit changed-source refresh is approved.
use hair::web::{Address, Page};
use sha2::{Digest, Sha256};
fn parse(s: &str) -> hair::Result<Page> {
    Page::html(
        Address::parse("https://example.org/a").unwrap(),
        s.as_bytes(),
    )
}
fn links(raw: &[String]) -> String {
    format!(
        "<p>Visible</p>{}",
        raw.iter()
            .map(|r| format!("<a href='{r}'>Target</a>"))
            .collect::<String>()
    )
}
fn examples() -> Vec<String> {
    (0..8)
        .map(|i| format!("https://example.org/p{i}?q=AB%2Fcd"))
        .collect()
}
#[test]
fn insertion_preserves_full_destination_identity_not_physical_sort_slot() {
    let a = parse("<p>Visible</p><a href='/b'>B</a><a href='/c'>C</a>")
        .unwrap()
        .generate()
        .unwrap();
    let b = parse("<p>Visible</p><a href='/a'>A</a><a href='/b'>B</a><a href='/c'>C</a>")
        .unwrap()
        .generate()
        .unwrap();
    for d in &a.doors {
        assert_eq!(
            b.doors.iter().find(|e| e.resource == d.resource).unwrap(),
            d
        );
    }
}
#[test]
fn old_hash_collisions_have_individual_doors_and_nine_links_never_truncate() {
    let raw = examples();
    let full = parse(&links(&raw)).unwrap().generate().unwrap();
    assert_eq!(full.doors.len(), 8);
    let other = (1000..2000)
        .map(|i| format!("https://example.org/p{i}"))
        .find(|r| Sha256::digest(r.as_bytes())[0] % 8 == Sha256::digest(raw[0].as_bytes())[0] % 8)
        .unwrap();
    let collision = parse(&links(&[raw[0].clone(), other.clone()]))
        .unwrap()
        .generate()
        .unwrap();
    assert_eq!(collision.doors.len(), 2);
    assert!(collision.doors.iter().all(|d| d.destinations.len() == 1));
    let mut more = raw;
    more.push(other);
    let g = parse(&links(&more)).unwrap().generate().unwrap();
    assert_eq!(g.doors.len(), 9);
    assert_eq!(
        g.doors.iter().map(|d| d.destinations.len()).sum::<usize>(),
        9
    );
}
#[test]
fn reordering_and_reinsertion_restore_exact_geometry_and_every_door_is_reachable() {
    let raw = examples();
    let full = parse(&links(&raw)).unwrap().generate().unwrap();
    let mut reversed = raw.clone();
    reversed.reverse();
    let reordered = parse(&links(&reversed)).unwrap().generate().unwrap();
    assert_eq!(full.host, reordered.host);
    assert_eq!(full.doors, reordered.doors);
    for removed in 0..8 {
        let mut subset = raw.clone();
        subset.remove(removed);
        let small = parse(&links(&subset)).unwrap().generate().unwrap();
        small.host.check_routes(&[]).unwrap();
        for d in &small.doors {
            assert!(full.doors.contains(d));
        }
        subset.insert(0, raw[removed].clone());
        assert_eq!(
            parse(&links(&subset)).unwrap().generate().unwrap().host,
            full.host
        );
    }
    full.host.check_routes(&[]).unwrap();
    assert!(!full.host.blocked(full.host.spawn));
    assert!(full.host.blocked([full.host.room[0] - 0.1, 0.0]));
    assert!(full.host.blocked([0.0, full.host.room[1] - 0.1]));
    for d in &full.doors {
        let a = full.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        let n = full.host.portal_inward(&d.anchor);
        let pose = hair::Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [a.at[0] + n[0], a.at[2] + n[1]],
            yaw: (-n[0]).atan2(n[1]),
            pitch: -0.2,
        };
        assert!(!full.host.blocked(pose.position));
        let t = hair::interaction::resolve(&full.host, &pose, &full.world).unwrap();
        assert_eq!(t.anchor, d.anchor);
        assert_eq!(t.destination, Some(d.destinations[0].clone()));
    }
}
#[test]
fn fragments_queries_and_raw_provenance_survive_individual_placement() {
    let g = parse(
        "<p>Visible</p><a href='/b?q=AB%2Fcd#one'>First</a><a href='/b?q=AB%2Fcd#two'>Second</a>",
    )
    .unwrap()
    .generate()
    .unwrap();
    assert_eq!(g.world.current, "https://example.org/a");
    assert_eq!(g.doors.len(), 2);
    assert_eq!(
        g.doors
            .iter()
            .flat_map(|d| d.destinations.clone())
            .collect::<Vec<_>>(),
        [
            "https://example.org/b?q=AB%2Fcd#one",
            "https://example.org/b?q=AB%2Fcd#two"
        ]
    );
    assert_eq!(g.page.links[0].raw, "/b?q=AB%2Fcd#one");
    assert_eq!(g.world.reader_links(), g.page.links);
    for d in &g.doors {
        assert_eq!(
            hair::web::door_destination(&g.world, &d.anchor),
            Some(d.resource.clone())
        );
    }
}
