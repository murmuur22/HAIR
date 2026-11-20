//! Presentation pause menu + session UI scale, exercised through the same App
//! key/focus/pointer adapters the native event loop uses.
use super::*;
use hair::presentation::{Item, View};

fn app() -> App {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["packs/office.toml", "packs/archive.toml"]
        .iter()
        .map(|name| hair::theme::Theme::load(&root.join(name), &mut cache).unwrap())
        .collect();
    App::new(
        root.to_path_buf(),
        themes,
        0,
        World::load(root).unwrap(),
        false,
    )
}
fn tap(a: &mut App, code: KeyCode) -> bool {
    let exit = a.key(code, true, false).unwrap();
    assert!(!a.key(code, false, false).unwrap(), "release never exits");
    exit
}
fn focused(a: &App) -> Item {
    a.menu.expect("menu open").focused()
}

#[test]
fn world_escape_opens_paused_menu_and_releases_cursor_instead_of_quitting() {
    let mut a = app();
    a.input.captured = true;
    a.input.forward = true;
    assert!(
        !a.key(KeyCode::Escape, true, false).unwrap(),
        "world Escape must not request exit"
    );
    assert_eq!(a.menu.map(|m| m.view), Some(View::Pause));
    assert_eq!(focused(&a), Item::Resume);
    assert!(
        !a.input.captured && !a.input.forward,
        "modal clears capture/movement"
    );
}

#[test]
fn q_opens_quit_confirmation_and_only_enter_on_quit_exits() {
    let mut a = app();
    assert!(!tap(&mut a, KeyCode::KeyQ), "Q may not bypass the menu");
    assert_eq!(focused(&a), Item::Quit);
    assert!(
        !tap(&mut a, KeyCode::KeyQ),
        "Q inside menu is not a second bypass"
    );
    assert!(a.menu.is_some());
    assert!(
        tap(&mut a, KeyCode::Enter),
        "Enter on Quit is the intentional exit"
    );
}

#[test]
fn keyboard_navigation_wraps_and_resume_leaves_cursor_uncaptured() {
    let mut a = app();
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::ArrowDown);
    assert_eq!(focused(&a), Item::Settings);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(focused(&a), Item::Quit);
    tap(&mut a, KeyCode::Tab);
    assert_eq!(focused(&a), Item::Resume, "Tab wraps");
    a.key(KeyCode::ShiftLeft, true, false).unwrap();
    tap(&mut a, KeyCode::Tab);
    assert_eq!(focused(&a), Item::Quit, "Shift+Tab reverses");
    a.key(KeyCode::ShiftLeft, false, false).unwrap();
    tap(&mut a, KeyCode::ArrowUp);
    tap(&mut a, KeyCode::ArrowUp);
    assert_eq!(focused(&a), Item::Resume);
    assert!(!tap(&mut a, KeyCode::Enter));
    assert!(a.menu.is_none(), "Resume closes");
    assert!(!a.input.captured, "Resume never recaptures without a click");
    assert!(!a.input.running && a.shifts == [false; 2]);
}

#[test]
fn settings_open_from_menu_and_escape_backs_out_one_level() {
    let mut a = app();
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::ArrowDown);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.menu.map(|m| m.view), Some(View::Settings));
    assert!(!tap(&mut a, KeyCode::Escape));
    assert_eq!(
        a.menu.map(|m| m.view),
        Some(View::Pause),
        "Escape backs out"
    );
    assert_eq!(
        focused(&a),
        Item::Settings,
        "back retains the Settings focus"
    );
    assert!(!tap(&mut a, KeyCode::Escape));
    assert!(a.menu.is_none(), "second Escape resumes, never exits");
}

#[test]
fn repeated_or_held_escape_cannot_chain_menu_transitions() {
    let mut a = app();
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
    for _ in 0..4 {
        assert!(!a.key(KeyCode::Escape, true, true).unwrap());
    }
    assert!(
        a.menu.is_some(),
        "auto-repeat must not close the menu it opened"
    );
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(!a.key(KeyCode::Escape, true, false).unwrap());
    assert!(a.menu.is_none());
    for _ in 0..4 {
        assert!(!a.key(KeyCode::Escape, true, true).unwrap());
    }
    assert!(a.menu.is_none(), "held Escape after resume may not reopen");
    // A duplicate non-repeat press without release is also inert.
    a.key(KeyCode::Escape, false, false).unwrap();
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_some(), "a fresh press after release reopens");
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.menu.is_none(), "Enter on Resume");
    assert!(!a.key(KeyCode::Enter, true, false).unwrap());
    assert!(
        a.menu.is_none(),
        "Enter held across Resume may not act again"
    );
}

