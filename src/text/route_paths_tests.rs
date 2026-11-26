use super::*;
#[test]
fn dense_room_grid_routes_every_pair_without_crossing_room_centres_or_caption_ports() {
    let rooms: HashSet<_> = (0..8)
        .flat_map(|x| (0..8).map(move |y| [2 * x + 1, 2 * y + 1]))
        .collect();
    let ordered: Vec<_> = (0..8)
        .flat_map(|x| (0..8).map(move |y| [2 * x + 1, 2 * y + 1]))
        .collect();
    for &a in &ordered {
        for &b in &ordered {
            if a == b {
                continue;
            }
            let path = route(a, b, [16, 16], &rooms, &HashMap::new(), true)
                .expect("reserved gutters must keep every pair reachable");
            assert_eq!(path.first(), Some(&a));
            assert_eq!(path.last(), Some(&b));
            assert!(
                path[1] != [a[0], a[1] + 1],
                "do not exit through own caption"
            );
            assert!(
                path[path.len() - 2] != [b[0], b[1] + 1],
                "do not enter through destination caption"
            );
            assert!(
                path.iter()
                    .all(|p| p[0] >= 0 && p[1] >= 0 && p[0] <= 16 && p[1] <= 16)
            );
            assert!(
                path[1..path.len() - 1].iter().all(|p| !rooms.contains(p)),
                "never cross a third room"
            );
            assert!(
                path.windows(2)
                    .all(|s| s[0][0].abs_diff(s[1][0]) + s[0][1].abs_diff(s[1][1]) == 1)
            );
        }
    }
}
#[test]
fn routing_is_deterministic_and_uses_available_alternative_gutters() {
    let a = [1, 1];
    let b = [7, 5];
    let rooms = HashSet::from([a, b, [3, 1], [3, 3], [5, 3]]);
    let initial = route(a, b, [8, 6], &rooms, &HashMap::new(), true).unwrap();
    assert_eq!(
        initial,
        route(a, b, [8, 6], &rooms, &HashMap::new(), true).unwrap()
    );
    let used = initial
        .windows(2)
        .map(|s| (segment(s[0], s[1]), 20))
        .collect();
    let alternative = route(a, b, [8, 6], &rooms, &used, true).unwrap();
    assert_ne!(
        initial, alternative,
        "prefer uncongested gutters when available"
    );
}
