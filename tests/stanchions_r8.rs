use hair::{
    assets::Vertex,
    host_scene::{HostScene, Wall, WallAperture},
    theme::Theme,
};
use std::path::Path;

fn theme(host: &HostScene, pack: &str) -> Theme {
    Theme::load_for_scene(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("packs/{pack}.toml")),
        &mut Default::default(),
        host,
    )
    .unwrap()
}
fn platform() -> HostScene {
    let mut h = HostScene::shared_default();
    h.version = 2;
    h.open_platform = true;
    for a in &mut h.anchors {
        if a.role == "document" {
            a.binding = "explicit-page".into();
        }
        if a.role == "portal" {
            a.binding = "explicit-door".into();
        }
    }
    HostScene::parse(&toml::to_string(&h).unwrap()).unwrap()
}
// Inspect actual composed triangles, not a proposed layout or mesh count.
fn edge_triangles(t: &Theme, axis: usize, edge: f32) -> Vec<&[Vertex]> {
    t.vertices
        .as_chunks::<3>().0.iter()
        .filter(|tri| {
            tri.iter()
                .all(|v| (v.position[axis] - edge).abs() <= 0.121 && v.position[1] <= 1.06)
                && tri.iter().any(|v| v.position[1] > 0.2)
        })
        .map(|tri| tri.as_slice())
        .collect()
}
fn covered(tris: &[&[Vertex]], tangent: usize, point: f32) -> bool {
    tris.iter().any(|tri| {
        let low = tri
            .iter()
            .map(|v| v.position[tangent])
            .fold(f32::INFINITY, f32::min);
        let high = tri
            .iter()
            .map(|v| v.position[tangent])
            .fold(f32::NEG_INFINITY, f32::max);
        low <= point && high >= point
    })
}
#[test]
fn r8_platform_perimeter_is_visible_except_full_front_travel_gap() {
    let h = platform();
    let before = h.clone();
    let t = theme(&h, "office");
    for (axis, bound, tangent, low, high) in [
        (2, 1, 0, h.room[0], h.room[2]),
        (2, 3, 0, h.room[0], h.room[2]),
        (0, 0, 2, h.room[1], h.room[3]),
        (0, 2, 2, h.room[1], h.room[3]),
    ] {
        let triangles = edge_triangles(&t, axis, h.room[bound]);
        for i in 1..75 {
            let p = low + (high - low) * i as f32 / 75.0;
            if bound == 1 && p.abs() < 0.92 {
                continue;
            }
            assert!(
                covered(&triangles, tangent, p),
                "missing boundary axis={axis} bound={bound} at={p}"
            );
        }
        if bound == 1 {
            for i in -8..=8 {
                assert!(
                    !covered(&triangles, tangent, i as f32 * 0.1),
                    "rope/post across front door"
                );
            }
        }
    }
    assert_eq!(h, before);
    assert_eq!(
        t.host, before,
        "cosmetics cannot add host anchors or solids"
    );
}
#[test]
fn r8_side_and_rear_apertures_omit_posts_and_rope_across_the_entire_opening() {
    for wall in [Wall::Rear, Wall::Left, Wall::Right] {
        let mut h = platform();
        let (axis, tangent, edge) = match wall {
            Wall::Rear => (2, 0, h.room[3]),
            Wall::Left => (0, 2, h.room[0]),
            Wall::Right => (0, 2, h.room[2]),
        };
        let a = h.anchors.iter_mut().find(|a| a.role == "portal").unwrap();
        a.at = [0.0; 3];
        a.at[axis] = edge + if wall == Wall::Left { -0.5 } else { 0.5 };
        a.size = if axis == 2 {
            [1.4, 2.1, 0.2]
        } else {
            [0.2, 2.1, 1.4]
        };
        h.wall_apertures.push(WallAperture {
            anchor: a.id.clone(),
            wall,
            width: 1.6,
            height: 2.3,
        });
        let h = HostScene::parse(&toml::to_string(&h).unwrap()).unwrap();
        let t = theme(&h, "office");
        let tris = edge_triangles(&t, axis, edge);
        assert!(covered(&tris, tangent, -2.0) && covered(&tris, tangent, 2.0));
        for i in -79..=79 {
            assert!(
                !covered(&tris, tangent, i as f32 * 0.01),
                "blocked {wall:?} opening"
            );
        }
    }
}
#[test]
fn r8_closed_rooms_keep_walls_and_dense_generated_front_travel_gaps() {
    let html = (0..64)
        .map(|i| format!("<a href='/next-{i}'>Next</a>"))
        .collect::<String>();
    let h = hair::web::Page::html(
        hair::web::Address::parse("https://test.example/").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
    .host;
    let t = theme(&h, "office");
    assert_eq!(t.host.walls(), h.walls());
    let front = edge_triangles(&t, 2, h.room[1]);
    assert!(covered(&front, 0, -7.0), "unprotected front edge");
    for a in h
        .anchors
        .iter()
        .filter(|a| a.role == "portal" && h.portal_inward(&a.id) == [0.0, 1.0])
    {
        for offset in [-0.69, 0.0, 0.69] {
            assert!(
                !covered(&front, 0, a.at[0] + offset),
                "front travel opening {}",
                a.id
            );
        }
    }
    for (axis, bound) in [(0, 0), (0, 2), (2, 3)] {
        // Front end-posts touch the two wall corners; only the interior of
        // each already-walled edge must be free of redundant rope runs.
        let tangent = if axis == 2 { 0 } else { 2 };
        let interior = edge_triangles(&t, axis, h.room[bound])
            .into_iter()
            .filter(|tri| {
                tri.iter().all(|v| {
                    v.position[tangent] > h.room[if tangent == 0 { 0 } else { 1 }] + 0.25
                        && v.position[tangent] < h.room[if tangent == 0 { 2 } else { 3 }] - 0.25
                })
            })
            .collect::<Vec<_>>();
        assert!(interior.is_empty(), "do not overlay ropes on solid walls");
    }
}
#[test]
fn r8_theme_palette_changes_not_scale_host_or_file_geometry() {
    let h = platform();
    let a = theme(&h, "office");
    let b = theme(&h, "archive");
    let aa = edge_triangles(&a, 2, h.room[3]);
    let bb = edge_triangles(&b, 2, h.room[3]);
    assert!(!aa.is_empty());
    let positions = |t: &[&[Vertex]]| {
        t.iter()
            .flat_map(|v| v.iter().map(|v| v.position))
            .collect::<Vec<_>>()
    };
    assert_eq!(positions(&aa), positions(&bb));
    let colors = |t: &[&[Vertex]]| {
        t.iter()
            .flat_map(|v| v.iter().map(|v| v.color))
            .collect::<Vec<_>>()
    };
    assert_ne!(
        colors(&aa),
        colors(&bb),
        "stanchions follow loaded role palette, not a theme-name table"
    );
    for t in [&a, &b] {
        assert_eq!(t.host, h);
        assert!(t.vertices.iter().all(|v| {
            v.position
                .iter()
                .chain(v.color.iter())
                .chain(v.uv.iter())
                .all(|x| x.is_finite())
        }));
        assert!(t.vertices.len() <= 300_000);
    }
}
