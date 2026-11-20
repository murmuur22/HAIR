use hair::{Pose, render, resources, theme, web};
use std::path::{Path, PathBuf};

fn page() -> web::Generated {
    web::Page::html(
        web::Address::parse("https://example.org/Reading?q=aB%2Fcd#Source").unwrap(),
        b"<title>Reader layout fixture</title><h1>Readable content</h1><p>Minimal reader; source is literal, not uppercased.</p><a href='/Next?q=aB%2Fcd#Target'>Next destination</a>",
    ).unwrap().generate().unwrap()
}
fn pixels(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let (w, h) = (r.info().width, r.info().height);
    let mut p = vec![0; r.output_buffer_size().unwrap()];
    r.next_frame(&mut p).unwrap();
    (w, h, p)
}
fn srgb(v: f32) -> u8 {
    ((if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }) * 255.0)
        .round() as u8
}
#[test]
#[ignore = "explicit fresh Metal captures in exclusive owned output"]
fn reader_owns_footer_without_global_status_strip() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("HAIR_LAYOUT_CAPTURES").unwrap());
    let mut g = page();
    g.world.reading = true;
    g.world.cycle_reader_link(false);
    let pack = theme::Theme::load_for_scene(
        &root.join("packs/office.toml"),
        &mut Default::default(),
        &g.host,
    )
    .unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
        println!("ACTUAL_ADAPTER {:?}", adapter.get_info());
        for (w, h) in [(960, 640), (640, 480)] {
            let dir = out.join(format!("reader-{w}x{h}"));
            std::fs::create_dir_all(&dir).unwrap();
            let output = resources::smoke_output(&root, &dir).unwrap();
            let mut r = render::Renderer::new(
                &adapter,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                [w, h],
                &pack.vertices_for(&g.world),
            )
            .await
            .unwrap();
            r.set_materials(&pack.textures).unwrap();
            r.pack_feedback = "L address | R Return | F5 Refresh\nPack office loaded".into();
            r.capture(&output, "reader-welcome.png", &Pose::default(), &g.world)
                .unwrap();
            let (actual_w, actual_h, p) = pixels(&dir.join("reader-welcome.png"));
            assert_eq!((actual_w, actual_h), (w, h));
            let x = 14;
            // Parent's 2x status panels have a gap at h-59. Sample inside
            // the second panel's top padding (h-58..h-56), before glyph ink.
            let y = h as usize - 57;
            let actual = &p[(y * w as usize + x) * 4..][..4];
            let expected = [srgb(0.025), srgb(0.045), srgb(0.035), 255];
            assert!(
                actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
                "reader footer split by global HUD at {w}x{h} ({x},{y}): actual={actual:?} expected={expected:?}"
            );

            // Same protected output handle, three existing allowlisted names.
            // office.png = closed reader; reader-workshop.png = address entry.
            g.world.reading = false;
            r.capture(&output, "office.png", &Pose::default(), &g.world)
                .unwrap();
            let (cw, ch, closed) = pixels(&dir.join("office.png"));
            assert_eq!((cw, ch), (w, h));
            let status = &closed[(y * w as usize + x) * 4..][..4];
            let neutral = [srgb(0.025), srgb(0.025), srgb(0.025), 255];
            assert!(
                status.iter().zip(neutral).all(|(a, e)| a.abs_diff(e) <= 2),
                "closed reader must retain global status"
            );
            r.address_entry = Some((
                "https://example.org/Next?q=aB%2Fcd#Target".into(),
                Some("Refused; edit literal address or Escape to cancel".into()),
                0,
            ));
            r.capture(&output, "reader-workshop.png", &Pose::default(), &g.world)
                .unwrap();
            let (aw, ah, address) = pixels(&dir.join("reader-workshop.png"));
            assert_eq!((aw, ah), (w, h));
            assert_ne!(
                &address[(y * w as usize + x) * 4..][..4],
                status,
                "address entry must suppress global status"
            );
            println!(
                "CAPTURE_SEMANTICS {w}x{h}: reader-welcome.png=reader office.png=closed reader-workshop.png=address-entry; protected writer and generic screenshot guard unchanged"
            );
            g.world.reading = true;
        }
    });
}
