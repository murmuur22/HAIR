//! Slice7 test preparation only. Actual App input/HUD producer; no runtime seam.
use super::*;
use hair::{assets::Vertex, folder_scene};
use std::fs;

const ESSENTIALS: &str = "U Parent | R Back | F5 refresh | I source";

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let owned = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(temp.starts_with(owned), "owned TMPDIR required");
        let path = temp.join(format!("hair-slice7-{label}-{}", std::process::id()));
        fs::create_dir(&path).expect("exclusive fixture; never reuse leftovers");
        fs::create_dir(path.join("Lost Child MiXeD")).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn app(&self) -> App {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut cache = hair::theme::AssetCache::default();
        let themes = ["office", "archive"]
            .iter()
            .map(|name| {
                hair::theme::Theme::load(&root.join(format!("packs/{name}.toml")), &mut cache)
                    .unwrap()
            })
            .collect();
        let mut a = App::new(root.clone(), themes, 0, World::load(&root).unwrap(), false);
        a.complete_folder(Ok(Some(self.0.clone()))).unwrap();
        a
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn tap(a: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!a.keyboard_input(Some(code), down, false, None).unwrap());
    }
}
fn neutral(a: &mut App) {
    a.pose = Pose {
        position: a.themes[a.theme_index].host.spawn,
        yaw: 0.0,
        pitch: 0.0,
    };
    a.world.explicit_target = None;
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
    assert!(
        a.world.interaction.is_none(),
        "I must inspect the DIRECTORY, not a selected entry"
    );
}
fn stale_child_refusal(a: &mut App, f: &Fixture) {
    let id = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "Lost Child MiXeD")
        .unwrap()
        .0
        .clone();
    let anchor = a.themes[a.theme_index]
        .host
        .anchors
        .iter()
        .find(|v| v.id == id)
        .unwrap();
    let host = &a.themes[a.theme_index].host;
    let n = host.portal_inward(&id);
    a.pose = Pose {
        position: [anchor.at[0] + 1.05 * n[0], anchor.at[2] + 1.05 * n[1]],
        yaw: (-n[0]).atan2(n[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(a.pose.position));
    a.world.explicit_target = None;
    hair::interaction::refresh(&a.themes[a.theme_index].host, &a.pose, &mut a.world);
    assert_eq!(a.world.interaction.as_ref().unwrap().anchor, id);
    fs::remove_dir(f.0.join("Lost Child MiXeD")).unwrap(); // test-owned stale listing
    tap(a, KeyCode::KeyE);
    assert!(!a.world.reading && a.local_status.starts_with("Refused:"));
    assert_eq!(a.local.as_ref().unwrap().explorer.current().folder, f.0);
}
fn bounds(vertices: &[Vertex], size: [u32; 2]) -> [f32; 4] {
    assert!(!vertices.is_empty(), "missing geometry");
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for v in vertices {
        let x = (v.position[0] + 1.0) * 0.5 * size[0] as f32;
        let y = (1.0 - v.position[1]) * 0.5 * size[1] as f32;
        assert!(x.is_finite() && y.is_finite());
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    b
}
fn ink(vertices: &[Vertex]) -> Vec<Vertex> {
    vertices
        .iter()
        .filter(|v| v.color == [0.85; 3])
        .copied()
        .collect()
}
// Compare a row's full emitted ink against the public single-line renderer,
// translated to its actual band. This catches missing controls/tail/~ glyphs,
// not merely string presence or an in-bounds rectangle.
fn row_matches(row: &[Vertex], literal: &str, size: [u32; 2]) {
    let columns = (size[0] as usize - 24) / 12;
    let expected = if literal.chars().count() > columns {
        format!("{}~", literal.chars().take(columns - 1).collect::<String>())
    } else {
        literal.to_string()
    };
    let reference = ink(&hair::text::pack_overlay(&expected, size[0], size[1]));
    let actual = ink(row);
    let ab = bounds(&actual, size);
    let rb = bounds(&reference, size);
    assert_eq!(
        actual.len(),
        reference.len(),
        "missing row ink: {literal:?}"
    );
    for (a, r) in actual.iter().zip(&reference) {
        assert!((a.position[0] - r.position[0]).abs() < 0.0001);
        let ay = (1.0 - a.position[1]) * 0.5 * size[1] as f32 - ab[1];
        let ry = (1.0 - r.position[1]) * 0.5 * size[1] as f32 - rb[1];
        assert!((ay - ry).abs() < 0.01, "literal row glyph mismatch");
    }
    assert!(ab[0] >= 0.0 && ab[2] <= size[0] as f32 + 0.01);
    assert!(ab[3] - ab[1] >= 13.99, "retain 2x essential/status glyphs");
}
fn rows(vertices: &[Vertex]) -> Vec<&[Vertex]> {
    let starts: Vec<_> = vertices
        .as_chunks::<6>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, q)| q[0].color == [0.025; 3])
        .map(|(i, _)| i * 6)
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| &vertices[start..starts.get(i + 1).copied().unwrap_or(vertices.len())])
        .collect()
}
fn three_bands(message: &str, size: [u32; 2]) {
    let geometry = hair::text::pack_overlay(message, size[0], size[1]);
    let bands = rows(&geometry);
    assert_eq!(
        bands.len(),
        3,
        "local summary, essentials and recovery each need a visible band"
    );
    let mut last_bottom = 0.0;
    for (band, line) in bands.iter().zip(message.lines().take(3)) {
        row_matches(band, line, size);
        let panel = bounds(&band[..6], size);
        let text = bounds(&ink(band), size);
        assert!(panel[0] >= 0.0 && panel[2] <= size[0] as f32 + 0.01);
        assert!(
            panel[1] > last_bottom && panel[3] < size[1] as f32 - 40.0 + 0.01,
            "reserve a positive gap above the bottom target summary: {panel:?}"
        );
        assert!(text[1] > panel[1] && text[3] < panel[3]);
        last_bottom = panel[3];
    }
}

