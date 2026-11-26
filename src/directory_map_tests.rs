use super::*;
fn snapshot(name: &str) -> Snapshot {
    Snapshot {
        folder: std::path::PathBuf::from(name),
        entries: vec![],
    }
}
fn add(m: &mut Map, n: u64, p: Option<u64>) {
    m.observe(
        &snapshot(&format!("folder-{n}")),
        [1, n],
        None,
        0,
        [-10.5, -6.0, 10.5, 6.0],
        p.map(|p| Relation::ChildOf([1, p]))
            .unwrap_or(Relation::Selection),
    );
}
#[test]
fn directory_tree_layout_has_disjoint_subtrees_instead_of_graph_crossings() {
    let mut m = Map::default();
    add(&mut m, 0, None);
    for (n, p) in [
        (1, 0),
        (2, 0),
        (3, 1),
        (4, 1),
        (5, 3),
        (6, 2),
        (7, 2),
        (8, 7),
    ] {
        add(&mut m, n, Some(p));
    }
    let xy = m.layout();
    for (i, n) in m.nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            assert_eq!(
                xy[i][1],
                xy[p][1] + 1.0,
                "hierarchy depth must determine row"
            );
        }
        for (j, q) in xy.iter().enumerate().take(i) {
            assert_ne!(xy[i], *q, "distinct rooms {i}/{j} overlap");
        }
    }
    fn range(m: &Map, xy: &[[f32; 2]], p: usize) -> [f32; 2] {
        let mut r = [xy[p][0]; 2];
        for (i, n) in m.nodes.iter().enumerate() {
            if n.parent == Some(p) {
                let v = range(m, xy, i);
                r[0] = r[0].min(v[0]);
                r[1] = r[1].max(v[1]);
            }
        }
        r
    }
    for p in 0..m.nodes.len() {
        let children: Vec<_> = m
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent == Some(p))
            .map(|(i, _)| i)
            .collect();
        for pair in children.windows(2) {
            assert!(range(&m, &xy, pair[0])[1] < range(&m, &xy, pair[1])[0]);
        }
    }
    m.observe(
        &snapshot("folder-1"),
        [1, 1],
        Some([1, 0]),
        0,
        [-10.5, -6.0, 10.5, 6.0],
        Relation::Visit,
    );
    assert_eq!(m.layout(), xy);
}
#[test]
fn directory_room_dimensions_use_immediate_contents_not_page_number() {
    use crate::{
        folder_scene,
        local_explorer::{Entry, Kind},
    };
    let mut s = snapshot("/example");
    let (_, empty) = folder_scene::build(&s, 0, false).unwrap();
    assert_eq!(empty.room, [-10.5, -6.0, 10.5, 6.0]);
    for i in 0..12 {
        let name = format!("dir-{i:02}");
        s.entries.push(Entry {
            name: name.clone().into(),
            path: s.folder.join(name),
            kind: Kind::Folder,
        });
    }
    let (_, wide) = folder_scene::build(&s, 0, false).unwrap();
    assert!(
        wide.room[2] - wide.room[0] > empty.room[2] - empty.room[0],
        "more subdirectories should widen the bounded platform"
    );
    for i in 0..25 {
        let name = format!("file-{i:02}.txt");
        s.entries.push(Entry {
            name: name.clone().into(),
            path: s.folder.join(name),
            kind: Kind::File,
        });
    }
    let (_, dense) = folder_scene::build(&s, 0, false).unwrap();
    assert!(dense.room[3] - dense.room[1] > empty.room[3] - empty.room[1]);
    for page in 0..4 {
        let (_, h) = folder_scene::build(&s, page, false).unwrap();
        assert_eq!(h.room, dense.room);
        h.check_routes(&[h.spawn]).unwrap();
        for a in &h.anchors {
            match a.id.as_str() {
                "folder-parent" => assert_eq!(a.at[2], h.room[3] + 0.7),
                "folder-next" => assert_eq!(a.at[0], h.room[2] + 0.7),
                "folder-previous" => assert_eq!(a.at[0], h.room[0] - 0.7),
                _ => (),
            }
        }
    }
    assert!(dense.room[2] - dense.room[0] <= 30.0 && dense.room[3] - dense.room[1] <= 21.0);
}
#[test]
fn directory_map_cycles_are_rejected_and_limit_restart_is_disclosed() {
    let mut m = Map::default();
    add(&mut m, 0, None);
    add(&mut m, 1, Some(0));
    add(&mut m, 0, Some(1));
    assert_eq!(m.nodes[0].parent, None);
    for n in 2..=MAX_NODES as u64 {
        add(&mut m, n, Some(0));
    }
    assert!(m.restarted);
    assert_eq!(m.nodes.len(), 1);
    assert_eq!(m.current, Some(0));
    assert_eq!(m.nodes[0].id, [1, MAX_NODES as u64]);
    m.observe(
        &snapshot("after-limit"),
        [1, MAX_NODES as u64],
        Some([1, 0]),
        0,
        [-10.5, -6.0, 10.5, 6.0],
        Relation::Visit,
    );
    assert!(
        m.has_back,
        "map restart must not claim that held Back history disappeared"
    );
    assert_eq!(m.back, None, "held destination is outside restarted map");
}
