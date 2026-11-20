use hair::web::{Address, Page};
fn parse(s: &str) -> hair::Result<Page> {
    Page::html(
        Address::parse("https://example.org/a").unwrap(),
        s.as_bytes(),
    )
}
#[test]
fn mutation_placement_observation() {
    let a = parse("<p>Visible</p><a href='/b'>B</a><a href='/c'>C</a>")
        .unwrap()
        .generate()
        .unwrap();
    let b = parse("<p>Visible</p><a href='/a'>A</a><a href='/b'>B</a><a href='/c'>C</a>")
        .unwrap()
        .generate()
        .unwrap();
    let da = a.doors.iter().find(|d| d.resource.ends_with("/b")).unwrap();
    let aa = a.host.anchors.iter().find(|x| x.id == da.anchor).unwrap();
    let ab = b.host.anchors.iter().find(|x| x.id == da.anchor).unwrap();
    println!(
        "OBSERVED stable_id={} before={:?} after={:?}",
        da.anchor, aa.at, ab.at
    );
    assert_eq!(aa.at, ab.at, "adding sibling reshuffles existing link door");
}

use sha2::{Digest, Sha256};
fn slot(raw: &str) -> usize {
    usize::from(Sha256::digest(raw.as_bytes())[0] % 8)
}
fn examples() -> Vec<String> {
    let mut slots = vec![String::new(); 8];
    for i in 0..1000 {
        let raw = format!("https://example.org/p{i}?q=AB%2Fcd");
        let s = slot(&raw);
        if slots[s].is_empty() {
            slots[s] = raw;
        }
    }
    assert!(slots.iter().all(|s| !s.is_empty()));
    slots
}
fn links(raw: &[String]) -> String {
    let mut html = String::from("<p>Visible</p>");
    for r in raw {
        html.push_str(&format!("<a href='{r}'>Target</a>"));
    }
    html
}
#[test]
fn fixed_slots_group_collision_and_overflow_without_silent_truncation() {
    let raw = examples();
    let full = parse(&links(&raw)).unwrap().generate().unwrap();
    assert_eq!(full.doors.len(), 8);
    let other = (1000..2000)
        .map(|i| format!("https://example.org/p{i}"))
        .find(|r| slot(r) == slot(&raw[0]))
        .unwrap();
    let collision = parse(&links(&[raw[0].clone(), other.clone()]))
        .unwrap()
        .generate()
        .unwrap();
    assert_eq!(collision.doors.len(), 1);
    assert_eq!(collision.doors[0].destinations.len(), 2);
    let mut overflow = raw.clone();
    overflow.push(other);
    let result = parse(&links(&overflow)).unwrap().generate().unwrap();
    assert_eq!(result.doors.len(), 8);
    assert_eq!(
        result
            .doors
            .iter()
            .map(|d| d.destinations.len())
            .sum::<usize>(),
        9
    );
}
#[test]
fn permutations_insertion_deletion_reintroduction_keep_all_landmarks_and_reachability() {
    let raw = examples();
    let full = parse(&links(&raw)).unwrap().generate().unwrap();
    let mut reverse = raw.clone();
    reverse.reverse();
    let reordered = parse(&links(&reverse)).unwrap().generate().unwrap();
    assert_eq!(full.host, reordered.host);
    assert_eq!(full.doors, reordered.doors);
    for removed in 0..8 {
        let mut subset = raw.clone();
        subset.remove(removed);
        let small = parse(&links(&subset)).unwrap().generate().unwrap();
        assert_eq!(small.host.room, full.host.room);
        assert_eq!(small.host.spawn, full.host.spawn);
        assert_eq!(small.host.geometry, full.host.geometry);
        assert_eq!(small.host.walls(), full.host.walls());
        for a in &small.host.anchors {
            assert!(full.host.anchors.contains(a));
        }
        subset.insert(0, raw[removed].clone());
        assert_eq!(
            parse(&links(&subset)).unwrap().generate().unwrap().host,
            full.host
        );
    }
    full.host.check_routes(&[]).unwrap();
    assert!(!full.host.blocked(full.host.spawn));
    assert!(full.host.blocked([-8.1, 0.0]));
    assert!(full.host.blocked([0.0, -3.8]));
    for d in &full.doors {
        let a = full.host.anchors.iter().find(|a| a.id == d.anchor).unwrap();
        assert_eq!(a.at, [-7.2 + slot(&d.resource) as f32 * 1.8, 0.1, -4.0]);
        let pose = hair::Pose {
            position: [a.at[0], -2.8],
            yaw: 0.0,
            pitch: -0.2,
        };
        assert!(!full.host.blocked(pose.position));
        let t = hair::interaction::resolve(&full.host, &pose, &full.world).unwrap();
        assert_eq!(t.anchor, d.anchor);
        assert_eq!(t.destination, Some(d.destinations[0].clone()));
    }
}
#[test]
fn fragments_queries_and_raw_provenance_survive_fixed_placement() {
    let html =
        "<p>Visible</p><a href='/b?q=AB%2Fcd#one'>First</a><a href='/b?q=AB%2Fcd#two'>Second</a>";
    let g = parse(html).unwrap().generate().unwrap();
    assert_eq!(g.world.current, "https://example.org/a");
    assert_eq!(g.doors.len(), 1);
    assert_eq!(g.doors[0].resource, "https://example.org/b?q=AB%2Fcd");
    assert_eq!(
        g.doors[0].destinations,
        [
            "https://example.org/b?q=AB%2Fcd#one",
            "https://example.org/b?q=AB%2Fcd#two"
        ]
    );
    assert_eq!(g.page.links[0].raw, "/b?q=AB%2Fcd#one");
    assert_eq!(g.world.reader_links(), g.page.links);
    assert_eq!(
        hair::web::door_destination(&g.world, &g.doors[0].anchor),
        Some(g.doors[0].destinations[0].clone())
    );
}
