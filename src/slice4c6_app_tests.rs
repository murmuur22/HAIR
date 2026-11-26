//! App input + production overlay regressions; no image/comfort acceptance.
use super::*;
use hair::assets::Vertex;
const RED: [f32; 3] = [1., 0.2, 0.15];
const PENDING: [f32; 3] = [0.65, 0.7, 0.8];
const GREEN: [f32; 3] = [0.1, 0.9, 0.5];

fn painted(mesh: &[Vertex], size: [u32; 2], at: [f32; 2], label: &str, scale: f32, color: [f32; 3]) {
    let template = hair::placement::toolbox::paint(size, &[], &[(
        [at[0] / scale - 6., at[1] / scale - 7., at[0] / scale + 310., at[1] / scale + 13.], label,
    )], None);
    let mut expected = template[12..].to_vec();
    for v in &mut expected {
        v.position[0] = (v.position[0] + 1.) * scale - 1.;
        v.position[1] = 1. - (1. - v.position[1]) * scale;
        v.color = color;
    }
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|v| v.position[0].abs() <= 1. && v.position[1].abs() <= 1.));
    assert!(mesh.windows(expected.len()).any(|part| part.iter().zip(&expected).all(|(a, b)|
        a.color == b.color && a.position.into_iter().zip(b.position).all(|(a, b)| (a - b).abs() < 0.00001))),
        "full production label missing: {label}");
}
fn header(a: &mut App, row: usize, label: &str) {
    if a.editor_compact() { click(a,place_mode_native::Control::EditorDetails); }
    let size = a.inventory_frame().full;
    let mesh = a.place_ui();
    painted(&mesh, size, [(size[0] as f32 - 632.).max(0.) + 8., 80. + row as f32 * 10.], label, 1., [0.8, 0.9, 0.82]);
}
fn refusal_rows(a: &mut App, reason: &str) {
    let chars: Vec<_> = format!("Snapshot refused: {reason}").chars().collect();
    for (i, chunk) in chars.chunks(49).enumerate() {
        header(a, 8 + i, &chunk.iter().collect::<String>());
    }
}
fn attach(a: &mut App, size: [u32; 2]) {
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    a.renderer = Some(pollster::block_on(render::Renderer::new(&adapter,
        wgpu::TextureFormat::Rgba8UnormSrgb, size, &a.themes[0].vertices_for(&a.world))).unwrap());
}
fn footer_and_controls(a: &mut App) {
    let logical = a.inventory_frame().full;
    let footer = &a.renderer.as_ref().unwrap().pack_feedback;
    for (i, row) in footer.lines().take(2).enumerate() {
        assert!(row.chars().count() <= (logical[0] as usize - 24) / 12);
        painted(&hair::text::pack_overlay(footer, logical[0], logical[1]), logical,
            [12., logical[1] as f32 - 96. + i as f32 * 20.], row, 2., [0.85; 3]);
    }
    for (_, r, label) in a.toolbox_controls() {
        assert!(r[0] >= 0. && r[2] <= logical[0] as f32 && r[3] < logical[1] as f32 - 98.);
        painted(&a.place_ui(), logical, [r[0] + 6., r[1] + 3.], label, 1., [0.8, 0.9, 0.82]);
    }
}

