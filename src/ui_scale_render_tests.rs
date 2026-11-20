//! Rendered screen-space geometry under Settings UI scale, measured in
//! physical pixels from the exact vertices the renderer uploads (normalized
//! positions laid out in the logical viewport, mapped to the physical target).
use crate::{
    assets::Vertex,
    presentation::{Menu, STEPS, applied, ui_viewport},
};

const SIZES: [[u32; 2]; 3] = [[640, 480], [1280, 960], [1920, 1280]];
const HEADER: [f32; 3] = [0.5, 1.0, 0.65];
const BODY: [f32; 3] = [0.9, 0.96, 0.91];
const PANEL_TEXT: [f32; 3] = [0.65, 1.0, 0.75];
const STATUS: [f32; 3] = [0.85, 0.85, 0.85];
const TREE_HEADER: [f32; 3] = [0.8, 0.8, 0.8];
const TREE_PANEL: [f32; 3] = [0.012, 0.012, 0.012];
const PORTAL_PANEL: [f32; 3] = [0.015, 0.04, 0.025];

fn web() -> crate::web::Generated {
    let title = "aB".repeat(256);
    let target = format!("https://example.org/{}", "cD".repeat(1000));
    crate::web::Page::html(
        crate::web::Address::parse("https://example.org/scale").unwrap(),
        format!(
            "<title>Scale</title><p>{}</p><a href='{target}'>{title}</a><a href='/b'>Beta</a>",
            "readable presentation text ".repeat(40)
        )
        .as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
}
/// Web room aimed at the door holding the maximum-length destination.
fn aimed(g: &crate::web::Generated) -> crate::World {
    let mut world = web().world;
    let anchor = g
        .doors
        .iter()
        .find(|d| d.destinations.iter().any(|t| t.len() > 1000))
        .unwrap()
        .anchor
        .clone();
    world.interaction = Some(crate::interaction::Target {
        anchor,
        kind: crate::interaction::Kind::Enter,
        page: world.current.clone(),
        destination: None,
        prompt: "E - Follow".into(),
    });
    world
}
/// Smallest quad edge (one bitmap glyph pixel) of `color`, in physical pixels.
fn glyph_px(vertices: &[Vertex], color: [f32; 3], physical: [u32; 2]) -> f32 {
    let quads: Vec<_> = vertices
        .as_chunks::<6>()
        .0
        .iter()
        .filter(|q| q[0].color == color)
        .collect();
    assert!(!quads.is_empty(), "no {color:?} ink");
    quads
        .iter()
        .map(|q| {
            let ys = q
                .iter()
                .map(|v| (1.0 - v.position[1]) * 0.5 * physical[1] as f32);
            ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min)
        })
        .fold(f32::MAX, f32::min)
}
fn rect_px(q: &[Vertex], physical: [u32; 2]) -> [f32; 4] {
    let xs = q
        .iter()
        .map(|v| (v.position[0] + 1.0) * 0.5 * physical[0] as f32);
    let ys = q
        .iter()
        .map(|v| (1.0 - v.position[1]) * 0.5 * physical[1] as f32);
    [
        xs.clone().fold(f32::MAX, f32::min),
        ys.clone().fold(f32::MAX, f32::min),
        xs.fold(f32::MIN, f32::max),
        ys.fold(f32::MIN, f32::max),
    ]
}
fn on_screen(vertices: &[Vertex]) -> bool {
    vertices.iter().all(|v| {
        v.position
            .iter()
            .all(|p| p.is_finite() && (-1.0..=1.0).contains(p))
    })
}

