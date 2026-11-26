//! Finite actual Renderer readback of the unchanged placard oracle/lifecycle.
//! Synthetic App inputs, not native/physical input or human legibility acceptance.
use super::*;

pub(super) fn attach(app: &mut App) {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Metal);
    println!("OWNER_R1B_CAPTION_ADAPTER {info:?}");
    let mut renderer = pollster::block_on(render::Renderer::new(&adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb, SIZES[0],
        &app.themes[0].vertices_for(&app.world))).unwrap();
    renderer.set_theme_appearance(&app.themes[0]).unwrap();
    app.renderer = Some(renderer);
}
fn capture(app: &App, output: &Path, label: &str, size: [u32; 2]) -> Vec<u8> {
    let dir = output.join(label);
    fs::create_dir(&dir).unwrap();
    let target = resources::smoke_output(&app.root, &dir).unwrap();
    let receipt = format!("label={label}\nviewport={size:?}\npose={:?}\nstatus={}\nproduction_guard=required; no sparse fallback\nsource_test_sha256={:x}\n",
        app.pose, app.local_status, Sha256::digest(fs::read(app.root.join("src/owner_r1b_caption_metal_tests.rs")).unwrap()));
    exclusive(&dir.join("capture.txt"), receipt.as_bytes());
    let result = app.renderer.as_ref().unwrap().capture(&target, "office.png", &app.pose, &app.world);
    if let Err(error) = &result { exclusive(&dir.join("FAILURE.txt"), error.as_bytes()); }
    result.unwrap();
    let bytes = fs::read(dir.join("office.png")).unwrap();
    exclusive(&dir.join("png-sha256.txt"), format!("{:x}\n", Sha256::digest(&bytes)).as_bytes());
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    assert_eq!([info.width, info.height], size);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    pixels.truncate(info.buffer_size());
    pixels
}
pub(super) fn observe(app: &mut App, output: &Path, label: &str, size: [u32; 2], parts: &[Vec<Vertex>; 2]) {
    app.renderer.as_mut().unwrap().resize(size);
    app.update_hud();
    let positive = capture(app, output, label, size);
    let scene = app.themes[0].vertices_for(&app.world);
    let m = matrix(&app.pose, size);
    for (role, mesh) in ["filename", "type"].into_iter().zip(parts) {
        let points: Vec<_> = mesh.as_chunks::<3>().0.iter().flat_map(|t| projected(t, m, size)).collect();
        let bounds = points.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p|
            [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]);
        let negative_scene = difference(&scene, mesh);
        assert_eq!(scene.len() - negative_scene.len(), mesh.len());
        app.renderer.as_mut().unwrap().set_scene(&negative_scene); // Negative only.
        let negative = capture(app, output, &format!("{label}-erased-{role}-negative"), size);
        let mut changed = 0;
        for y in bounds[1].floor().max(0.) as usize..bounds[3].ceil().min(f64::from(size[1])) as usize {
            for x in bounds[0].floor().max(0.) as usize..bounds[2].ceil().min(f64::from(size[0])) as usize {
                let i = (y * size[0] as usize + x) * 4;
                if positive[i..i+3] != negative[i..i+3] { changed += 1; }
            }
        }
        let receipt = format!("actual_gpu_changed_pixels={changed}\nprojected_literal_{role}_bounds={bounds:?}\nonly exact literal label triangles erased; image, camera, HUD unchanged\n");
        exclusive(&output.join(format!("{label}-{role}-roi.txt")), receipt.as_bytes());
        println!("OWNER_R1B_CAPTION_GPU {label} {role}: {receipt}");
        assert!(changed > 20, "actual literal caption must reach its own projected pixel ROI, not just HUD");
    }
    app.place_scene_refresh(); // Restore through production consumers.
    app.update_hud();
    let restored = capture(app, output, &format!("{label}-production-restored"), size);
    assert_eq!(restored, positive);
}
#[test]
#[ignore = "explicit finite serialized Metal; fresh HAIR_OWNER_R1B_CAPTION_OUTPUT required"]
fn owner_r1b_actual_metal_png_jpeg_captions_with_erasure_controls() {
    let output = PathBuf::from(std::env::var_os("HAIR_OWNER_R1B_CAPTION_OUTPUT").expect("fresh exclusive output"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(output.parent(), root.parent());
    fs::create_dir(&output).unwrap();
    exclusive(&output.join("PROVENANCE.txt"), b"Actual offscreen Metal, production Renderer capture guards. Same unchanged PNG/JPEG placard geometry/visibility/held-source/lifecycle oracle as ordinary test; real App E/Escape/I inputs and real preview workers. Exact filename/type erasure negatives in projected label-only pixel ROIs, exact production-restored frames. Not OCR/native/human acceptance; package_ready=false.\n");
    placard_trace(Some(&output));
    exclusive(&output.join("WITNESS-PASS.txt"), b"Finite PNG/JPEG caption GPU/lifecycle assertions passed at 960x640 and 640x480. Parent visual/fullsuite/review gates remain. package_ready=false.\n");
}