#[test]
fn slice4c6_actual_preview_marks_player_trap_invalid_not_overlap_only_green() {
    let (f, mut a) = open_fixture("slice4c6-player");
    start(&mut a, [-2., 0.], true);
    a.pose.position = [-2., 0.15];
    a.pose.pitch = (-a.pose.eye_height() / 0.15).atan();
    let d = a.decoration_position().unwrap();
    assert!(hair::placement::surface::clearance(&a.themes[0].host, &d.envelope_mesh().unwrap(), hair::placement::Overlap::Prevent).is_ok());
    let before = records(&f.0);
    let mesh = a.place_ui();
    assert!(mesh.iter().any(|v| v.color == RED), "visitor enclosure is invalid, despite clear overlap");
    assert!(!mesh.iter().any(|v| v.color == GREEN));
    refusal_rows(&mut a, "host scene document/portal unreachable from spawn");
    tap(&mut a, KeyCode::Enter);
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none());
    assert!(a.local_status.contains("unreachable"), "{}", a.local_status);
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice4c6_actual_preview_checks_connected_routes_and_invalidates_host_snapshot() {
    let (_f, mut a) = open_fixture("slice4c6-route");
    start(&mut a, [0., 0.], true);
    let h = &mut a.themes[0].host;
    h.anchors.clear(); h.geometry.clear(); h.overhead.clear(); h.file_placements.clear();
    h.room = [-3., -3., 3., 3.]; h.open_platform = true; h.spawn = [0., 2.];
    h.anchors.push(hair::host_scene::Anchor { id: "route-goal".into(), role: "document".into(),
        at: [0., 0., -2.], size: [0.3, 0.5, 0.3], binding: "explicit-page".into() });
    h.geometry.extend([-2., 2.].map(|x| hair::host_scene::Geometry { at: [x, 0., 0.], size: [2., 2., 0.3] }));
    a.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap().record.size = [1.8, 0.85, 0.3];
    let d = a.decoration_position().unwrap();
    assert!(hair::placement::surface::clearance(&a.themes[0].host, &d.envelope_mesh().unwrap(), hair::placement::Overlap::Prevent).is_ok());
    assert!(a.themes[0].host.check_routes(&[a.pose.position]).is_ok());
    let mesh = a.place_ui();
    assert!(mesh.iter().any(|v| v.color == RED), "connected route is blocked without intersecting furniture");
    refusal_rows(&mut a, "host scene document/portal unreachable from spawn");
    a.themes[0].host.geometry.clear();
    let mesh = a.place_ui();
    assert!(mesh.iter().any(|v| v.color == PENDING), "host change must invalidate cached route refusal");
    assert!(!mesh.iter().any(|v| v.color == GREEN), "snapshot is not fresh admission");
    // Ordinary overlap still refuses, independently of collision-off walking.
    a.place_mode.as_mut().unwrap().decoration_edit.as_mut().unwrap().record.collision = false;
    a.themes[0].host.geometry.push(hair::host_scene::Geometry { at: [0., 0., 0.], size: [0.4, 0.5, 0.2] });
    assert!(a.place_ui().iter().any(|v| v.color == RED));
    refusal_rows(&mut a, "Placement intersects structural geometry");
}

#[test]
fn slice4c6_repeated_app_hud_redraw_retains_authoritative_refusal_and_source_status() {
    let (f, mut a) = open_fixture("slice4c6-redraw");
    start(&mut a, [-2., 0.], true);
    // Real first-review filesystem refusal, followed by an independent mesh failure.
    std::fs::write(f.0.join(".hair-layout"), b"owned malformed fixture").unwrap();
    tap(&mut a, KeyCode::Enter);
    let status = a.local_status.clone();
    a.themes[0].decorations.entries.clear();
    let before = records(&f.0);
    for _ in 0..4 {
        a.update_hud();
        assert_eq!(a.local_status, status, "redraw must not replace the authoritative review refusal");
    }
    assert_eq!(records(&f.0), before);
    // Aim at an actual file: full status must remain inspectable even in object I.
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyI);
    assert!(a.world.reading);
    assert!(a.world.document().unwrap().contains(&status), "I lost the full refusal while exiting mode");
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice4c6_frozen_effective_controls_fresh_enter_and_pointer_release_safety() {
    use place_mode_native::Control;
    let (f, mut a) = open_fixture("slice4c6-frozen");
    start(&mut a, [-2., 0.], true);
    a.input.captured = true;
    a.key(KeyCode::KeyW, true, false).unwrap();
    a.key(KeyCode::Enter, true, false).unwrap();
    let frozen = a.decoration_position().unwrap();
    assert!(!a.input.captured && !a.input.forward);
    let controls = a.toolbox_controls();
    assert!(controls.iter().all(|(c, _, _)| matches!(c, Control::Enter | Control::Cancel | Control::Exit)),
        "frozen controls must not advertise inert rotation/collision actions: {controls:?}");
    header(&mut a, 4, &format!("Yaw {:.2} | collision {} | FROZEN", frozen.yaw, frozen.collision));
    header(&mut a, 6, "FROZEN: no writes; fresh Enter saves");
    let before = records(&f.0);
    a.key(KeyCode::Enter, true, true).unwrap();
    assert_eq!(records(&f.0), before);
    a.key(KeyCode::Enter, false, false).unwrap();
    a.key(KeyCode::KeyW, false, false).unwrap();
    for key in [KeyCode::ArrowRight, KeyCode::KeyH, KeyCode::KeyB] { tap(&mut a, key); }
    assert_eq!(a.decoration_position().unwrap(), frozen);
    assert!(a.local_status.contains("fresh Enter saves") && a.local_status.contains("no writes"), "{}", a.local_status);
    assert!(!a.local_status.contains("original retained"));
    // A press dragged off a save control cannot save.
    let (_, r, _) = a.toolbox_controls().into_iter().find(|(c, _, _)| *c == Control::Enter).unwrap();
    a.pointer_moved(Some([f64::from(r[0] + 10.), f64::from(r[1] + 10.)]));
    a.place_pointer(true);
    a.pointer_moved(None);
    a.place_pointer(false);
    assert_eq!(records(&f.0), before);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    assert_eq!(layout(&a).decorations, vec![frozen]);
}