#[test]
fn three_local_hud_lines_keep_essential_controls_and_recovery_visible() {
    for size in [[640, 480], [960, 640]] {
        let status = format!(
            "Refused: MiXeD {}; last good folder retained; I source",
            "context ".repeat(20)
        );
        let message = format!("Page 1/3 | entries 1-12 of 27 | office\n{ESSENTIALS}\n{status}");
        three_bands(&message, size);
        // At most three bands: no unbounded footer growth from extra input lines.
        let capped =
            hair::text::pack_overlay(&format!("{message}\nNOT A FOURTH BAND"), size[0], size[1]);
        assert_eq!(rows(&capped).len(), 3);
    }
}
#[test]
fn existing_two_line_overlay_keeps_both_rows_and_original_offsets() {
    let lines = [
        "L address | Refused; last good place retained",
        "office | retained pack",
    ];
    for size in [[640, 480], [960, 640]] {
        let geometry = hair::text::pack_overlay(&lines.join("\n"), size[0], size[1]);
        let bands = rows(&geometry);
        assert_eq!(bands.len(), 2);
        for (i, band) in bands.iter().enumerate() {
            row_matches(band, lines[i], size);
            let panel = bounds(&band[..6], size);
            assert!((panel[1] - (size[1] as f32 - 78.0 + i as f32 * 20.0)).abs() < 0.01);
        }
    }
}
fn inspect_full_status(a: &mut App, size: [u32; 2]) {
    neutral(a);
    let status = a.local_status.clone();
    let folder = a.local.as_ref().unwrap().explorer.current().clone();
    tap(a, KeyCode::KeyI);
    assert!(a.world.reading && a.world.reader_link.is_none());
    let document = a.world.document().unwrap().to_string();
    assert!(document.contains(&folder_scene::display(folder.folder.as_os_str())));
    let start = document
        .find(&status)
        .expect("directory I must retain FULL literal local_status, not just HUD preview");
    for retained_control in ["O choose", "PgUp/PgDn", "L inactive"] {
        assert!(
            document.contains(retained_control),
            "directory I must retain secondary help: {retained_control}"
        );
    }
    assert!(!status.is_empty());
    // Locate the status tail's wrapped row via the public production row counter.
    // The cloned document is solely an oracle; actual App.reader is never assigned.
    let end = start + status.trim_end().len();
    let page = a.local.as_ref().unwrap().page;
    let oracle_world = |text: &str| {
        let (mut world, _) = folder_scene::build(&folder, page, false).unwrap();
        world.reading = true;
        world.folder_view.as_mut().unwrap().reader = text.to_string();
        world
    };
    let mut prefix = oracle_world(&document[..end]);
    let tail_row = hair::text::local_reader_scroll_max(&prefix, size[0], size[1]);
    for collapsed in [false, true] {
        if a.world.tree_collapsed != collapsed {
            tap(a, KeyCode::KeyM);
        }
        // Recompute after changing tree width; reset by closing/reopening actual I.
        tap(a, KeyCode::Escape);
        tap(a, KeyCode::KeyI);
        assert_eq!(
            a.local_status, status,
            "inspection cannot erase its own recovery reason"
        );
        assert!(a.world.document().unwrap().contains(&status));
        prefix.tree_collapsed = collapsed;
        let row = hair::text::local_reader_scroll_max(&prefix, size[0], size[1]);
        let max = hair::text::local_reader_scroll_max(&a.world, size[0], size[1]);
        assert!(row <= max && (tail_row > 0 || row > 0));
        for _ in 0..row {
            tap(a, KeyCode::ArrowDown);
        }
        assert_eq!(
            a.scroll, row,
            "actual Down must reach the complete status tail"
        );
        let body = |w: &World| {
            hair::text::overlay(w, size[0], size[1], a.scroll)
                .into_iter()
                .filter(|v| v.color == [0.9, 0.96, 0.91])
                .collect::<Vec<_>>()
        };
        let actual = body(&a.world);
        let mut complete = oracle_world(&document);
        complete.tree_collapsed = collapsed;
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&actual),
            bytemuck::cast_slice::<_, u8>(&body(&complete)),
            "positive reference must match actual reader before tail-erasure negative"
        );
        let mut erased = oracle_world(&document);
        erased.tree_collapsed = collapsed;
        let at = document[..end].char_indices().last().unwrap().0;
        erased
            .folder_view
            .as_mut()
            .unwrap()
            .reader
            .replace_range(at..end, " ");
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&actual),
            bytemuck::cast_slice::<_, u8>(&body(&erased)),
            "full status tail must emit actual visible glyphs, not sit outside the reader"
        );
        let b = bounds(&actual, size);
        assert!(b[0] >= 0.0 && b[2] <= size[0] as f32 + 0.01 && b[3] < size[1] as f32 - 82.0);
        assert_eq!(a.local.as_ref().unwrap().explorer.current(), &folder);
    }
    tap(a, KeyCode::Escape);
}
#[test]
fn directory_i_exposes_cancelled_full_status_through_actual_reader_and_down() {
    let f = Fixture::new("directory-status");
    let mut a = f.app();
    a.complete_folder(Ok(None)).unwrap();
    assert!(a.local_status.starts_with("Folder selection cancelled"));
    // No renderer: App's existing keyboard scroll fallback is 960x640.
    inspect_full_status(&mut a, [960, 640]);
}
fn actual_hud(a: &mut App, size: [u32; 2]) {
    a.update_hud();
    let message = &a
        .renderer
        .as_ref()
        .expect("real Renderer required by update_hud")
        .pack_feedback;
    let lines: Vec<_> = message.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "actual App HUD must reserve recovery independently: {message:?}"
    );
    assert_eq!(
        lines[1], ESSENTIALS,
        "all essentials must fit the 640px row without truncation"
    );
    assert_eq!(
        lines[2], a.local_status,
        "never discard or append recovery behind verbose controls"
    );
    assert!(lines[0].contains(&a.world.folder_view.as_ref().unwrap().summary()));
    assert!(lines[0].contains(&a.themes[a.theme_index].name));
    assert!(ESSENTIALS.chars().count() <= (size[0] as usize - 24) / 12);
    three_bands(message, size);
    let overlay = hair::text::overlay(&a.world, size[0], size[1], 0);
    let target: Vec<_> = overlay
        .iter()
        .filter(|v| v.color == [0.1, 0.95, 0.35])
        .copied()
        .collect();
    let footer = hair::text::pack_overlay(message, size[0], size[1]);
    assert!(
        bounds(&footer, size)[3] + 2.0 < bounds(&target, size)[1],
        "HUD must clear target summary"
    );
}
#[test]
#[ignore = "serialized real Metal Renderer for actual update_hud storage and 640px reader scroll; no PNG claim"]
fn actual_app_hud_and_directory_i_keep_recovery_at_both_sizes() {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    for (label, size) in [("hud-small", [640, 480]), ("hud-normal", [960, 640])] {
        let f = Fixture::new(label);
        let mut a = f.app();
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap();
        renderer.set_materials(&a.themes[0].textures).unwrap();
        a.renderer = Some(renderer);
        // Short status, inactive-action explanation, cancellation, real stale E refusal.
        for case in 0..4 {
            match case {
                0 => tap(&mut a, KeyCode::KeyR),
                1 => tap(&mut a, KeyCode::KeyL),
                2 => a.complete_folder(Ok(None)).unwrap(),
                _ => stale_child_refusal(&mut a, &f),
            }
            assert!(!a.local_status.is_empty());
            neutral(&mut a);
            actual_hud(&mut a, size);
            inspect_full_status(&mut a, size);
            actual_hud(&mut a, size);
            println!(
                "HUD_STATUS_CONTRACT size={size:?} case={case} status={:?}; App input + real Renderer storage + CPU text geometry, NOT pixel/native approval",
                a.local_status
            );
        }
        pollster::block_on(a.renderer.as_ref().unwrap().error_probe()).unwrap();
        a.renderer.as_ref().unwrap().wait().unwrap();
    }
}