#[test]
fn menu_isolates_world_theme_navigation_url_and_loading_hotkeys() {
    let mut a = app();
    a.pose = Pose {
        position: [0.0, -3.0],
        yaw: 0.0,
        pitch: -0.1,
    };
    tap(&mut a, KeyCode::Escape);
    let before = (
        a.world.current.clone(),
        a.world.reading,
        a.world.tree_collapsed,
        a.theme_index,
        a.pose.position,
        a.world.explicit_target.clone(),
    );
    for code in [
        KeyCode::KeyW,
        KeyCode::KeyE,
        KeyCode::KeyT,
        KeyCode::KeyR,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyO,
        KeyCode::F5,
        KeyCode::F8,
    ] {
        assert!(!a.key(code, true, false).unwrap());
    }
    assert!(!a.input.forward, "movement leaked behind menu");
    assert!(a.address.is_none(), "URL entry leaked behind menu");
    assert!(!a.chooser_pending, "folder chooser leaked behind menu");
    assert!(!a.web_details);
    assert_eq!(
        before,
        (
            a.world.current.clone(),
            a.world.reading,
            a.world.tree_collapsed,
            a.theme_index,
            a.pose.position,
            a.world.explicit_target.clone(),
        )
    );
    // W and E are still physically held across Resume: repeats stay inert.
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::Escape, false, false).unwrap();
    assert!(a.menu.is_none());
    a.key(KeyCode::KeyW, true, true).unwrap();
    a.key(KeyCode::KeyE, true, true).unwrap();
    assert!(
        !a.input.forward,
        "held W resumed movement without a fresh press"
    );
    assert_eq!(a.world.current, "welcome");
    a.key(KeyCode::KeyW, false, false).unwrap();
    a.key(KeyCode::KeyE, false, false).unwrap();
    a.key(KeyCode::KeyW, true, false).unwrap();
    assert!(a.input.forward, "a fresh press after release moves again");
    a.key(KeyCode::KeyE, true, false).unwrap();
    assert_eq!(a.world.current, "workshop", "a fresh E press re-arms");
}

#[test]
fn focus_loss_keeps_menu_paused_and_clears_its_held_keys() {
    let mut a = app();
    a.key(KeyCode::Escape, true, false).unwrap();
    a.key(KeyCode::ArrowDown, true, false).unwrap();
    a.focus_changed(false);
    assert!(a.menu.is_some(), "focus loss keeps the safe paused state");
    assert!(!a.key(KeyCode::Enter, true, false).unwrap());
    assert_eq!(
        a.menu.map(|m| m.view),
        Some(View::Pause),
        "unfocused input inert"
    );
    a.key(KeyCode::Enter, false, false).unwrap();
    a.focus_changed(true);
    // Releases may have been lost while unfocused; a fresh press must work.
    a.key(KeyCode::Escape, true, false).unwrap();
    assert!(a.menu.is_none());
}

#[test]
fn address_then_reader_take_escape_before_the_world_menu() {
    let mut a = app();
    tap(&mut a, KeyCode::KeyL);
    assert!(a.address.is_some());
    assert!(!tap(&mut a, KeyCode::Escape));
    assert!(
        a.address.is_none() && a.menu.is_none(),
        "Escape closed address only"
    );
    a.pose = Pose {
        position: [0.0, 1.8],
        yaw: 0.0,
        pitch: -0.5,
    };
    tap(&mut a, KeyCode::KeyE);
    assert!(a.world.reading);
    assert!(!tap(&mut a, KeyCode::Escape));
    assert!(
        !a.world.reading && a.menu.is_none(),
        "Escape closed reader only"
    );
    assert!(!tap(&mut a, KeyCode::Escape));
    assert!(a.menu.is_some(), "then world Escape opens the menu");
}

