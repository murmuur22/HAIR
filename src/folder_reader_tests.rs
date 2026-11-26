use super::*;
use std::{fs, os::unix::fs::MetadataExt};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn body_geometry(world: &World, w: u32, h: u32, scroll: usize) -> Vec<([f32; 3], [f32; 3])> {
    hair::text::overlay(world, w, h, scroll)
        .into_iter()
        .filter(|v| v.color == [0.9, 0.96, 0.91])
        .map(|v| (v.position, v.color))
        .collect()
}
fn tap(app: &mut App, code: KeyCode) {
    assert!(!app.key(code, true, false).unwrap());
    assert!(!app.key(code, false, false).unwrap());
}
fn assert_chunk(app: &App, bytes: &[u8], start: usize, file: &Path) {
    let end = (start + 4096).min(bytes.len());
    let doc = app.world.document().unwrap();
    assert!(doc.starts_with("TEXT: UTF-8 interpretation of this byte range\n"));
    assert!(doc.contains(&format!("RANGE [0x{start:016X}, 0x{end:016X})")));
    assert!(doc.contains(&format!(
        "Observed length: {} bytes | {}",
        bytes.len(),
        if end == bytes.len() {
            "EOF (next inert)"
        } else {
            "MORE"
        }
    )));
    let body = doc
        .split_once("\nTEXT (links inactive; escapes shown; range only):\n")
        .unwrap()
        .1
        .split_once("\n\nFULL SOURCE / METADATA:\n")
        .unwrap()
        .0;
    assert_eq!(body.as_bytes(), &bytes[start..end]);
    assert!(doc.contains(&hair::folder_scene::display(file.as_os_str())));
    assert!(doc.contains(&hair::folder_scene::display(file.file_name().unwrap())));
}
#[test]
fn le01_real_file_tail_is_keyboard_reachable_for_supported_viewports_and_tree_states() {
    check_tail(false);
    check_tail(true);
}
fn check_tail(repeated: bool) {
    let temp = std::env::temp_dir().canonicalize().unwrap();
    let owned = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    assert!(
        temp.starts_with(owned),
        "TMPDIR must be inside the owned milestone workspace"
    );
    let f = Fixture(temp.join(format!("hair-le01-{}", std::process::id())));
    fs::create_dir(&f.0).unwrap();
    let mut dir = f.0.clone();
    for _ in 0..3 {
        dir = dir.join("\u{1}".repeat(150));
        fs::create_dir(&dir).unwrap();
    }
    let file = dir.join(format!("{}.txt", "\u{1}".repeat(240)));
    use std::os::unix::ffi::OsStrExt;
    assert!(file.as_os_str().as_bytes().len() < (libc::PATH_MAX as usize));
    // Preserve the inherited full fixture, including all empty lines and its
    // long escaped name/source. It deliberately spans FOUR reader chunks.
    let bytes = [vec![b'\n'; 16376], b"TAILEND".to_vec()].concat();
    assert!(bytes.len() <= 16384);
    assert_eq!(bytes.len().div_ceil(4096), 4);
    fs::write(&file, &bytes).unwrap();
    let mode = fs::metadata(&file).unwrap().mode();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(dir.clone()))).unwrap();
    let anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap();
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [anchor.at[0], anchor.at[2] + 1.05],
        yaw: 0.0,
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, "entry-0");
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    assert_chunk(&app, &bytes, 0, &file);
    assert!(!app.world.document().unwrap().contains("TAILEND"));
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut failures = Vec::new();
    for (w, h) in [(960, 640), (640, 480)] {
        app.renderer = Some(
            pollster::block_on(render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [w, h],
                &app.themes[0].vertices_for(&app.world),
            ))
            .unwrap(),
        );
        for collapsed in [true, false] {
            app.world.tree_collapsed = collapsed;
            // Reset through actual page keys, not a synthetic document/offset.
            for _ in 0..3 {
                tap(&mut app, KeyCode::ArrowLeft);
            }
            assert_chunk(&app, &bytes, 0, &file);
            for chunk in 1..4 {
                tap(&mut app, KeyCode::ArrowRight);
                assert_eq!(app.scroll, 0);
                assert_chunk(&app, &bytes, chunk * 4096, &file);
            }
            let original = app.world.document().unwrap().to_string();
            tap(&mut app, KeyCode::ArrowRight);
            assert_eq!(app.world.document().unwrap(), original, "EOF is inert");
            assert_eq!(app.scroll, 0);
            // Independent reserved-width calculation at 100% UI scale. Local
            // readers have no second in-content tree reservation or OFF header.
            let content_w = if collapsed {
                w
            } else {
                w - (w / 4).clamp(180, 280).min(w - 320)
            };
            assert_eq!(app.ui_viewport(), [content_w, h]);
            let scale = if content_w >= 640 && h >= 480 {
                2.0
            } else {
                1.0
            };
            let cols = ((content_w as f32 - 48.0) / (6.0 * scale)).max(1.0) as usize;
            let lines: Vec<String> = original
                .lines()
                .flat_map(|line| {
                    let c: Vec<_> = line.chars().collect();
                    if c.is_empty() {
                        vec![String::new()]
                    } else {
                        c.chunks(cols).map(|x| x.iter().collect()).collect()
                    }
                })
                .collect();
            let marker = lines
                .iter()
                .position(|line| line.contains("TAILEND"))
                .unwrap();
            let last = lines.len() - 1;
            let y = 80.0;
            let rows = ((h as f32 - y - 84.0) / (9.0 * scale)).max(1.0) as usize;
            let at_marker = body_geometry(&app.world, content_w, h, marker);
            assert!(!at_marker.is_empty());
            app.world.folder_view.as_mut().unwrap().reader = original.replace("TAILEND", "ZXZXZXZ");
            let control = body_geometry(&app.world, content_w, h, marker);
            assert_ne!(
                at_marker, control,
                "direct positive geometry control distinguishes actual marker"
            );
            app.world.folder_view.as_mut().unwrap().reader = original.clone();
            let mut ever_reached = false;
            let mut last_scroll = 0;
            for _ in 0..20000 {
                app.key(KeyCode::ArrowDown, true, repeated).unwrap();
                if !repeated {
                    app.key(KeyCode::ArrowDown, false, false).unwrap();
                }
                assert_eq!(
                    app.scroll,
                    (last_scroll + 1).min(last),
                    "every wrapped row, including full metadata, must be traversable"
                );
                last_scroll = app.scroll;
                if !ever_reached && marker >= app.scroll && marker < app.scroll + rows {
                    let reached = body_geometry(&app.world, content_w, h, app.scroll);
                    app.world.folder_view.as_mut().unwrap().reader =
                        original.replace("TAILEND", "ZXZXZXZ");
                    let other = body_geometry(&app.world, content_w, h, app.scroll);
                    app.world.folder_view.as_mut().unwrap().reader = original.clone();
                    ever_reached = reached != other;
                }
            }
            app.key(KeyCode::ArrowDown, false, false).unwrap();
            // The tail is before the footer now. Visibility at document END is
            // not a body-tail requirement; instead prove both independently.
            assert_eq!(app.scroll, last);
            assert_eq!(
                hair::text::local_reader_scroll_max(&app.world, content_w, h),
                last
            );
            let end_geometry = body_geometry(&app.world, content_w, h, app.scroll);
            app.world.folder_view.as_mut().unwrap().reader =
                original.replace("262144 bytes.", "ZXZXZX ZXZXZX");
            assert_ne!(
                end_geometry,
                body_geometry(&app.world, content_w, h, app.scroll),
                "actual final footer glyphs reachable"
            );
            app.world.folder_view.as_mut().unwrap().reader = original.clone();
            println!(
                "LE01 {w}x{h} collapsed={collapsed} repeated={repeated} chunks=4 markerRow={marker} keyScroll={} rows={rows} everReachedMarkerGlyph={ever_reached} fullMetadataAndEndReached=true",
                app.scroll
            );
            if !ever_reached {
                failures.push((w, h, collapsed, marker, app.scroll, rows));
            }
        }
    }
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert_eq!(fs::metadata(&file).unwrap().mode(), mode);
    assert!(
        failures.is_empty(),
        "LE01 supported text tail unreachable via real App E/Right/Down: {failures:?}"
    );
}
