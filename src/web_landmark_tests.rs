use super::*;
use sha2::{Digest, Sha256};
fn tap(a: &mut App, k: KeyCode) {
    a.key(k, true, false).unwrap();
    a.key(k, false, false).unwrap();
}
fn state(a: &App) -> String {
    format!(
        "{:?}",
        (
            (
                &a.world.current,
                a.world.document(),
                a.world.reading,
                &a.world.reader_link,
                &a.world.explicit_target,
                a.scroll
            ),
            (
                a.pose.position,
                a.pose.yaw,
                a.pose.pitch,
                a.theme_index,
                &a.themes[a.theme_index].source,
                &a.themes[a.theme_index].host,
                &a.web.as_ref().unwrap().doors,
                a.web.as_ref().unwrap().history.len()
            )
        )
    )
}
fn colliding(a: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    let mut resources = std::collections::BTreeMap::new();
    for i in 0..100 {
        let r = format!("https://example.org/collision-{i}");
        let slot = Sha256::digest(r.as_bytes())[0] % 8;
        if let Some(other) = resources.insert(slot, r.clone()) {
            return Page::html(
                a,
                format!("<p>Collision groups</p><a href='{other}'>One</a><a href='{r}'>Two</a>")
                    .as_bytes(),
            );
        }
    }
    unreachable!()
}
fn overflow(a: Address, _: &hair::acquisition::Cancellation, _: Instant) -> Result<Page> {
    Page::html(
        a,
        format!(
            "<p>Overflow groups</p>{}",
            (0..9)
                .map(|i| format!("<a href='/o{i}'>Target</a>"))
                .collect::<String>()
        )
        .as_bytes(),
    )
}
#[test]
fn collision_overflow_follow_refresh_and_back_are_usable_without_page_refusal() {
    let mut a = landmark_app();
    let entry = a.world.current.clone();
    for (fetch, count) in [
        (colliding as web_native::Fetcher, 2),
        (overflow as web_native::Fetcher, 9),
    ] {
        a.web.as_mut().unwrap().fetcher = fetch;
        let pose = a.pose;
        tap(&mut a, KeyCode::F5);
        finish(&mut a);
        assert_eq!(a.pose.position, pose.position);
        assert_eq!(
            a.web
                .as_ref()
                .unwrap()
                .doors
                .iter()
                .map(|d| d.destinations.len())
                .sum::<usize>(),
            count
        );
        assert!(!a.web.as_ref().unwrap().status.contains("Refused:"));
        let before = state(&a);
        a.request_web("https://example.org/available", web_native::Mode::Follow)
            .unwrap();
        finish(&mut a);
        assert_eq!(a.world.current, "https://example.org/available");
        assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
        tap(&mut a, KeyCode::KeyR);
        assert_eq!(state(&a), before);
        assert_eq!(a.world.current, entry);
        assert_eq!(a.theme_index, 1);
    }
}
#[test]
fn deleted_current_page_returns_surviving_entry_and_root_escape_is_available() {
    let mut a = landmark_app();
    let root = state(&a);
    a.request_web("https://example.org/deleted", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    a.web.as_mut().unwrap().fetcher = |_, _, _| Err("fixture HTTP 410 current page deleted".into());
    let current = state(&a);
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert_eq!(state(&a), current);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(state(&a), root);
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert_eq!(state(&a), root);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(state(&a), root);
    assert!(a.web.as_ref().unwrap().status.contains("surviving entry"));
    a.world.reading = true;
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
    assert!(!a.world.reading);
    // Presentation menu migration: root Escape stays available but pauses instead
    // of exiting; Q focuses the explicit Quit choice; Enter confirms the exit.
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
    assert!(a.menu.is_some());
    assert!(!a.key(KeyCode::KeyQ, true, false).unwrap());
    assert!(a.key(KeyCode::Enter, true, false).unwrap());
}
static RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn late(a: Address, _: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    while !RELEASE.load(std::sync::atomic::Ordering::SeqCst) {
        if Instant::now() > d {
            return Err("fixture deadline".into());
        }
        std::thread::yield_now();
    }
    landmark_fixture(a, &Default::default(), d)
}
#[test]
fn returning_while_worker_is_unfinished_never_activates_stale_success() {
    let mut a = landmark_app();
    let entry = state(&a);
    a.request_web("https://example.org/current", web_native::Mode::Follow)
        .unwrap();
    finish(&mut a);
    RELEASE.store(false, std::sync::atomic::Ordering::SeqCst);
    a.web.as_mut().unwrap().fetcher = late;
    a.request_web("https://example.org/stale", web_native::Mode::Follow)
        .unwrap();
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(state(&a), entry);
    assert!(
        a.request_web("https://example.org/too-early", web_native::Mode::Follow)
            .is_err()
    );
    RELEASE.store(true, std::sync::atomic::Ordering::SeqCst);
    finish(&mut a);
    assert_eq!(state(&a), entry);
    assert!(a.web.as_ref().unwrap().status.contains("Cancelled"));
    a.web.as_mut().unwrap().fetcher = landmark_fixture;
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert_eq!(a.world.current, "https://example.org/one?q=AB%2Fcd#top");
    assert_eq!(a.theme_index, 1);
}
#[test]
fn closed_reader_refresh_keeps_surviving_aimed_selection_and_e_destination() {
    let mut a = landmark_app();
    let d = a
        .web
        .as_ref()
        .unwrap()
        .doors
        .iter()
        .find(|d| d.resource.ends_with("/b"))
        .unwrap()
        .clone();
    let anchor = a.themes[1]
        .host
        .anchors
        .iter()
        .find(|x| x.id == d.anchor)
        .unwrap();
    a.pose = Pose {
        position: [anchor.at[0], -2.8],
        yaw: 0.0,
        pitch: -0.2,
    };
    tap(&mut a, KeyCode::Tab);
    let target = hair::interaction::resolve(&a.themes[1].host, &a.pose, &a.world).unwrap();
    assert_eq!(target.anchor, d.anchor);
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    let next = a.themes[1]
        .host
        .anchors
        .iter()
        .find(|x| x.id == target.anchor)
        .unwrap();
    let inside = a.themes[1].host.portal_inward(&target.anchor);
    a.pose.position = [next.at[0] + inside[0], next.at[2] + inside[1]];
    a.pose.yaw = (-inside[0]).atan2(inside[1]);
    hair::interaction::refresh(&a.themes[1].host, &a.pose, &mut a.world);
    assert_eq!(
        hair::interaction::resolve(&a.themes[1].host, &a.pose, &a.world).unwrap(),
        target
    );
    let departure = a.pose.position;
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(a.world.current, d.destinations[0]);
    assert_eq!(a.theme_index, 1);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.pose.position, departure);
    assert_eq!(a.world.current, "https://example.org/one?q=AB%2Fcd#top");
}
fn pixels(p: &Path) -> Vec<u8> {
    let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    r.next_frame(&mut b).unwrap();
    b
}
#[test]
#[ignore = "finite offscreen Metal generated-room reskin/refresh via actual T/F5 key adapter; explicit capture root"]
fn rendered_fixed_landmarks_reskin_refresh() {
    let dir = PathBuf::from(std::env::var_os("HAIR_STABLE_CAPTURE_DIR").unwrap());
    let mut a = landmark_app();
    a.pose = Pose {
        position: [0.0, 3.5],
        yaw: 0.0,
        pitch: -0.15,
    };
    // Optional separately authored texture fixtures qualify the same geometry without
    // weakening the generic capture guard. Sparse baseline failure remains recorded.
    if let Some(packs) = std::env::var_os("HAIR_STABLE_PACK_DIR") {
        let packs = PathBuf::from(packs);
        let host = a.themes[1].host.clone();
        let mut cache = hair::theme::AssetCache::default();
        a.themes = ["office.toml", "archive.toml"]
            .iter()
            .map(|p| hair::theme::Theme::load_for_scene(&packs.join(p), &mut cache, &host).unwrap())
            .collect();
    }
    let output = resources::smoke_output(&a.root, &dir).unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        println!("OBSERVED GPU {:?}", adapter.get_info());
        let mut r = render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [960, 640],
            &a.themes[1].vertices_for(&a.world),
        )
        .await
        .unwrap();
        r.set_materials(&a.themes[1].textures).unwrap();
        r.capture(&output, "office.png", &a.pose, &a.world).unwrap();
        a.renderer = Some(r);
        let before = state(&a);
        let host = a.themes[1].host.clone();
        tap(&mut a, KeyCode::KeyT);
        assert_eq!(a.theme_index, 0);
        assert_eq!(a.themes[0].host, host);
        for x in -32..=32 {
            for z in -15..=15 {
                let p = [x as f32 * 0.25, z as f32 * 0.25];
                assert_eq!(a.themes[0].blocked(p), a.themes[1].blocked(p));
            }
        }
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&output, "reader-welcome.png", &a.pose, &a.world)
            .unwrap();
        let p = pixels(&dir.join("office.png"));
        let q = pixels(&dir.join("reader-welcome.png"));
        let changed = p
            .as_chunks::<4>()
            .0
            .iter()
            .zip(q.as_chunks::<4>().0)
            .filter(|(x, y)| x != y)
            .count();
        assert!(changed > 100, "reskin not visibly different");
        tap(&mut a, KeyCode::KeyT);
        assert_eq!(state(&a), before);
        let pose = a.pose;
        tap(&mut a, KeyCode::F5);
        finish(&mut a);
        assert_eq!(
            (a.pose.position, a.pose.yaw, a.pose.pitch),
            (pose.position, pose.yaw, pose.pitch)
        );
        for anchor in &host.anchors {
            assert!(a.themes[1].host.anchors.contains(anchor));
        }
        assert!(!a.world.reading);
        assert_eq!(a.theme_index, 1);
        a.renderer
            .as_mut()
            .unwrap()
            .capture(&output, "reader-workshop.png", &a.pose, &a.world)
            .unwrap();
        a.renderer.as_mut().unwrap().wait().unwrap();
        println!(
            "OBSERVED rendered changed_pixels={changed}; reader_closed=true; fixed_shell/camera/pack/anchors/collision preserved; refreshed source content=true"
        );
    });
}