/// Physical pixel bounds of each rendered button quad, in draw order.
fn rendered_buttons(
    menu: &hair::presentation::Menu,
    percent: u16,
    size: [u32; 2],
) -> Vec<[f64; 4]> {
    use hair::presentation::{BUTTON_COLOR, FOCUS_COLOR};
    let ui = hair::presentation::overlay(menu, percent, size);
    assert!(
        ui.iter().all(|v| v
            .position
            .iter()
            .all(|p| p.is_finite() && (-1.0..=1.0).contains(p))),
        "menu clipped at {size:?} {percent}%"
    );
    ui.chunks(6)
        .filter(|q| q.len() == 6 && (q[0].color == BUTTON_COLOR || q[0].color == FOCUS_COLOR))
        .map(|q| {
            let xs = q
                .iter()
                .map(|v| (v.position[0] as f64 + 1.0) * 0.5 * size[0] as f64);
            let ys = q
                .iter()
                .map(|v| (1.0 - v.position[1] as f64) * 0.5 * size[1] as f64);
            [
                xs.clone().fold(f64::MAX, f64::min),
                ys.clone().fold(f64::MAX, f64::min),
                xs.fold(f64::MIN, f64::max),
                ys.fold(f64::MIN, f64::max),
            ]
        })
        .collect()
}

#[test]
fn pointer_hits_match_rendered_buttons_across_scale_resize_and_hidpi() {
    use hair::presentation::{Menu, STEPS, hit};
    for size in [
        [960, 640],
        [640, 480],
        [1920, 1280],
        [2560, 1440],
        [1280, 720],
    ] {
        for percent in STEPS {
            for menu in [Menu::pause(), Menu::quit_confirm(), Menu::settings()] {
                let rects = rendered_buttons(&menu, percent, size);
                assert_eq!(rects.len(), menu.items().len(), "{size:?} {percent}%");
                for (item, [x0, y0, x1, y1]) in menu.items().iter().zip(&rects) {
                    assert!(y1 - y0 >= 20.0 * 0.99, "button too small to click");
                    let centre = [(x0 + x1) * 0.5, (y0 + y1) * 0.5];
                    assert_eq!(hit(&menu, size, percent, centre), Some(*item));
                    // Just inside the rendered edges also hits; just outside does not.
                    assert_eq!(hit(&menu, size, percent, [x0 + 1.0, y0 + 1.0]), Some(*item));
                    assert_ne!(
                        hit(&menu, size, percent, [x0 - 2.0, centre[1]]),
                        Some(*item)
                    );
                }
                // Larger requested scale renders physically larger buttons.
                if percent == 200 && size == [1920, 1280] {
                    let base = rendered_buttons(&menu, 100, size);
                    assert!(rects[0][3] - rects[0][1] > 1.9 * (base[0][3] - base[0][1]));
                }
            }
        }
    }
    let menu = Menu::pause();
    assert_eq!(
        hit(&menu, [960, 640], 100, [2.0, 2.0]),
        None,
        "outside buttons"
    );
}

#[test]
fn clicking_rendered_buttons_drives_the_app_and_never_captures() {
    let mut a = app();
    tap(&mut a, KeyCode::Escape);
    let centre = |a: &App, item: Item| {
        let menu = a.menu.unwrap();
        let i = menu.items().iter().position(|i| *i == item).unwrap();
        let [x0, y0, x1, y1] = rendered_buttons(&menu, a.ui_scale, [960, 640])[i];
        [(x0 + x1) * 0.5, (y0 + y1) * 0.5]
    };
    assert!(!a.menu_click([1.0, 1.0]), "background click inert");
    assert!(a.menu.is_some() && !a.input.captured);
    assert!(!a.menu_click(centre(&a, Item::Settings)));
    assert_eq!(a.menu.map(|m| m.view), Some(View::Settings));
    assert!(!a.menu_click(centre(&a, Item::Larger)));
    assert_eq!(a.ui_scale, 125, "click applies immediately");
    // Rendered layout changes with scale; the click must follow the new rects.
    assert!(!a.menu_click(centre(&a, Item::Larger)));
    assert_eq!(a.ui_scale, 150);
    assert!(!a.menu_click(centre(&a, Item::Reset)));
    assert_eq!(a.ui_scale, 100);
    assert!(!a.menu_click(centre(&a, Item::Back)));
    assert!(!a.menu_click(centre(&a, Item::Resume)));
    assert!(
        a.menu.is_none() && !a.input.captured,
        "Resume click does not capture"
    );
    tap(&mut a, KeyCode::KeyQ);
    assert!(a.menu_click(centre(&a, Item::Quit)), "Quit click exits");
}

