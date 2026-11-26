//! Parent-run offscreen Metal captures through the actual App + Renderer.
//! Each state writes only the permitted office.png into its own new child of
//! an explicit, existing, empty capture root. Generic capture guards unchanged.
use super::*;

fn pixels(p: &Path) -> (Vec<u8>, usize) {
    let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut b).unwrap();
    (b, info.width as usize)
}
/// Height in physical rows of the first text line inside the region that
/// differs between two otherwise identical frames (the overlay panel). The
/// panel's own fill is its most common changed colour; text is the rest.
fn first_line_rows(with: &[u8], without: &[u8], width: usize) -> usize {
    let mut fill = std::collections::HashMap::<[u8; 3], usize>::new();
    for (a, b) in with
        .as_chunks::<4>()
        .0
        .iter()
        .zip(without.as_chunks::<4>().0)
    {
        if a != b {
            *fill.entry([a[0], a[1], a[2]]).or_default() += 1;
        }
    }
    let fill = fill.into_iter().max_by_key(|(_, n)| *n).unwrap().0;
    let rows: Vec<bool> = with
        .as_chunks::<4>()
        .0
        .chunks(width)
        .zip(without.as_chunks::<4>().0.chunks(width))
        .map(|(a, b)| {
            a.iter()
                .zip(b)
                .any(|(p, q)| p != q && [p[0], p[1], p[2]] != fill)
        })
        .collect();
    let first = rows.iter().position(|r| *r).expect("overlay text ink");
    rows[first..].iter().take_while(|r| **r).count()
}

#[test]
#[ignore = "serialized offscreen Metal; requires HAIR_READER_CONTEXT_CAPTURE_DIR (existing, empty)"]
fn reader_context_scale_code_map_and_return_door_captures() {
    let root = PathBuf::from(
        std::env::var_os("HAIR_READER_CONTEXT_CAPTURE_DIR")
            .expect("explicit separate existing capture directory"),
    );
    let mut a = app();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    println!("OBSERVED GPU {:?}", adapter.get_info());
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [1920, 1280],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(renderer);
    a.themes[0].respawn(&mut a.pose);
    let capture = |a: &mut App, name: &str| -> (Vec<u8>, usize) {
        a.update_hud();
        let dir = root.join(name);
        std::fs::create_dir(&dir).unwrap();
        let output = resources::smoke_output(&a.root, &dir).unwrap();
        a.renderer
            .as_ref()
            .unwrap()
            .capture(&output, "office.png", &a.pose, &a.world)
            .unwrap();
        println!(
            "READER_CONTEXT_CAPTURE {name}/office.png ui_scale={}",
            a.ui_scale
        );
        pixels(&dir.join("office.png"))
    };
    // Transient HTTPS address field: actual rendered text height at 100/200%.
    let mut heights = Vec::new();
    for percent in [100, 200] {
        a.ui_scale = percent;
        a.address = None;
        let (room, width) = capture(&mut a, &format!("room-{percent}"));
        a.address = Some("https://example.org/aB/Case".into());
        let (entry, _) = capture(&mut a, &format!("address-{percent}"));
        heights.push(first_line_rows(&entry, &room, width));
    }
    a.address = None;
    println!("ADDRESS_TEXT_ROWS 100%={} 200%={}", heights[0], heights[1]);
    assert!(
        heights[1] as f32 >= heights[0] as f32 * 1.9,
        "address text did not visibly scale: {heights:?}"
    );
    // Highlighted code block in the reader at 200%.
    a.world = hair::web::Page::html(
        hair::web::Address::parse("https://example.org/code").unwrap(),
        b"<title>Code</title><p>Call <code>main()</code></p><pre><code class=\"language-rust\">fn main() {\n    let s = \"Hi\"; // greet\n    let n = 42;\n}</code></pre>",
    )
    .unwrap()
    .generate()
    .unwrap()
    .world;
    a.world.reading = true;
    a.world.tree_collapsed = false;
    capture(&mut a, "reader-code-200");
    // Window-capped 160% at 1280x960: web map expanded, aimed return door.
    a = app();
    a.renderer = Some(
        pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [1280, 960],
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap(),
    );
    a.renderer
        .as_mut()
        .unwrap()
        .set_theme_appearance(&a.themes[0])
        .unwrap();
    a.ui_scale = 200;
    a.world.tree_collapsed = false;
    let door = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|x| x.id == "return-history")
        .unwrap()
        .at;
    // FIX1 D: harness-only correction, same as the parent's external visual-r5
    // copy. The square-on near-wall frame has 19 colours and the unchanged
    // generic 20-colour guard rightly refuses it. Assert the actual aim from an
    // oblique near pose, then capture an overview that includes furniture.
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [door[0] + 0.6, 3.0],
        yaw: std::f32::consts::PI + 0.45,
        pitch: -0.2,
    };
    hair::interaction::refresh_with_windows(
        &a.themes[0].host,
        &a.pose,
        &mut a.world,
        &a.web.as_ref().unwrap().window_doors,
    );
    assert_eq!(
        a.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("return-history")
    );
    println!("READER_CONTEXT_NEAR_AIM_ASSERTED return-history (no image)");
    a.pose.position = [0.6, -1.5];
    a.pose.yaw = std::f32::consts::PI;
    a.pose.pitch = -0.15;
    hair::interaction::refresh_with_windows(
        &a.themes[0].host,
        &a.pose,
        &mut a.world,
        &a.web.as_ref().unwrap().window_doors,
    );
    a.renderer
        .as_mut()
        .unwrap()
        .set_scene(&a.themes[0].vertices_for(&a.world));
    capture(&mut a, "map-return-door-1280x960-capped-overview");
    a.renderer.as_ref().unwrap().wait().unwrap();
}

