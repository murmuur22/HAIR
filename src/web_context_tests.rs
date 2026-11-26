//! Web navigation context panel: current page, visitor-history trail,
//! same-response rooms and known link context. Synthetic HTML only.
use crate::web::{Address, Page, PreviousPage};

const PANEL: [f32; 3] = [0.012, 0.012, 0.012];
const CURRENT: [f32; 3] = [0.6, 1.0, 0.7];
const HISTORY: [f32; 3] = [0.6, 0.75, 0.95];
const ROOM: [f32; 3] = [0.9, 0.8, 0.5];
const LINK: [f32; 3] = [0.85, 0.85, 0.85];
const VISITED: [f32; 3] = [0.55, 0.55, 0.55];

fn world(html: &str) -> crate::World {
    let mut w = Page::html(
        Address::parse("https://example.org/here").unwrap(),
        html.as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
    .world;
    w.tree_collapsed = false;
    w
}
fn bounds(v: &[crate::assets::Vertex], w: u32, h: u32) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for v in v {
        let x = (v.position[0] + 1.0) * 0.5 * w as f32;
        let y = (1.0 - v.position[1]) * 0.5 * h as f32;
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    b
}
const LINKS: &str = "<title>Here</title><p>Body</p><a href='/a'>Alpha</a><a href='/b'>Beta</a><a href='https://other.example/c'>Gamma</a>";

#[test]
fn web_room_shows_collapsible_graph_panel_without_fabricated_hierarchy() {
    let mut w = world(LINKS);
    assert!(w.navigation.rows.is_empty(), "no invented folder hierarchy");
    let expanded = crate::text::overlay(&w, 960, 640, 0);
    assert!(expanded.iter().any(|v| v.color == PANEL), "panel drawn");
    assert!(
        expanded.iter().any(|v| v.color == CURRENT),
        "current page row"
    );
    assert!(
        expanded.iter().any(|v| v.color == LINK),
        "available link rows"
    );
    w.tree_collapsed = true;
    let collapsed = crate::text::overlay(&w, 960, 640, 0);
    assert!(
        !collapsed.iter().any(|v| v.color == PANEL),
        "OFF leaves no header remnant"
    );
    assert!(!collapsed.iter().any(|v| v.color == LINK));
    assert!(collapsed.len() < expanded.len());
    // Link context is real: different labels draw different ink.
    let mut other = world(&LINKS.replace("Alpha", "Delta"));
    other.tree_collapsed = false;
    w.tree_collapsed = false;
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&crate::text::overlay(&w, 960, 640, 0)),
        bytemuck::cast_slice::<_, u8>(&crate::text::overlay(&other, 960, 640, 0))
    );
    // Bounded at 64 links and every supported size stays on screen.
    let many: String = (0..64)
        .map(|i| format!("<a href='/p{i}'>P{i}</a>"))
        .collect();
    let big = world(&format!("<p>x</p>{many}"));
    for (width, height) in [(960, 640), (640, 480), (1920, 1280)] {
        let ui = crate::text::overlay(&big, width, height, 0);
        assert!(ui.len() < 60000, "bounded raster work");
        assert!(
            ui.iter()
                .all(|v| v.position.iter().all(|p| (-1.0..=1.0).contains(p)))
        );
    }
}

#[test]
fn panel_distinguishes_history_visited_links_and_same_response_rooms() {
    let mut w = world(LINKS);
    let pocket = w.pocket.as_mut().unwrap();
    pocket.history = vec![PreviousPage {
        title: "Earlier".into(),
        address: "https://example.org/earlier".into(),
    }];
    pocket.previous = pocket.history.last().cloned();
    pocket.visited = vec!["https://example.org/a".into(), w.current.clone()];
    let c: Vec<_> = crate::text::overlay(&w, 960, 640, 0)
        .iter()
        .map(|v| v.color)
        .collect();
    assert!(c.contains(&HISTORY), "visitor-history trail row");
    assert!(c.contains(&VISITED), "visited link distinguished");
    assert!(c.contains(&LINK), "unvisited link stays distinct");
    // A held-response continuation is a room of the same page, not history.
    let links = "<a href='/x'>x</a>".repeat(65);
    let mut first = world(&format!("<p>one</p>{links}"));
    first.tree_collapsed = false;
    let c: Vec<_> = crate::text::overlay(&first, 960, 640, 0)
        .iter()
        .map(|v| v.color)
        .collect();
    assert!(c.contains(&ROOM), "same-response next room row");
    assert!(!c.contains(&HISTORY), "no history at the initial page");
}

#[test]
fn web_reader_reserves_panel_space_and_footer_in_both_collapse_states() {
    let mut w = world(&format!(
        "{LINKS}{}",
        "<p>long readable line of text</p>".repeat(60)
    ));
    w.reading = true;
    for collapsed in [false, true] {
        w.tree_collapsed = collapsed;
        for (width, height) in [(960, 640), (640, 480)] {
            let ui = crate::text::overlay(&w, width, height, 0);
            let panel_at = ui.iter().position(|v| v.color == PANEL);
            assert_eq!(panel_at.is_none(), collapsed, "OFF has no panel/header");
            let panel = panel_at.map(|i| bounds(&ui[i..i + 6], width, height));
            let body: Vec<_> = ui
                .iter()
                .filter(|v| v.color == [0.9, 0.96, 0.91])
                .copied()
                .collect();
            assert!(!body.is_empty());
            for quad in body.as_chunks::<6>().0 {
                let b = bounds(quad, width, height);
                assert!(
                    panel.is_none_or(|p| b[0] > p[2] + 0.01 || b[1] > p[3] + 0.01),
                    "reader ink under web panel: collapsed={collapsed}"
                );
                assert!(b[3] < height as f32 - 82.0, "body enters reader footer");
            }
        }
    }
}
