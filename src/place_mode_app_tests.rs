use super::*;
use std::os::unix::fs::MetadataExt;
#[path = "slice3_assembly_tests.rs"]
mod slice3_assembly;
#[path = "slice3_presentation_tests.rs"]
mod slice3_presentation;
#[path = "slice4a_palette_tests.rs"]
mod slice4a_palette;
#[path = "slice4c_app_tests.rs"]
mod slice4c;
#[path = "owner_feedback_r1_tests.rs"]
mod owner_feedback_r1;
#[path = "placement_editor_r2a_tests.rs"]
mod placement_editor_r2a;

fn open_fixture(label: &str) -> (Fixture, App) {
    let f = fixture(label);
    for name in ["A.txt", "Z.txt"] {
        std::fs::write(f.0.join(name), name.as_bytes()).unwrap();
    }
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    (f, a)
}
fn aim_file(a: &mut App, name: &str) {
    let host = &a.themes[0].host;
    let id = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new(name))
        .unwrap()
        .0;
    let anchor = host.anchors.iter().find(|an| &an.id == id).unwrap();
    let (at, y) = if let Some(p) = host.file_placements.get(id) {
        (
            [p.bounds.at[0], p.bounds.at[2]],
            p.bounds.at[1] + p.bounds.size[1] * 0.5,
        )
    } else {
        (
            [anchor.at[0], anchor.at[2]],
            anchor.at[1] + (anchor.size[1] + 0.97) * 0.5,
        )
    };
    a.pose = Pose {
        position: [at[0], at[1] + 1.1],
        pitch: ((y - 1.65) / 1.1).atan(),
        ..Default::default()
    };
}
fn aim_floor(a: &mut App, at: [f32; 2]) {
    a.pose = Pose {
        position: [at[0], at[1] + 1.65],
        pitch: -std::f32::consts::FRAC_PI_4,
        ..Default::default()
    };
}
fn file_mesh(a: &App, name: &str) -> Vec<u8> {
    let id = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new(name))
        .unwrap()
        .0;
    bytemuck::cast_slice(&a.themes[0].file_item_shape(id, std::ffi::OsStr::new(name))).to_vec()
}
fn base_mesh(a: &App, name: &str) -> Vec<u8> {
    let id = a
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new(name))
        .unwrap()
        .0;
    bytemuck::cast_slice(&a.themes[0].collection_shape(id)).to_vec()
}
// Inherited positioning fixtures explicitly cross the new edit-first boundary.
// tap sends both key-down and key-up; R2A held-key tests remain independent.
fn begin_positioning(a: &mut App) {
    assert_eq!(a.editor_phase(), "Editing");
    tap(a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Positioning");
    tap(a, KeyCode::Tab);
    assert!(!a.input.captured);
}
fn reveal_control(a: &mut App, control: place_mode_native::Control) {
    // Traverse the actual disclosure/page controls, never synthesize a hidden
    // property rectangle or skip the input path at short supported heights.
    if a.editor_compact() && !a.toolbox_controls().iter().any(|(c,_,_)|*c==control) {
        click(a, place_mode_native::Control::EditorDetails);
    }
    if !a.toolbox_controls().iter().any(|(c,_,_)|*c==control) {
        while a.toolbox_controls().iter().any(|(c,_,_)|*c==place_mode_native::Control::EditorPrevious) {
            click(a,place_mode_native::Control::EditorPrevious);
        }
        for _ in 0..8 {
            if a.toolbox_controls().iter().any(|(c,_,_)|*c==control) { break; }
            if !a.toolbox_controls().iter().any(|(c,_,_)|*c==place_mode_native::Control::EditorNext) { break; }
            click(a,place_mode_native::Control::EditorNext);
        }
    }
}
fn click(a: &mut App, control: place_mode_native::Control) {
    reveal_control(a,control);
    let (_, r, _) = a
        .toolbox_controls()
        .into_iter()
        .find(|(c, _, _)| *c == control)
        .unwrap();
    let frame = a.inventory_frame();
    a.pointer_moved(Some([
        f64::from((r[0] + r[2]) * 0.5) * f64::from(frame.physical[0]) / f64::from(frame.full[0]),
        f64::from((r[1] + r[3]) * 0.5) * f64::from(frame.physical[1]) / f64::from(frame.full[1]),
    ]));
    a.place_pointer(true);
    a.place_pointer(false);
}

#[test]
fn slice2_recapture_release_rearms_toolbox_and_invalid_cursor_never_captures() {
    let (_f, mut a) = open_fixture("slice2-recapture");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    a.pointer_moved(None);
    a.place_pointer(true);
    a.place_pointer(false);
    assert!(!a.input.captured, "missing cursor cannot capture");
    a.pointer_moved(Some([300., 450.]));
    a.place_pointer(true);
    a.place_pointer(false);
    assert!(a.input.captured);
    tap(&mut a, KeyCode::Tab);
    let before = a.place_preview().unwrap();
    click(&mut a, place_mode_native::Control::Right);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&a.place_preview().unwrap()),
        "capture release must not strand the mouse latch"
    );
}