#[test]
fn every_screen_space_element_grows_with_applied_scale_in_physical_pixels() {
    let g = web();
    let mut reader = g.world;
    reader.reading = true;
    reader.tree_collapsed = false;
    let mut room = aimed(&web());
    room.tree_collapsed = false;
    let mut map = web().world;
    map.tree_collapsed = false;
    let raw = format!("https://example.org/{}", "aB".repeat(1014));
    for physical in SIZES {
        let mut previous: Option<(f32, Vec<f32>)> = None;
        let mut base = Vec::new();
        for step in STEPS {
            let [lw, lh] = ui_viewport(physical, step);
            let scale = applied(step, physical);
            let elements: Vec<(&str, Vec<Vertex>, [f32; 3])> = vec![
                ("hud header", crate::text::overlay(&room, lw, lh, 0), HEADER),
                (
                    "aimed door prompt",
                    crate::text::portal_overlay(&room, lw, lh),
                    PANEL_TEXT,
                ),
                (
                    "https address",
                    crate::text::address_overlay(&raw, Some("Refused; edit or Escape"), lw, lh),
                    PANEL_TEXT,
                ),
                ("reader", crate::text::overlay(&reader, lw, lh, 0), BODY),
                (
                    "status",
                    crate::text::pack_overlay("L address | Refused: 403", lw, lh),
                    STATUS,
                ),
                (
                    "web map",
                    crate::text::overlay(&map, lw, lh, 0),
                    TREE_HEADER,
                ),
                (
                    "menu",
                    crate::presentation::overlay(&Menu::settings(), step, physical),
                    STATUS,
                ),
            ];
            let mut sizes = Vec::new();
            for (name, vertices, color) in &elements {
                assert!(
                    !vertices.is_empty() && on_screen(vertices),
                    "{name} missing or clipped at {physical:?} {step}%"
                );
                sizes.push(glyph_px(vertices, *color, physical));
            }
            if step == 100 {
                base = sizes.clone();
            }
            for (i, (name, ..)) in elements.iter().enumerate() {
                let expected = base[i] * scale;
                assert!(
                    (sizes[i] - expected).abs() <= expected * 0.03,
                    "{name} at {physical:?} {step}%: {} px, expected {expected} (applied {scale})",
                    sizes[i]
                );
            }
            if let Some((was, old)) = &previous
                && scale > *was + 1e-4
            {
                for (i, (name, ..)) in elements.iter().enumerate() {
                    assert!(sizes[i] > old[i], "{name} did not visibly grow at {step}%");
                }
            }
            previous = Some((scale, sizes));
            // The web map panel and the aimed door panel never overlap; with
            // no room the map yields to the transient door panel.
            let ui = crate::text::overlay(&room, lw, lh, 0);
            let portal = crate::text::portal_overlay(&room, lw, lh);
            assert_eq!(portal[0].color, PORTAL_PANEL);
            let portal = rect_px(&portal[..6], physical);
            if let Some(tree) = ui.iter().position(|v| v.color == TREE_PANEL) {
                let tree = rect_px(&ui[tree..tree + 6], physical);
                assert!(
                    tree[3] <= portal[1] || tree[2] <= portal[0],
                    "web map overlaps aimed door panel at {physical:?} {step}%: {tree:?} {portal:?}"
                );
            }
            let small = crate::text::overlay(&map, lw, lh, 0);
            assert!(
                small.iter().any(|v| v.color == TREE_PANEL),
                "map without a door panel"
            );
        }
    }
}

/// FIX1 A: the transient address entry and the aimed HTTPS door panel use the
/// reader/menu 2x bitmap baseline (one glyph pixel = 2 logical pixels, 14
/// logical ink rows), still multiplied by the applied factor, and the panels
/// stay inside the viewport without covering the web map.
#[test]
fn address_and_aimed_door_text_use_reader_2x_baseline_within_viewport() {
    const ALL: [[u32; 2]; 4] = [[640, 480], [960, 640], [1280, 960], [1920, 1280]];
    let mut room = aimed(&web());
    room.tree_collapsed = false;
    let raw = format!("https://example.org/{}", "aB".repeat(1014));
    let error =
        "Input rejected (2048-byte limit/no controls); Backspace to edit or Escape to restart";
    for physical in ALL {
        for step in STEPS {
            let [lw, lh] = ui_viewport(physical, step);
            let per_logical = physical[1] as f32 / lh as f32;
            let reader_px = 2.0 * per_logical;
            for (name, vertices) in [
                (
                    "https address",
                    crate::text::address_overlay(&raw, Some(error), lw, lh),
                ),
                ("aimed door", crate::text::portal_overlay(&room, lw, lh)),
            ] {
                assert!(
                    !vertices.is_empty() && on_screen(&vertices),
                    "{name} missing or clipped at {physical:?} {step}%"
                );
                let px = glyph_px(&vertices, PANEL_TEXT, physical);
                assert!(
                    px + 1e-3 >= reader_px,
                    "{name} glyph pixel {px} < reader 2x baseline {reader_px} at {physical:?} {step}%"
                );
                // Ink stays inside its own opaque panel (first quad).
                let panel = rect_px(&vertices[..6], physical);
                for q in vertices
                    .as_chunks::<6>()
                    .0
                    .iter()
                    .filter(|q| q[0].color == PANEL_TEXT)
                {
                    let r = rect_px(q, physical);
                    assert!(
                        r[0] >= panel[0] - 0.01
                            && r[1] >= panel[1] - 0.01
                            && r[2] <= panel[2] + 0.01
                            && r[3] <= panel[3] + 0.01,
                        "{name} ink outside its panel at {physical:?} {step}%"
                    );
                }
            }
            // The 2x door panel never covers the web map or the header strip.
            let ui = crate::text::overlay(&room, lw, lh, 0);
            let portal = crate::text::portal_overlay(&room, lw, lh);
            let portal = rect_px(&portal[..6], physical);
            assert!(
                portal[1] >= 52.0 * per_logical - 0.01,
                "door panel covers header"
            );
            if let Some(tree) = ui.iter().position(|v| v.color == TREE_PANEL) {
                let tree = rect_px(&ui[tree..tree + 6], physical);
                assert!(
                    tree[3] <= portal[1] || tree[2] <= portal[0],
                    "web map overlaps 2x door panel at {physical:?} {step}%"
                );
            }
        }
    }
}