#[test]
fn small_window_at_max_scale_shows_capped_label_and_keeps_recovery_reachable() {
    use hair::presentation::{Menu, scale_label};
    let label = scale_label(200, [640, 480]);
    assert!(label.contains("200%") && label.contains("WINDOW LIMIT") && label.contains("100%"));
    let label = scale_label(150, [1920, 1280]);
    assert!(label.contains("150%") && !label.contains("WINDOW LIMIT"));
    let partial = scale_label(200, [1280, 720]);
    assert!(
        partial.contains("WINDOW LIMIT") && partial.contains("144%"),
        "{partial}"
    );
    let settings = rendered_buttons(&Menu::settings(), 200, [640, 480]);
    assert_eq!(settings.len(), 4, "Smaller/Larger/Reset/Back all reachable");
}

#[test]
fn reader_metrics_reflow_in_the_logical_viewport_at_every_scale() {
    let mut world = hair::web::Page::html(
        hair::web::Address::parse("https://example.org/long").unwrap(),
        "<p>line of readable presentation text</p>"
            .repeat(120)
            .as_bytes(),
    )
    .unwrap()
    .generate()
    .unwrap()
    .world;
    world.reading = true;
    for physical in [[640, 480], [960, 640], [1920, 1280]] {
        let mut previous = 0;
        for percent in hair::presentation::STEPS {
            let [w, h] = hair::presentation::ui_viewport(physical, percent);
            let max = hair::text::local_reader_scroll_max(&world, w, h);
            assert!(
                max >= previous,
                "larger text needs at least as many scroll rows"
            );
            previous = max;
            for scroll in [0, max / 2, max] {
                let ui = hair::text::overlay(&world, w, h, scroll);
                assert!(!ui.is_empty());
                assert!(
                    ui.iter()
                        .all(|v| v.position.iter().all(|p| (-1.0..=1.0).contains(p))),
                    "reader clipped at {physical:?} {percent}% scroll {scroll}"
                );
            }
        }
    }
    let mut a = app();
    a.ui_scale = 200;
    assert_eq!(
        a.ui_viewport(),
        hair::presentation::ui_viewport([960, 640], 200),
        "App scroll metrics use the same logical viewport as the renderer"
    );
    assert_ne!(a.ui_viewport(), [960, 640]);
}

#[test]
#[ignore = "serialized offscreen Metal; requires HAIR_PRESENTATION_CAPTURE_DIR"]
fn rendered_menu_scale_survives_real_theme_switch_and_captures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = std::env::var_os("HAIR_PRESENTATION_CAPTURE_DIR")
        .expect("explicit separate existing capture directory");
    // Validate the explicit capture root, then use a fresh child per state.
    // SmokeOutput deliberately permits only its three fixed capture filenames.
    resources::smoke_output(root, Path::new(&path)).unwrap();
    let mut a = app();
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [1920, 1280],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(renderer);
    open_settings(&mut a);
    for _ in 0..4 {
        activate(&mut a, Item::Larger);
    }
    a.update_hud();
    let capture = |a: &App, name: &str| {
        let dir = Path::new(&path).join(name.trim_end_matches(".png"));
        std::fs::create_dir(&dir).unwrap();
        let output = resources::smoke_output(root, &dir).unwrap();
        a.renderer
            .as_ref()
            .unwrap()
            .capture(&output, "office.png", &a.pose, &a.world)
            .unwrap();
    };
    capture(&a, "settings-200.png");
    tap(&mut a, KeyCode::Escape);
    capture(&a, "pause-200-stale-until-update.png");
    a.update_hud();
    capture(&a, "pause-200.png");
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::KeyT);
    assert_eq!(a.theme_index, 1, "real T switch with a renderer");
    a.update_hud();
    assert_eq!(a.renderer.as_ref().unwrap().ui_scale, 200);
    assert_eq!(a.renderer.as_ref().unwrap().ui_viewport(), [960, 640]);
    capture(&a, "world-archive-200.png");
    // Keep the strict scene-diversity capture guard: the tiny archive view is
    // legitimately only17 colors. Inspect the small menu over the richer office.
    tap(&mut a, KeyCode::KeyT);
    assert_eq!(a.theme_index, 0);
    a.renderer.as_mut().unwrap().resize([640, 480]);
    tap(&mut a, KeyCode::Escape);
    open_settings_from_pause(&mut a);
    a.update_hud();
    capture(&a, "settings-640x480-capped.png");
    a.renderer.as_ref().unwrap().wait().unwrap();
}
fn open_settings_from_pause(a: &mut App) {
    tap(a, KeyCode::ArrowDown);
    tap(a, KeyCode::Enter);
    assert_eq!(a.menu.map(|m| m.view), Some(View::Settings));
}