#[test]
fn slice2_saved_reposition_keeps_unrelated_saved_and_generated_objects_and_late_overlap_refuses() {
    let (f, mut a) = open_fixture("slice2-existing-saved");
    std::fs::write(f.0.join("T.txt"), b"stationary generated file").unwrap();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let p = |at| hair::placement_store::Position {
        properties: Default::default(),
        at,
        yaw: 0.4,
        size: [0.8, 0.85, 0.28],
        allow_overlap: false,
        surface: None,
    };
    let save = |a: &App, name: &str, position: &hair::placement_store::Position| {
        let local = a.local.as_ref().unwrap();
        let target =
            hair::placement_store::select_existing(&local.explorer, std::ffi::OsStr::new(name))
                .unwrap();
        let outcome = hair::placement_store::reposition_checked(
            &local.explorer,
            &target,
            position,
            |layout| {
                hair::placement_scene::validate_reposition(
                    &local.explorer,
                    layout,
                    std::ffi::OsStr::new(name),
                    position,
                    a.pose.position,
                    local.page,
                )
                .map(|_| ())
            },
        );
        assert!(
            matches!(outcome, hair::placement_store::Outcome::Committed { .. }),
            "{outcome:?}"
        );
    };
    save(&a, "Z.txt", &p([-2., 0.]));
    tap(&mut a, KeyCode::F5);
    let unrelated = file_mesh(&a, "Z.txt");
    let generated = file_mesh(&a, "T.txt");
    let base = base_mesh(&a, "T.txt");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::ArrowRight);
    tap(&mut a, KeyCode::Enter);
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("reposition saved"),
        "{}",
        a.local_status
    );
    let original = file_mesh(&a, "A.txt");
    assert_eq!(file_mesh(&a, "Z.txt"), unrelated);
    assert_eq!(file_mesh(&a, "T.txt"), generated);
    assert_eq!(base_mesh(&a, "T.txt"), base);
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [0., 0.]);
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("frozen review"),
        "{}",
        a.local_status
    );
    save(&a, "Z.txt", &p([0., 0.]));
    let before = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("original retained"),
        "{}",
        a.local_status
    );
    a.place_ui();
    assert!(
        a.local_status.contains("original retained"),
        "redraw lost failure: {}",
        a.local_status
    );
    assert_eq!(
        hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap(),
        before
    );
    tap(&mut a, KeyCode::F5);
    assert_eq!(file_mesh(&a, "A.txt"), original);
    assert_eq!(file_mesh(&a, "T.txt"), generated);
    assert_eq!(base_mesh(&a, "T.txt"), base);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(file_mesh(&reopened, "A.txt"), original);
    assert_eq!(file_mesh(&reopened, "T.txt"), generated);
    assert_eq!(base_mesh(&reopened, "T.txt"), base);
    assert_eq!(
        hair::placement_store::read_layout(&reopened.local.as_ref().unwrap().explorer).unwrap(),
        before
    );
}

#[test]
fn slice2_actual_app_overlay_draws_and_replaces_only_the_minimap_across_scales() {
    let (_f, mut a) = open_fixture("slice2-overlay");
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let mut renderer = pollster::block_on(render::Renderer::new(
        &adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [640, 480],
        &a.themes[0].vertices_for(&a.world),
    ))
    .unwrap();
    renderer.set_theme_appearance(&a.themes[0]).unwrap();
    a.renderer = Some(renderer);
    for (size, scale) in [([640, 480], 100), ([1600, 1000], 200)] {
        a.renderer.as_mut().unwrap().resize(size);
        a.set_ui_scale(scale);
        a.minimap_enabled = false;
        a.map_mode = hair::exploration::MapMode::Hidden;
        aim_file(&mut a, "A.txt");
        tap(&mut a, KeyCode::KeyP);
        tap(&mut a, KeyCode::KeyX);
        aim_floor(&mut a, [2., 0.]);
        click(&mut a, place_mode_native::Control::Right);
        a.update_hud();
        let r = a.renderer.as_ref().unwrap();
        assert!(r.directory_map.is_none() && r.inventory_overlay.is_none());
        assert!(!r.placement_overlay.is_empty() && r.collection_overlay.is_empty());
        assert!(
            r.placement_overlay
                .iter()
                .all(|v| v.position.into_iter().all(f32::is_finite))
        );
        let texture = r.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Place Mode App overlay gate"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        r.draw(
            &texture.create_view(&Default::default()),
            &a.pose,
            &a.world,
            0,
        )
        .unwrap();
        r.wait().unwrap();
        tap(&mut a, KeyCode::KeyP);
        a.update_hud();
        assert!(a.renderer.as_ref().unwrap().placement_overlay.is_empty());
        assert_eq!(
            a.renderer
                .as_ref()
                .unwrap()
                .directory_map
                .as_ref()
                .unwrap()
                .1,
            hair::exploration::MapMode::Hidden
        );
        tap(&mut a, KeyCode::KeyM);
        tap(&mut a, KeyCode::KeyP);
        a.update_hud();
        assert!(a.place_mode.is_none());
        assert_eq!(
            a.renderer
                .as_ref()
                .unwrap()
                .directory_map
                .as_ref()
                .unwrap()
                .1,
            hair::exploration::MapMode::Expanded
        );
        tap(&mut a, KeyCode::Escape);
    }
}