/// The bounded 2x panels never hide input silently: every literal address
/// character and error row is reachable by address scrolling, and a too-tall
/// door destination keeps its head, exact tail and an omitted-count disclosure.
#[test]
fn bounded_2x_panels_keep_full_address_reachable_and_disclose_omissions() {
    const ALL: [[u32; 2]; 4] = [[640, 480], [960, 640], [1280, 960], [1920, 1280]];
    let raw = format!("https://example.org/{}Zz", "aB".repeat(1013));
    assert_eq!(raw.len(), 2048);
    let error = "Refused: invalid host; last good place retained";
    let g = web();
    let room = aimed(&g);
    let destination = g
        .doors
        .iter()
        .flat_map(|d| d.destinations.iter())
        .find(|t| t.len() > 1000)
        .unwrap()
        .clone();
    for physical in ALL {
        for step in STEPS {
            let [lw, lh] = ui_viewport(physical, step);
            let max = crate::text::address_scroll_max(&raw, Some(error), lw, lh);
            let mut seen = std::collections::BTreeMap::new();
            for back in 0..=max + 2 {
                let rows = crate::text::address_rows(&raw, Some(error), back, lw, lh);
                assert!(
                    70.0 + rows.len() as f32 * 20.0 + 12.0 <= lh as f32,
                    "address panel taller than viewport at {physical:?} {step}%"
                );
                let body = &rows[3..];
                if max == 0 {
                    seen.extend(body.iter().cloned().enumerate());
                    continue;
                }
                let first: usize = body[0]
                    .strip_prefix("~ ROWS ")
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.parse().ok())
                    .expect("overflow is disclosed with a row position");
                if back == 0 {
                    assert!(
                        body.iter().any(|r| r == "_"),
                        "edit cursor visible by default"
                    );
                }
                seen.extend(
                    body[1..]
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, r)| (first - 1 + i, r)),
                );
            }
            let all: Vec<String> = seen.into_values().collect();
            let joined = all.concat();
            assert!(
                joined.starts_with(&format!("{raw}_")) && joined.ends_with(error),
                "address/error not fully reachable at {physical:?} {step}%"
            );
            let door = crate::text::portal_rows(&room, lw, lh);
            assert!(!door.is_empty());
            let tail = &destination[destination.len() - 8..];
            assert!(
                door.iter().any(|r| r.ends_with(tail)),
                "door panel lost the distinguishing destination tail at {physical:?} {step}%"
            );
            let shown: String = door.concat();
            assert!(
                shown.contains(&destination) || shown.contains("CHARS NOT SHOWN; FULL IN READER"),
                "door destination shortened without disclosure at {physical:?} {step}%"
            );
        }
    }
}

#[test]
fn window_capped_scale_is_reported_and_small_window_keeps_baseline() {
    use crate::presentation::scale_label;
    assert_eq!(applied(200, [640, 480]), 1.0);
    assert!(scale_label(200, [640, 480]).contains("WINDOW LIMIT: 100% APPLIED"));
    assert!((applied(200, [1280, 960]) - 1.6).abs() < 1e-6);
    assert!(scale_label(200, [1280, 960]).contains("WINDOW LIMIT: 160% APPLIED"));
    assert_eq!(applied(200, [1920, 1280]), 2.0);
    assert!(!scale_label(200, [1920, 1280]).contains("WINDOW LIMIT"));
}
