//! Parent-owned ignored real offscreen Metal captures. Synthetic fixture fetcher,
//! real App feedback + shared renderer, not live/native/visual acceptance.
use super::*;
use hair::render::web_feedback::Snapshot;

fn capture(a: &mut App, output: &hair::resources::SmokeOutput, name: &str, view: &Snapshot) {
    a.update_hud(); // same HUD synchronization as ordinary redraw
    let context_owned = a.world.reading || a.address.is_some();
    a.renderer
        .as_mut()
        .unwrap()
        .set_web_feedback(view, context_owned);
    a.renderer
        .as_ref()
        .unwrap()
        .capture(output, name, &a.pose, &a.world)
        .unwrap();
}
fn group(root: &Path, base: &Path, name: &str) -> hair::resources::SmokeOutput {
    let output = base.join(name);
    std::fs::create_dir(&output).expect("exclusive absent capture group required");
    hair::resources::smoke_output(root, &output).unwrap()
}
fn captures(size: [u32; 2]) {
    // Refuse absent/relative/nonempty/overlapping destinations before GPU work.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let base = PathBuf::from(std::env::var_os("HAIR_VORTEX_CAPTURE_OUTPUT").expect(
        "set HAIR_VORTEX_CAPTURE_OUTPUT to an existing empty exclusive directory outside source",
    ));
    assert!(base.is_absolute());
    let base = base.canonicalize().unwrap();
    assert!(!base.starts_with(&root) && !root.starts_with(&base));
    assert!(
        std::fs::read_dir(&base).unwrap().next().is_none(),
        "fresh empty output only"
    );
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
    let mut a = app();
    let vertices = a.themes[0].vertices_for(&a.world);
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        size,
        &vertices,
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(renderer);
    let pose = a.pose;
    let current = a.world.current.clone();
    let document = a.world.document().unwrap().to_string();
    let destination = format!("https://example.org/{}?q=A%2Fb#ExactTail", "aB".repeat(980));
    a.request_web(&destination, Mode::Follow).unwrap();
    completed(&a); // genuinely occupied, completed-unpolled; no live request
    let mut pending = a.web_visual_at(Instant::now());
    pending.seconds = 0.0; // explicit sampled animation times, not elapsed measurement
    pending.static_motion = false;
    let motion = group(&root, &base, "motion");
    capture(&mut a, &motion, "office.png", &pending);
    pending.seconds = 0.75;
    capture(&mut a, &motion, "reader-welcome.png", &pending);
    pending.static_motion = true;
    capture(&mut a, &motion, "reader-workshop.png", &pending);
    let context = group(&root, &base, "context");
    pending.details = true;
    let mut reached_tail = false;
    for scroll in 0..1024 {
        pending.scroll = scroll;
        if pending.visible_rows(size).join("").contains("#ExactTail") {
            reached_tail = true;
            break;
        }
    }
    assert!(reached_tail);
    capture(&mut a, &context, "office.png", &pending); // long exact tail disclosure
    a.world.reading = true;
    capture(&mut a, &context, "reader-welcome.png", &pending);
    a.world.reading = false;
    tap(&mut a, KeyCode::KeyR);
    let cancel = a.web_visual_at(Instant::now());
    assert_eq!(cancel.mode, VisualMode::Cancelled);
    let terminal = group(&root, &base, "terminal");
    capture(&mut a, &terminal, "reader-workshop.png", &cancel);
    finish(&mut a);
    assert_eq!(a.world.current, current);
    assert_eq!(a.world.document().unwrap(), document);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    a.request_web("https://example.org/bad", Mode::Follow)
        .unwrap();
    finish(&mut a);
    let now = Instant::now();
    let red = a.web_visual_at(now);
    assert_eq!(red.mode, VisualMode::FailureRed);
    let settled = a.web_visual_at(now + web_native::FAILURE_RED_DURATION);
    assert_eq!(settled.mode, VisualMode::FailureSettled);
    assert_eq!(red.reason, settled.reason);
    capture(&mut a, &terminal, "office.png", &red);
    capture(&mut a, &terminal, "reader-welcome.png", &settled);
    assert_eq!(a.world.current, current);
    assert_eq!(
        (a.pose.position, a.pose.yaw, a.pose.pitch),
        (pose.position, pose.yaw, pose.pitch)
    );
    // Initial address failure has no web doorway or Runtime, yet real completion
    // feedback must render while the existing address recovery stays usable.
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut initial = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    initial.renderer = a.renderer.take();
    let r = initial.renderer.as_mut().unwrap();
    r.set_theme_appearance(&initial.themes[0]).unwrap();
    r.set_scene(&initial.themes[0].vertices_for(&initial.world));
    initial.web = Some(web_native::Runtime::new(Vec::new(), fixture));
    initial
        .request_web("https://example.org/bad", Mode::Initial)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while initial.web.as_ref().is_some_and(|w| w.pending.is_some()) {
        initial.poll_web().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(initial.web.is_none() && initial.address.is_some());
    let view = initial.web_visual_at(Instant::now());
    assert_eq!(view.mode, VisualMode::FailureRed);
    capture(&mut initial, &context, "reader-workshop.png", &view);
    initial.renderer.as_ref().unwrap().wait().unwrap();
    println!(
        "VORTEX_CAPTURE_PREPARED_PATH_EXECUTED size={size:?} output={} inspect_images_separately=true synthetic_fixture=true native=false live=false",
        base.display()
    );
}
#[test]
#[ignore = "parent-only Metal; explicit exclusive HAIR_VORTEX_CAPTURE_OUTPUT; image inspection separate"]
fn vortex_metal_normal_capture() {
    captures([960, 640]);
}
#[test]
#[ignore = "parent-only Metal; explicit exclusive HAIR_VORTEX_CAPTURE_OUTPUT; image inspection separate"]
fn vortex_metal_small_capture() {
    captures([640, 480]);
}
