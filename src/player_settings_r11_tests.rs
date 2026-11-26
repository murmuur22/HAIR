//! R11 regressions through the production App web/menu/local-reader adapters.
use super::*;
use hair::presentation::{Item, View};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("r11-{label}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let world = World::load(&root).unwrap();
    App::new(root, vec![theme], 0, world, false)
}
fn tap(a: &mut App, code: KeyCode) {
    assert!(!a.keyboard_input(Some(code), true, false, None).unwrap());
    assert!(!a.keyboard_input(Some(code), false, false, None).unwrap());
}
fn player(a: &mut App) {
    // Q, unlike Escape, can open the paused menu over an active reader.
    tap(a, KeyCode::KeyQ);
    activate(a, Item::Settings);
    activate(a, Item::PlayerSettings);
    assert_eq!(a.menu.unwrap().view, View::PlayerSettings);
}
fn activate(a: &mut App, item: Item) {
    assert!(a.menu.unwrap().items().contains(&item));
    while a.menu.unwrap().focused() != item {
        tap(a, KeyCode::ArrowDown);
    }
    tap(a, KeyCode::Enter);
}
fn close(a: &mut App) {
    for _ in 0..3 {
        tap(a, KeyCode::Escape);
    }
    assert!(a.menu.is_none());
}
fn page(
    a: hair::web::Address,
    c: &hair::acquisition::Cancellation,
    d: Instant,
) -> Result<hair::web::Page> {
    c.check(d)?;
    hair::web::Page::html(
        a,
        b"<title>R11</title><p>Held fixture</p><a href='/next'>Next</a>",
    )
}
fn finish(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.web.as_ref().unwrap().pending.is_some() {
        a.poll_web().unwrap();
        assert!(Instant::now() < deadline, "web fixture did not finish");
        std::thread::yield_now();
    }
    assert_eq!(
        a.web_feedback(Instant::now()).phase,
        web_native::FeedbackPhase::Success
    );
}
#[test]
fn r11_actual_initial_web_keeps_saved_true_and_false_through_saves_and_recreation() {
    for visible in [true, false] {
        let dir = Fixture::new(if visible { "web-visible" } else { "web-hidden" });
        let mut seed = app();
        seed.restore_preferences_at(&dir.0).unwrap();
        player(&mut seed);
        activate(&mut seed, Item::FileTree);
        if !visible {
            activate(&mut seed, Item::FileTree);
        }
        drop(seed);
        let mut a = app();
        a.restore_preferences_at(&dir.0).unwrap();
        assert_eq!(!a.world.tree_collapsed, visible);
        // Same ordering as native --url: restore first, then real Initial worker.
        a.web = Some(web_native::Runtime::new(Vec::new(), page));
        a.request_web("https://example.org/initial", web_native::Mode::Initial)
            .unwrap();
        finish(&mut a);
        assert_eq!(
            !a.world.tree_collapsed, visible,
            "Initial replaced saved preference"
        );
        player(&mut a);
        activate(&mut a, Item::FogDistance);
        activate(&mut a, Item::Larger);
        close(&mut a);
        let expected = (a.fog, a.ui_scale);
        let mut after_save = app();
        after_save.restore_preferences_at(&dir.0).unwrap();
        assert_eq!(
            (
                after_save.fog,
                after_save.ui_scale,
                !after_save.world.tree_collapsed
            ),
            (expected.0, expected.1, visible),
            "fog/scale save immediately after Initial must preserve saved tree"
        );
        drop(after_save);
        a.request_web("https://example.org/next", web_native::Mode::Follow)
            .unwrap();
        finish(&mut a);
        assert_eq!(!a.world.tree_collapsed, visible, "Follow");
        a.request_web("https://example.org/next", web_native::Mode::Refresh)
            .unwrap();
        finish(&mut a);
        assert_eq!(!a.world.tree_collapsed, visible, "Refresh");
        // History must not restore an older visitor preference.
        player(&mut a);
        activate(&mut a, Item::FileTree);
        close(&mut a);
        a.return_web().unwrap();
        assert_eq!(!a.world.tree_collapsed, !visible, "history return");
        player(&mut a);
        activate(&mut a, Item::FileTree);
        activate(&mut a, Item::FogTint);
        close(&mut a);
        let expected_fog = a.fog;
        drop(a);
        let mut b = app();
        b.restore_preferences_at(&dir.0).unwrap();
        assert_eq!(
            !b.world.tree_collapsed, visible,
            "later fog/scale saves changed tree"
        );
        assert_eq!((b.fog, b.ui_scale), (expected_fog, expected.1));
    }
}
#[test]
fn r11_actual_local_reader_tree_toggle_clamps_tail_to_current_layout() {
    let dir = Fixture::new("reader-tree");
    let source = dir.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let bytes = format!(
        "{}TAILEND",
        "long line with several words to wrap differently at each width ".repeat(60)
    );
    std::fs::write(source.join("Long.txt"), &bytes).unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(source.clone()))).unwrap();
    let anchor = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap();
    a.pose = Pose {
        position: [anchor.at[0], anchor.at[2] + 1.05],
        yaw: 0.0,
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
        ..Default::default()
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    tap(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    assert!(a.world.document().unwrap().contains("TAILEND"));
    let document = a.world.document().unwrap().to_string();
    player(&mut a);
    activate(&mut a, Item::FileTree); // open sidebar, narrower reader
    close(&mut a);
    let [w, h] = a.ui_viewport();
    let narrow_max = hair::text::local_reader_scroll_max(&a.world, w, h);
    a.scroll_reader(i64::MAX);
    assert_eq!(a.scroll, narrow_max);
    player(&mut a);
    activate(&mut a, Item::FileTree); // close sidebar while reader stays open
    let [w, h] = a.ui_viewport();
    let wide_max = hair::text::local_reader_scroll_max(&a.world, w, h);
    assert!(
        narrow_max > wide_max && wide_max > 0,
        "fixture must rewrap at tail"
    );
    assert_eq!(
        a.scroll, wide_max,
        "tree toggle must clamp immediately, not on next scroll"
    );
    close(&mut a);
    tap(&mut a, KeyCode::ArrowUp);
    assert_eq!(a.scroll, wide_max - 1, "no stale tail backlog");
    assert_eq!(a.world.document().unwrap(), document);
    assert_eq!(
        std::fs::read_to_string(source.join("Long.txt")).unwrap(),
        bytes
    );
}
#[test]
fn r11_settings_version_and_unknown_fields_remain_rejected_without_repair() {
    let dir = Fixture::new("schema");
    let store = hair::acquisition::PreferenceFile::open(&dir.0).unwrap();
    for prefix in ["version = 2\n", "version = 1\nextra = true\n"] {
        let text = format!(
            "{prefix}ui_scale = 125\ntree_visible = true\n[fog]\nenabled = true\ndistance = 20.0\ntint = [0.1, 0.2, 0.3]\n"
        );
        store.write(text.as_bytes()).unwrap();
        let mut a = app();
        assert!(a.restore_preferences_at(&dir.0).is_err());
        assert!(a.world.tree_collapsed);
        assert_eq!(a.ui_scale, 100);
        assert_eq!(a.fog, hair::fog::Fog::default());
        assert_eq!(store.read().unwrap().unwrap(), text.as_bytes());
    }
}
