//! RED-only tracer through actual App input/poll and the existing world texture consumer.
//! No preview state, source result, material, geometry or pixel is injected.
use super::*;
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, sync::Arc};

const BODY: &str = "# aB9\nx\t\\\né\n";
const NEXT_BODY: &str = "# zQ7\n";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let base =
            PathBuf::from(std::env::var_os("TMPDIR").expect("explicit candidate-owned TMPDIR"))
                .canonicalize()
                .unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        // Short exclusive basename; no inherited workspace or socket path assumption.
        let path = base.join(format!("t{}", std::process::id()));
        fs::create_dir(&path).expect("absent exclusive text fixture");
        for name in ["a", "b", "a/sub", "b/sub"] {
            fs::create_dir(path.join(name)).unwrap();
        }
        for (name, body) in [
            ("a/n.txt", BODY),
            ("b/n.txt", NEXT_BODY),
            ("a/sub/hidden.txt", "NOT IN THIS ROOM\n"),
        ] {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path.join(name))
                .unwrap();
            file.write_all(body.as_bytes()).unwrap();
            file.flush().unwrap();
        }
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
    }
}
fn aim(app: &mut App) -> String {
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "n.txt")
        .unwrap()
        .0
        .clone();
    let host = &app.themes[app.theme_index].host;
    let a = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(&id);
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [a.at[0] + 1.05 * inward[0], a.at[2] + 1.05 * inward[1]],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
    id
}
fn preview(app: &mut App, id: &str) -> Arc<hair::assets::Texture> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        // The same production consumer as redraw, not a hypothetical text API.
        app.poll_local_image().unwrap();
        assert!(!app.world.reading);
        assert!(
            app.object_previews.cache.keys().all(|key| {
                app.world
                    .folder_view
                    .as_ref()
                    .unwrap()
                    .bindings
                    .get(key)
                    .is_some_and(|b| b.entry.kind == hair::local_explorer::Kind::File)
            }),
            "only current-room file bindings may publish previews"
        );
        if let Some(texture) = app.object_previews.cache.get(id) {
            return texture.clone();
        }
        assert!(
            Instant::now() < deadline,
            "TEXT_EXCERPT_BEFORE_E_MISSING: current-room n.txt has no literal content texture; status={}",
            app.object_previews.status
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

// Extract the existing literal 5x7 font as an oracle only. These vertices never
// enter App/Theme/Renderer. Search admits padding, either palette and 1..6x glyphs;
// it does not prescribe a texture size, origin, colors or new production API.
fn mask(line: &str) -> (usize, Vec<bool>) {
    let width = line.chars().count() * 6 - 1;
    let mut bits = vec![false; width * 7];
    for (i, c) in line.chars().enumerate() {
        let glyph = hair::text::object_label(&c.to_string(), [0.0; 3], 1);
        for quad in glyph.as_chunks::<6>().0 {
            let left = quad
                .iter()
                .map(|v| v.position[0])
                .fold(f32::INFINITY, f32::min);
            let top = quad
                .iter()
                .map(|v| v.position[1])
                .fold(f32::NEG_INFINITY, f32::max);
            let col = ((left + 0.036) / 0.012).round() as usize;
            let row = (-top / 0.012).round() as usize;
            assert!(col < 5 && row < 7);
            bits[row * width + i * 6 + col] = true;
        }
    }
    (width, bits)
}
fn has_line(texture: &hair::assets::Texture, line: &str) -> bool {
    let (mw, bits) = mask(line);
    let (w, h) = (texture.width as usize, texture.height as usize);
    let ink = bits.iter().position(|b| *b).unwrap();
    let paper = bits.iter().position(|b| !*b).unwrap();
    let pixel = |x: usize, y: usize| &texture.rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
    for scale in 1..=6 {
        if mw * scale > w || 7 * scale > h {
            continue;
        }
        for y in 0..=h - 7 * scale {
            for x in 0..=w - mw * scale {
                let fg = pixel(x + ink % mw * scale, y + ink / mw * scale);
                let bg = pixel(x + paper % mw * scale, y + paper / mw * scale);
                if !fg.iter().zip(bg).any(|(a, b)| a.abs_diff(*b) >= 64) {
                    continue;
                }
                if bits.iter().enumerate().all(|(i, bit)| {
                    (0..scale).all(|dy| {
                        (0..scale).all(|dx| {
                            pixel(x + i % mw * scale + dx, y + i / mw * scale + dy)
                                == if *bit { fg } else { bg }
                        })
                    })
                }) {
                    return true;
                }
            }
        }
    }
    false
}
fn literal_texture(texture: &hair::assets::Texture, line: &str) {
    assert!((1..=1024).contains(&texture.width) && (1..=1024).contains(&texture.height));
    assert_eq!(
        texture.rgba.len(),
        (texture.width * texture.height * 4) as usize
    );
    assert!(texture.rgba.len() <= 4 * 1024 * 1024);
    assert!(texture.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    assert!(
        has_line(texture, line),
        "TEXT_EXCERPT_LITERAL_PIXELS_MISSING: {line:?}"
    );
}
fn world_consumer(app: &App, id: &str, texture: &Arc<hair::assets::Texture>) {
    let theme = &app.themes[app.theme_index];
    let material = theme
        .textures
        .iter()
        .position(|t| Arc::ptr_eq(t, texture))
        .expect("normal App poll must publish excerpt to Theme materials")
        as u32
        + 1;
    let a = theme.host.anchors.iter().find(|a| a.id == id).unwrap();
    let scene = theme.vertices_for(&app.world);
    let plane: Vec<_> = scene
        .iter()
        .filter(|v| v.texture == (material | 0x8000_0000))
        .collect();
    let model = hair::file_items::Catalog::builtin()
        .unwrap()
        .representation(
            &app.world.folder_view.as_ref().unwrap().bindings[id]
                .entry
                .name,
        );
    let expected = super::local_model_preview_tests::reference_floating(&model.vertices, a)
        .into_iter()
        .filter(|v| v.texture != 0)
        .map(|v| hair::assets::Vertex {
            texture: material | 0x8000_0000,
            ..v
        })
        .collect::<Vec<_>>();
    assert!(!expected.is_empty() && expected.len().is_multiple_of(3));
    assert_eq!(
        plane.len(),
        expected.len(),
        "literal texture occupies the authored class-item surface, not HUD"
    );
    for (actual, expected) in plane.into_iter().zip(expected) {
        assert!((0..3).all(|i| (actual.position[i] - expected.position[i]).abs() < 0.00001));
        assert_eq!(
            (actual.uv, actual.color, actual.texture),
            (expected.uv, expected.color, expected.texture)
        );
    }
}
fn raw_bytes(document: &str) -> Option<Vec<u8>> {
    let body = document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    let mut bytes = Vec::new();
    for line in body.lines().filter_map(|line| line.strip_prefix("HEX ")) {
        for token in line.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
    }
    Some(bytes)
}

#[test]
fn literal_text_before_e_reader_raw_escape_and_reused_slot() {
    let fixture = Fixture::new();
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
    app.complete_folder(Ok(Some(fixture.0.join("a")))).unwrap();
    let id = aim(&mut app);
    let bound = app.world.folder_view.as_ref().unwrap().bindings[&id].clone();
    assert_eq!(bound.entry.path, fixture.0.join("a/n.txt"));
    assert!(
        !app.world
            .folder_view
            .as_ref()
            .unwrap()
            .bindings
            .values()
            .any(|b| b.entry.name == "hidden.txt")
    );
    let host = app.themes[0].host.clone();
    let current = app.world.current.clone();
    assert!(
        app.object_previews.cache.is_empty(),
        "room selection queues lazy work only"
    );
    let first = preview(&mut app, &id); // Intended baseline RED, before any E.
    literal_texture(&first, "# aB9"); // '#' and mixed case are literal, not Markdown.
    literal_texture(&first, "x\\x09\\\\");
    literal_texture(&first, "\\u{E9}");
    assert!(
        !has_line(&first, "# AB9"),
        "preview must not uppercase source text"
    );
    world_consumer(&app, &id, &first);
    assert_eq!(app.themes[0].host, host);

    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    let text = app.world.document().unwrap();
    assert!(text.starts_with("TEXT: UTF-8 interpretation"));
    assert!(text.contains("# aB9\nx\\x09\\\\\n\\u{E9}\n"));
    assert!(text.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
    tap(&mut app, KeyCode::KeyV);
    assert_eq!(
        raw_bytes(app.world.document().unwrap()).as_deref(),
        Some(BODY.as_bytes())
    );
    let raw = app.world.document().unwrap().to_string();
    tap(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        app.world.document().unwrap(),
        raw,
        "small owned source EOF is inert"
    );
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    assert_eq!(app.world.current, current);
    assert_eq!(app.world.folder_view.as_ref().unwrap().bindings[&id], bound);
    assert_eq!(app.themes[0].host, host);
    let restored = preview(&mut app, &id); // No F5 or state injection.
    literal_texture(&restored, "# aB9");
    world_consumer(&app, &id, &restored);

    // Same raw basename and slot in another directory must not inherit A's text.
    // This is a consumer-level replacement check, not a deterministic delayed-worker race.
    app.complete_folder(Ok(Some(fixture.0.join("b")))).unwrap();
    let next_id = aim(&mut app);
    assert_eq!(
        next_id, id,
        "fixture must genuinely reuse the document slot"
    );
    assert!(app.object_previews.cache.is_empty());
    let next = preview(&mut app, &next_id);
    literal_texture(&next, "# zQ7");
    assert!(!has_line(&next, "# aB9"));
    world_consumer(&app, &next_id, &next);
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().bindings[&next_id]
            .entry
            .path,
        fixture.0.join("b/n.txt")
    );
    assert_eq!(
        fs::read(fixture.0.join("a/n.txt")).unwrap(),
        BODY.as_bytes()
    );
    assert_eq!(
        fs::read(fixture.0.join("b/n.txt")).unwrap(),
        NEXT_BODY.as_bytes()
    );
    assert_eq!(
        fs::read(fixture.0.join("a/sub/hidden.txt")).unwrap(),
        b"NOT IN THIS ROOM\n"
    );
    drop(app); // Join/retire owned workers before fixture cleanup.
}