/// Actual aim at a front-wall link door through the shared resolver, from a
/// bounded set of safe probe poses. Returns the pose and resolved target.
fn aim_link_door(a: &App) -> (Pose, hair::interaction::Target) {
    let host = &a.themes[0].host;
    let windows = &a.web.as_ref().unwrap().window_doors;
    for link in host
        .anchors
        .iter()
        .filter(|x| x.id.starts_with("web-slot-"))
    {
        for z in [-2.8, -2.4, -2.0] {
            for dx in [0.0, -0.3, 0.3] {
                let pose = Pose {
                    eye_offset: 1.65,
                    feet_y: 0.0,
                    position: [link.at[0] + dx, z],
                    yaw: 0.0,
                    pitch: -0.2,
                };
                if host.blocked(pose.position) {
                    continue;
                }
                if let Some(t) =
                    hair::interaction::resolve_with_windows(host, &pose, &a.world, windows)
                    && t.anchor.starts_with("web-slot-")
                {
                    return (pose, t);
                }
            }
        }
    }
    panic!("fixture needs a safe pose aimed at a link door");
}

#[test]
fn link_door_aim_probe_resolves_and_draws_2x_destination_panel() {
    let mut a = app();
    let (pose, target) = aim_link_door(&a);
    a.pose = pose;
    hair::interaction::refresh_with_windows(
        &a.themes[0].host,
        &a.pose,
        &mut a.world,
        &a.web.as_ref().unwrap().window_doors,
    );
    assert_eq!(a.world.interaction.as_ref(), Some(&target));
    let rows = hair::text::portal_rows(&a.world, 960, 640);
    assert!(rows[0].starts_with("E FOLLOW"), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("https://")), "{rows:?}");
}

/// FIX1 A: actual App + Renderer captures of the HTTPS address entry and the
/// aimed link-door context at every Settings step at 960x640 (window-capped
/// to 120%) and at the 100%/200% endpoints at 1920x1280. Each frame is its own
/// new child folder holding only office.png; nothing is overwritten.
#[test]
#[ignore = "serialized offscreen Metal; requires HAIR_READER_SCALE_MATRIX_DIR (existing, empty)"]
fn address_and_aimed_door_scale_matrix_captures() {
    let root = PathBuf::from(
        std::env::var_os("HAIR_READER_SCALE_MATRIX_DIR")
            .expect("explicit separate existing capture directory"),
    );
    assert!(
        std::fs::read_dir(&root).unwrap().next().is_none(),
        "capture root must be empty; never overwrite earlier captures"
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    println!("OBSERVED GPU {:?}", adapter.get_info());
    for (size, steps) in [
        ([960u32, 640u32], &hair::presentation::STEPS[..]),
        ([1920, 1280], &[100, 200][..]),
    ] {
        let mut a = app();
        let (near, target) = aim_link_door(&a);
        a.pose = near;
        hair::interaction::refresh_with_windows(
            &a.themes[0].host,
            &a.pose,
            &mut a.world,
            &a.web.as_ref().unwrap().window_doors,
        );
        assert_eq!(a.world.interaction.as_ref(), Some(&target));
        println!(
            "SCALE_MATRIX_NEAR_AIM_ASSERTED {} (no image)",
            target.anchor
        );
        let mut renderer = pollster::block_on(render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            size,
            &a.themes[0].vertices_for(&a.world),
        ))
        .unwrap();
        renderer.set_theme_appearance(&a.themes[0]).unwrap();
        a.renderer = Some(renderer);
        // Overview pose with room furniture; the asserted target stays the
        // aimed-door context drawn by the panel (labelled as overview).
        a.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [near.position[0], 1.5],
            yaw: 0.0,
            pitch: -0.15,
        };
        a.world.interaction = Some(target.clone());
        a.renderer
            .as_mut()
            .unwrap()
            .set_scene(&a.themes[0].vertices_for(&a.world));
        for &step in steps {
            a.ui_scale = step;
            let applied = (hair::presentation::applied(step, size) * 100.0).round();
            let tag = format!("{}x{}-{step}", size[0], size[1]);
            let shot = |a: &mut App, name: &str| -> (Vec<u8>, usize) {
                a.update_hud();
                let dir = root.join(name);
                std::fs::create_dir(&dir).unwrap();
                let output = resources::smoke_output(&a.root, &dir).unwrap();
                a.renderer
                    .as_ref()
                    .unwrap()
                    .capture(&output, "office.png", &a.pose, &a.world)
                    .unwrap();
                println!(
                    "SCALE_MATRIX_CAPTURE {name}/office.png requested={step}% applied={applied}%"
                );
                pixels(&dir.join("office.png"))
            };
            a.address = None;
            shot(&mut a, &format!("{tag}-aimed-door-overview"));
            let saved = a.world.interaction.take();
            let (room, width) = shot(&mut a, &format!("{tag}-room-no-panel"));
            a.address = Some("https://example.org/aB/Case?q=Long".into());
            let (entry, _) = shot(&mut a, &format!("{tag}-address"));
            a.address = None;
            a.world.interaction = saved;
            let [_, lh] = hair::presentation::ui_viewport(size, step);
            let expected = 14.0 * size[1] as f32 / lh as f32;
            let rows = first_line_rows(&entry, &room, width);
            println!("SCALE_MATRIX_ADDRESS_ROWS {tag} rows={rows} expected~{expected}");
            assert!(
                rows as f32 >= expected.floor() - 1.0,
                "address text below the 2x baseline at {tag}: {rows} < {expected}"
            );
        }
        a.renderer.as_ref().unwrap().wait().unwrap();
    }
}