#[test]
fn slice4c6_actual_labels_fit_normal_small_viewports_and_every_scale() {
    let (f, mut a) = open_fixture("slice4c6-labels");
    attach(&mut a, [1280, 960]);
    start(&mut a, [-2., 0.], true);
    let before = records(&f.0);
    for size in [[1280, 960], [640, 480]] {
        a.renderer.as_mut().unwrap().resize(size);
        for scale in hair::presentation::STEPS {
            a.ui_scale = scale;
            a.update_hud();
            header(&mut a, 5, "Snapshot: partial current-room checks");
            header(&mut a, 6, "Pending fresh review + final validation");
            assert!(a.renderer.as_ref().unwrap().pack_feedback.starts_with("Preview: partial snapshot; no writes\nEsc back/cancel | P exit | I full status\n"));
            footer_and_controls(&mut a);
        }
    }
    tap(&mut a, KeyCode::Enter);
    for size in [[1280, 960], [640, 480]] {
        a.renderer.as_mut().unwrap().resize(size);
        for scale in hair::presentation::STEPS {
            a.ui_scale = scale;
            a.update_hud();
            header(&mut a, 6, "FROZEN: no writes; fresh Enter saves");
            assert!(a.renderer.as_ref().unwrap().pack_feedback.starts_with("Frozen: no writes; fresh Enter saves\nEsc back/cancel | P exit | I full status\n"));
            footer_and_controls(&mut a);
        }
    }
    tap(&mut a, KeyCode::Escape);
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice4c6_preview_is_neutral_partial_and_never_reads_or_repairs_layout_on_redraw() {
    let (f, mut a) = open_fixture("slice4c6-no-read");
    start(&mut a, [-2., 0.], true);
    std::fs::write(f.0.join(".hair-layout"), b"not a directory; owned read trap").unwrap();
    let before = records(&f.0);
    for _ in 0..3 {
        let mesh = a.place_ui();
        assert!(mesh.iter().any(|v| v.color == PENDING));
        assert!(!mesh.iter().any(|v| v.color == GREEN));
    }
    header(&mut a, 5, "Snapshot: partial current-room checks");
    tap(&mut a, KeyCode::Enter);
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none());
    assert_eq!(records(&f.0), before);
}

#[test]
fn slice4c6_late_commit_refusal_survives_redraw_then_source_without_retrying_save() {
    let (f, mut a) = open_fixture("slice4c6-late");
    start(&mut a, [-2., 0.], true);
    tap(&mut a, KeyCode::Enter);
    let mut rival = a.decoration_position().unwrap();
    rival.placement_id = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into();
    assert!(matches!(hair::placement_store::place_decoration_checked(
        &a.local.as_ref().unwrap().explorer, &rival, |_| Ok(())),
        hair::placement_store::Outcome::Committed { .. }));
    let before = records(&f.0);
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("overlap"), "fresh commit must see rival absent from snapshot: {}", a.local_status);
    let status = a.local_status.clone();
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().pending.is_none());
    a.themes[0].decorations.entries.clear();
    for _ in 0..4 {
        a.update_hud();
        assert_eq!(a.local_status, status);
        assert_eq!(records(&f.0), before);
    }
    a.pose.pitch = 0.8; // Directory I, as well as aimed-file I, retains the reason.
    tap(&mut a, KeyCode::KeyI);
    assert!(a.world.document().unwrap().contains(&status));
    assert_eq!(records(&f.0), before);
}