#[test]
fn slice2_toolbox_escape_click_recapture_never_acts_on_world_or_leaves_motion() {
    let (f, mut a) = open_fixture("slice2-toolbox");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2.0, 0.0]);
    a.input.captured = true;
    a.input.forward = true;
    a.shifts = [true; 2];
    tap(&mut a, KeyCode::Tab);
    assert!(!a.input.captured && !a.input.forward && !a.input.running);
    assert_eq!(a.shifts, [false; 2]);
    assert!(a.place_preview().is_ok(), "unlock must retain edit");
    assert!(a.menu.is_none() && !a.world.reading);
    let before = a.place_preview().unwrap();
    click(&mut a, place_mode_native::Control::Right);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&a.place_preview().unwrap())
    );
    click(&mut a, place_mode_native::Control::Overlap);
    assert!(!a.input.captured);
    a.pointer_moved(Some([300., 450.]));
    a.place_pointer(true);
    a.place_pointer(false);
    assert!(a.input.captured);
    assert!(!a.world.reading && a.inventory.items().is_empty());
    tap(&mut a, KeyCode::Escape);
    tap(&mut a, KeyCode::Escape);
    assert!(a.place_preview().is_err());
    assert!(a.place_mode.is_some() && a.menu.is_none());
    tap(&mut a, KeyCode::Escape);
    assert!(a.place_mode.is_none() && a.menu.is_none());
    tap(&mut a, KeyCode::Escape);
    assert!(a.menu.is_some());
    assert!(!f.0.join(".hair-layout").exists());
}

#[test]
fn slice2_reposition_confirms_once_retains_identity_unrelated_transforms_and_reopens() {
    let (f, mut a) = open_fixture("slice2-persist");
    let m = std::fs::metadata(f.0.join("A.txt")).unwrap();
    let identity = (m.dev(), m.ino(), m.created().unwrap());
    let unrelated = file_mesh(&a, "Z.txt");
    let base = base_mesh(&a, "Z.txt");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2.0, 0.0]);
    tap(&mut a, KeyCode::ArrowRight);
    tap(&mut a, KeyCode::KeyC);
    let mesh = a.place_preview().unwrap();
    a.keyboard_input(Some(KeyCode::Enter), true, false, None)
        .unwrap();
    assert!(!f.0.join(".hair-layout").exists(), "review cannot write");
    assert_eq!(a.editor_phase(), "Review");
    a.keyboard_input(Some(KeyCode::Enter), true, true, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::Enter), true, false, None)
        .unwrap();
    assert!(
        !f.0.join(".hair-layout").exists(),
        "held confirmation cannot commit"
    );
    let pose = a.pose;
    a.input.captured = true;
    a.keyboard_input(Some(KeyCode::KeyW), true, false, None)
        .unwrap();
    a.advance_motion(0.2);
    a.keyboard_input(Some(KeyCode::KeyW), false, false, None)
        .unwrap();
    assert_eq!(
        a.pose.position, pose.position,
        "review must freeze movement"
    );
    a.keyboard_input(Some(KeyCode::Enter), false, false, None)
        .unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(
        a.local_status.contains("reposition saved"),
        "{}",
        a.local_status
    );
    assert!(
        a.place_mode.is_some(),
        "confirmation returns to Place Mode, not ordinary X collection"
    );
    let layout = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    assert_eq!(layout.positions.len(), 1);
    assert!(layout.positions[0].1.allow_overlap);
    assert!(a.inventory.items().is_empty());
    for _ in 0..2 {
        tap(&mut a, KeyCode::Enter);
    }
    assert_eq!(
        hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap(),
        layout
    );
    let saved = file_mesh(&a, "A.txt");
    assert_eq!(saved, bytemuck::cast_slice::<_, u8>(&mesh));
    assert_eq!(file_mesh(&a, "Z.txt"), unrelated);
    assert_eq!(base_mesh(&a, "Z.txt"), base);
    tap(&mut a, KeyCode::F5);
    assert_eq!(file_mesh(&a, "A.txt"), saved);
    let mut reopened = app();
    reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(file_mesh(&reopened, "A.txt"), saved);
    assert_eq!(file_mesh(&reopened, "Z.txt"), unrelated);
    assert_eq!(base_mesh(&reopened, "Z.txt"), base);
    let m = std::fs::metadata(f.0.join("A.txt")).unwrap();
    assert_eq!((m.dev(), m.ino(), m.created().unwrap()), identity);
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"A.txt");
}