fn open_settings(a: &mut App) {
    tap(a, KeyCode::Escape);
    tap(a, KeyCode::ArrowDown);
    tap(a, KeyCode::Enter);
    assert_eq!(a.menu.map(|m| m.view), Some(View::Settings));
}
fn activate(a: &mut App, item: Item) {
    while focused(a) != item {
        tap(a, KeyCode::ArrowDown);
    }
    assert!(!tap(a, KeyCode::Enter));
}

#[test]
fn settings_scale_steps_are_bounded_immediate_and_resettable() {
    let mut a = app();
    assert_eq!(a.ui_scale, 100, "baseline stays 100%");
    open_settings(&mut a);
    for expected in [125, 150, 175, 200, 200, 200] {
        activate(&mut a, Item::Larger);
        assert_eq!(
            a.ui_scale, expected,
            "Larger applies immediately, capped at 200%"
        );
    }
    for expected in [175, 150, 125, 100, 100] {
        activate(&mut a, Item::Smaller);
        assert_eq!(a.ui_scale, expected, "Smaller never goes below 100%");
    }
    tap(&mut a, KeyCode::ArrowRight);
    tap(&mut a, KeyCode::ArrowRight);
    assert_eq!(a.ui_scale, 150, "Right also steps in Settings");
    tap(&mut a, KeyCode::ArrowLeft);
    assert_eq!(a.ui_scale, 125);
    a.key(KeyCode::ArrowRight, true, false).unwrap();
    a.key(KeyCode::ArrowRight, true, true).unwrap();
    a.key(KeyCode::ArrowRight, false, false).unwrap();
    assert_eq!(a.ui_scale, 150, "auto-repeat cannot chain steps");
    activate(&mut a, Item::Reset);
    assert_eq!(a.ui_scale, 100);
    activate(&mut a, Item::Back);
    assert_eq!(a.menu.map(|m| m.view), Some(View::Pause));
    assert_eq!(
        hair::presentation::step_scale(90, false),
        100,
        "unknown values clamp"
    );
    assert_eq!(hair::presentation::step_scale(500, true), 200);
}

#[test]
fn scale_is_session_wide_across_theme_world_navigation_and_focus() {
    let mut a = app();
    open_settings(&mut a);
    activate(&mut a, Item::Larger);
    activate(&mut a, Item::Larger);
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_none());
    assert_eq!(a.ui_scale, 150);
    // T only switches with a GPU renderer (switch_theme); renderer-held scale
    // across a real T is covered by the GPU-gated renderer test below.
    tap(&mut a, KeyCode::KeyT);
    a.pose = Pose {
        position: [0.0, -3.0],
        yaw: 0.0,
        pitch: -0.1,
    };
    tap(&mut a, KeyCode::KeyE);
    assert_eq!(a.world.current, "workshop");
    a.focus_changed(false);
    a.focus_changed(true);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, "welcome");
    assert_eq!(
        a.ui_scale, 150,
        "theme/world/focus changes keep the session scale"
    );
}

#[test]
fn applied_scale_keeps_baseline_caps_small_windows_and_is_monotonic() {
    use hair::presentation::{STEPS, applied, ui_viewport};
    for size in [[960, 640], [640, 480], [1920, 1280], [3840, 2160]] {
        assert_eq!(
            ui_viewport(size, 100),
            size,
            "100% is the unchanged baseline"
        );
        assert_eq!(applied(100, size), 1.0);
    }
    assert_eq!(
        ui_viewport([1920, 1280], 200),
        [960, 640],
        "Retina 960x640 at 200%"
    );
    assert_eq!(applied(200, [1920, 1080]), 2.0);
    // 640x480 at max: safe capped layout (nothing below the tested 640x480 HUD).
    assert_eq!(applied(200, [640, 480]), 1.0);
    assert_eq!(ui_viewport([640, 480], 200), [640, 480]);
    for size in [
        [640, 480],
        [960, 640],
        [1280, 720],
        [1280, 960],
        [1600, 1000],
        [2560, 1440],
    ] {
        let mut previous = 0.0;
        for step in STEPS {
            let scale = applied(step, size);
            let [w, h] = ui_viewport(size, step);
            assert!(scale >= previous, "{size:?} not monotonic at {step}%");
            assert!(scale >= 1.0 && scale <= step as f32 / 100.0);
            assert!(
                scale == 1.0 || (w >= 800 && h >= 500),
                "{size:?} {step}% shrank logical HUD below 800x500: {w}x{h}"
            );
            previous = scale;
        }
    }
    assert!(applied(200, [1280, 720]) > 1.0, "mid windows still enlarge");
}
