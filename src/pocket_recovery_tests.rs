use super::*;

fn overflow_page(
    a: Address,
    c: &hair::acquisition::Cancellation,
    d: Instant,
    bodies: &[&str],
) -> Result<Page> {
    c.check(d)?;
    let mut html = String::new();
    for i in 0..7 {
        html.push_str(&format!("<h2>Retained collection</h2><p>Filler {i}</p>"));
    }
    for body in bodies {
        html.push_str(&format!("<h2>Version ONE</h2><p>{body}</p>"));
    }
    Page::html(a, html.as_bytes())
}
fn overflow_before(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    overflow_page(a, c, d, &["First text", "Second text"])
}
fn overflow_after(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    overflow_page(a, c, d, &["Second text"])
}
fn singleton_before(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    overflow_page(a, c, d, &["First text"])
}
#[test]
fn overflow_hidden_origin_duplicate_must_not_rebind_to_promoted_survivor() {
    let mut a = app(overflow_before, "First text");
    let p = a.world.pocket.as_ref().unwrap();
    assert_eq!(
        p.documents
            .values()
            .filter(|d| d.title == "Version ONE")
            .count(),
        1,
        "fixture must hide the duplicate from grouped-title counting"
    );
    assert!(
        p.documents
            .values()
            .any(|d| d.title == "Multiple sections" && d.text.contains("Second text"))
    );
    let pose = a.pose;
    let source = a.world.current.clone();
    let pack = a.theme_index;
    refresh(&mut a, overflow_after);
    assert!(
        a.world.pocket.as_ref().unwrap().reading.is_none(),
        "grouped titles cannot prove origin uniqueness across overflow"
    );
    assert!(
        a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED")
    );
    assert_eq!(a.world.current, source);
    assert_eq!(a.theme_index, pack);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.pose.yaw, pose.yaw);
    assert_eq!(a.pose.pitch, pose.pitch);
    assert!(a.web.as_ref().unwrap().history.is_empty());
    assert!(!a.world.document().unwrap().contains("First text"));
    a.web.as_mut().unwrap().shutdown();
}
#[test]
fn newly_introduced_duplicate_hidden_in_overflow_requires_explicit_recovery() {
    let mut a = app(singleton_before, "First text");
    assert!(
        a.world
            .pocket
            .as_ref()
            .unwrap()
            .documents
            .values()
            .all(|d| d.title != "Multiple sections")
    );
    refresh(&mut a, overflow_before);
    assert!(
        a.world.pocket.as_ref().unwrap().reading.is_none(),
        "new overflow must not conceal destination-title ambiguity"
    );
    assert!(
        a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED")
    );
    assert!(
        a.world.document().unwrap().contains("First text")
            && a.world.document().unwrap().contains("Second text")
    );
    a.web.as_mut().unwrap().shutdown();
}
fn duplicate_before(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(
        a,
        b"<h2>Same</h2>First text<h2>Same</h2>Second text<h2>Other</h2>Other text",
    )
}
fn duplicate_after(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(a, b"<h2>Same</h2>Second text<h2>Other</h2>Other text")
}
fn unique_before(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(
        a,
        b"<h2>Orchard</h2>Apple trees<h2>Harbour</h2>Boats arrive<h2>Mountain</h2>Rocks",
    )
}
fn unique_after(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(a, b"<h2>Harbour</h2>Boats arrive<h2>Mountain</h2>Rocks")
}
fn app(fetch: web_native::Fetcher, wanted: &str) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/pocket").unwrap(),
        &[
            root.join("packs/office.toml"),
            root.join("packs/archive.toml"),
        ],
        fetch,
        &Default::default(),
    )
    .unwrap();
    let mut a = App::new(root, p.themes, 0, p.generated.world, false);
    let pocket = a.world.pocket.as_mut().unwrap();
    pocket.reading = Some(
        pocket
            .documents
            .iter()
            .find(|(_, d)| d.text.contains(wanted))
            .unwrap()
            .0
            .clone(),
    );
    a.world.reading = true;
    a.web = Some(web_native::Runtime::new(p.generated.doors, fetch));
    a
}
fn refresh(a: &mut App, fetch: web_native::Fetcher) {
    a.web.as_mut().unwrap().fetcher = fetch;
    a.keyboard_input(Some(KeyCode::F5), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::F5), false, false, None)
        .unwrap();
    finish(a);
}
#[test]
fn duplicate_origin_title_cannot_silently_rebind_after_one_duplicate_is_removed() {
    for wanted in ["First text", "Second text"] {
        let mut a = app(duplicate_before, wanted);
        let pose = a.pose;
        let source = a.world.current.clone();
        let pack = a.theme_index;
        refresh(&mut a, duplicate_after);
        assert!(a.world.reading);
        assert!(
            a.world.pocket.as_ref().unwrap().reading.is_none(),
            "duplicate-origin identity must recover conservatively, not select surviving Same"
        );
        assert_eq!(a.world.current, source);
        assert_eq!(a.pose.position, pose.position);
        assert_eq!(a.pose.yaw, pose.yaw);
        assert_eq!(a.pose.pitch, pose.pitch);
        assert_eq!(a.theme_index, pack);
        assert!(a.web.as_ref().unwrap().history.is_empty());
        assert!(
            a.web
                .as_ref()
                .unwrap()
                .status
                .contains("full page restored")
        );
        a.web.as_mut().unwrap().shutdown();
    }
}
#[test]
fn removed_collection_has_recovery_notice_inside_reader_not_just_window_title() {
    let mut a = app(unique_before, "Apple trees");
    a.scroll = 19;
    refresh(&mut a, unique_after);
    assert!(a.world.reading);
    assert!(a.world.pocket.as_ref().unwrap().reading.is_none());
    assert!(
        a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED"),
        "recovery must be visible on the reader-owned surface, not only hidden global status or native title"
    );
    assert_eq!(
        a.scroll, 0,
        "recovery should show the full page beginning rather than a stale section scroll offset"
    );
    a.web.as_mut().unwrap().shutdown();
}
#[test]
#[ignore = "explicit protected Metal recovery-reader proof; not native-human input"]
fn recovery_notice_is_visible_survives_pack_and_clears_on_explicit_document_choice() {
    let out = PathBuf::from(std::env::var_os("HAIR_RECOVERY_CAPTURES").expect("explicit output"));
    assert!(!out.exists());
    std::fs::create_dir_all(&out).unwrap();
    let mut a = app(unique_before, "Apple trees");
    refresh(&mut a, unique_after);
    assert!(
        a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED")
    );
    let output = resources::smoke_output(&a.root, &out).unwrap();
    pollster::block_on(async {
        let instance = render::instance();
        let adapter = render::adapter(&instance, None).await.unwrap();
        assert_eq!(adapter.get_info().backend, wgpu::Backend::Metal);
        let mut r = render::Renderer::new(
            &adapter,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [640, 480],
            &a.themes[0].vertices_for(&a.world),
        )
        .await
        .unwrap();
        r.set_materials(&a.themes[0].textures).unwrap();
        a.renderer = Some(r);
    });
    a.update_hud();
    a.renderer
        .as_mut()
        .unwrap()
        .capture(&output, "reader-welcome.png", &a.pose, &a.world)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyT), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyT), false, false, None)
        .unwrap();
    assert_eq!(a.theme_index, 1);
    assert!(
        a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED")
    );
    a.update_hud();
    a.renderer
        .as_mut()
        .unwrap()
        .capture(&output, "reader-workshop.png", &a.pose, &a.world)
        .unwrap();
    a.keyboard_input(Some(KeyCode::Escape), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::Escape), false, false, None)
        .unwrap();
    let document = a.themes[1]
        .host
        .anchors
        .iter()
        .find(|h| h.role == "document" && !h.id.starts_with("web-reading-"))
        .unwrap();
    a.pose = Pose {
        position: [document.at[0], document.at[2] + 1.5],
        yaw: 0.0,
        pitch: -0.1,
    };
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    assert!(a.world.reading);
    assert!(
        !a.world
            .reader_link_description()
            .contains("FULL PAGE RESTORED"),
        "an explicit fresh document choice acknowledges the recovery"
    );
    a.update_hud();
    a.renderer
        .as_mut()
        .unwrap()
        .capture(&output, "office.png", &a.pose, &a.world)
        .unwrap();
    a.web.as_mut().unwrap().shutdown();
}