#[test]
fn slice2_cancel_focus_navigation_and_refusal_never_publish_pending_edit() {
    for code in [
        KeyCode::KeyP,
        KeyCode::KeyM,
        KeyCode::KeyU,
        KeyCode::F5,
        KeyCode::KeyI,
    ] {
        let (f, mut a) = open_fixture("slice2-cancel-nav");
        aim_file(&mut a, "A.txt");
        tap(&mut a, KeyCode::KeyP);
        tap(&mut a, KeyCode::KeyX);
        aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::Enter);
        tap(&mut a, code);
        tap(&mut a, KeyCode::Enter);
        assert!(
            !f.0.join(".hair-layout").exists(),
            "{code:?} published edit"
        );
        assert!(a.place_mode.is_none());
    }
    let (f, mut a) = open_fixture("slice2-focus");
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::Enter);
    a.focus_changed(false);
    a.focus_changed(true);
    tap(&mut a, KeyCode::Enter);
    assert!(a.place_mode.is_none() && !f.0.join(".hair-layout").exists());
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::Enter);
    std::fs::rename(f.0.join("A.txt"), f.0.join("old.txt")).unwrap();
    std::fs::write(f.0.join("A.txt"), b"replacement").unwrap();
    tap(&mut a, KeyCode::Enter);
    assert!(!f.0.join(".hair-layout").exists());
    assert!(a.local_status.contains("retained"));
    assert_eq!(std::fs::read(f.0.join("old.txt")).unwrap(), b"A.txt");
    assert_eq!(std::fs::read(f.0.join("A.txt")).unwrap(), b"replacement");
}

#[test]
fn slice2_world_p_is_fresh_context_owned_and_preserves_map_preference() {
    let mut a = app();
    a.minimap_enabled = false;
    a.map_mode = hair::exploration::MapMode::Hidden;
    a.keyboard_input(Some(KeyCode::KeyP), true, false, None)
        .unwrap();
    assert!(a.local_status.contains("Place Mode"), "{}", a.local_status);
    let status = a.local_status.clone();
    for repeat in [true, false] {
        a.keyboard_input(Some(KeyCode::KeyP), true, repeat, None)
            .unwrap();
        assert_eq!(a.local_status, status);
    }
    a.keyboard_input(Some(KeyCode::KeyP), false, false, None)
        .unwrap();
    tap(&mut a, KeyCode::KeyP);
    assert!(a.local_status.contains("exited"));
    assert!(!a.minimap_enabled);
    assert_eq!(a.map_mode, hair::exploration::MapMode::Hidden);
    for context in 0..4 {
        a.local_status = "context owns P".into();
        match context {
            0 => a.world.reading = true,
            1 => a.menu = Some(hair::presentation::Menu::settings()),
            2 => a.address = Some(String::new()),
            _ => a.chooser_pending = true,
        }
        tap(&mut a, KeyCode::KeyP);
        assert_eq!(a.local_status, "context owns P");
        a.world.reading = false;
        a.menu = None;
        a.address = None;
        a.chooser_pending = false;
    }
}

#[test]
fn slice2_place_x_selects_without_collecting_or_writing_and_b_is_not_inventory() {
    let f = fixture("slice2-select");
    std::fs::write(f.0.join("One.txt"), b"original bytes").unwrap();
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let host = a.themes[0].host.clone();
    let anchor = host.anchors.iter().find(|an| an.id == "entry-0").unwrap();
    a.pose = Pose {
        position: [anchor.at[0], anchor.at[2] + 1.1],
        pitch: ((anchor.at[1] + (anchor.size[1] + 0.97) * 0.5 - 1.65) / 1.1).atan(),
        ..Default::default()
    };
    tap(&mut a, KeyCode::KeyP);
    tap(&mut a, KeyCode::KeyX);
    assert!(
        a.inventory.items().is_empty(),
        "Place X must never acquire bytes"
    );
    assert!(a.local_status.contains("One.txt"), "{}", a.local_status);
    assert!(!f.0.join(".hair-layout").exists());
    assert_eq!(a.themes[0].host, host);
    tap(&mut a, KeyCode::KeyB);
    assert!(!a.inventory_open, "Place B belongs to decoration palette");
    assert!(a.local_status.contains("decoration"), "{}", a.local_status);
    assert_eq!(
        std::fs::read(f.0.join("One.txt")).unwrap(),
        b"original bytes"
    );
}
