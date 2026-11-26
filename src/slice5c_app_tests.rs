//! Explicit ignored 5C offscreen Metal witness. No native surface or human acceptance.
//! Positive shots NEVER upload a scene assembled by this harness. Actual X/cancel/save
//! upload via App; subsequent frame consumers use App::place_scene_refresh, including
//! the normal interaction-change branch. Only deliberately negative controls set_scene.
use super::*;
use hair::placement_store::Decoration;

fn frame(a: &mut App) {
    // Same production consumers/order as App::redraw before its native surface acquire.
    // App::redraw itself requires ActiveEventLoop + surface; neither is fabricated here.
    a.poll_web().unwrap();
    a.poll_local_image().unwrap();
    a.advance_motion(0.);
    let old = a.world.interaction.clone();
    hair::interaction::refresh_with_windows(&a.themes[a.theme_index].host, &a.pose,
        &mut a.world, a.web.as_ref().map_or(&[], |w| w.window_doors.as_slice()));
    let animated = a.focused && a.place_context()
        && a.themes[a.theme_index].has_file_animation(&a.world);
    if old != a.world.interaction || animated { a.place_scene_refresh(); }
    a.update_hud();
}
fn capture(a: &mut App, output: &Path, label: &str, backend: &str) -> Vec<u8> {
    frame(a);
    // This scene is receipt/oracle only. shot() does NOT call set_scene. In particular
    // do not use Theme::vertices_for(_at) here: that would misdescribe the actual X scene.
    let actual_path = a.place_scene_at(0.);
    let pixels = shot(a, output, label, backend, &actual_path);
    journal(a, output, label, &actual_path);
    pixels
}
fn settle_fixture(a: &mut App) {
    // Text previews finish asynchronously and remap texture keys. Pin only the
    // completed real fixture, never placeholder bytes; do not disable the worker.
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        frame(a);
        if a.object_previews.test_model_fallback_progress().1 { break; }
        assert!(Instant::now() < deadline, "owned fixture preview completion deadline");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
fn journal(a: &App, output: &Path, label: &str, scene: &[Vertex]) {
    let expected = a.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref())
        .and_then(|e| e.expected());
    let data = &a.local.as_ref().unwrap().explorer.current().folder;
    let pinned = records(data).iter().map(|(p, (ino, bytes))|
        format!("{} inode={ino} sha256={} bytes={}\n", p.display(), hash(bytes), bytes.len()))
        .collect::<String>();
    let host = &a.themes[a.theme_index].host;
    let old_authority = expected.map(|d| {
        let ray = Pose { position: [d.at[0], d.at[1] + 1.1], yaw: 0.,
            pitch: ((d.size[1] * 0.5 - a.pose.eye_height()) / 1.1).atan(), ..a.pose };
        format!("old_ray_pose={ray:?}\nold_ray_actual_target={:#?}\nold_player_blocked={}\nold_host_blocked={}\nold_mesh_sha256={}\n",
            hair::interaction::decoration_target(host, &ray, &a.world),
            hair::player::blocked_at(host, d.at, 0.), host.blocked(d.at),
            hash(mesh_bytes(&a.themes[a.theme_index].decoration_preview(d).unwrap())))
    }).unwrap_or_default();
    let state = format!("source_mode={}\nexpected={expected:#?}\nproposal={:#?}\npose={:?}\nhost={host:#?}\nactual_target={:#?}\nscene_sha256={}\nA_mesh_sha256={}\nZ_mesh_sha256={}\nrecords:\n{pinned}",
        if expected.is_some() { "Existing(expected)" } else { "no existing edit" },
        a.decoration_position().ok(), a.pose,
        hair::interaction::decoration_target(host, &a.pose, &a.world),
        hash(mesh_bytes(scene)), hash(&file_mesh(a, "A.txt")), hash(&file_mesh(a, "Z.txt")));
    std::fs::write(output.join(format!("{label}-state.txt")), format!("{state}\n{old_authority}")).unwrap();
}
fn select(a: &mut App, old: &Decoration) {
    if a.place_mode.is_none() { tap(a, KeyCode::KeyP); }
    a.pose = Pose { position: [old.at[0], old.at[1] + 1.1], yaw: 0.,
        pitch: ((old.size[1] * 0.5 - a.pose.eye_height()) / 1.1).atan(), ..a.pose };
    frame(a);
    assert_eq!(hair::interaction::decoration_target(&a.themes[0].host, &a.pose, &a.world), Some(old));
    tap(a, KeyCode::KeyX);
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().expected(), Some(old));
    assert!(a.local_status.contains("reposition") && a.local_status.contains(&old.placement_id));
    assert!(a.local_status.contains("Existing Follow theme") && a.local_status.contains("conversion not supported"));
    assert!(!a.toolbox_controls().iter().any(|(c, _, _)| matches!(c,
        place_mode_native::Control::FollowTheme | place_mode_native::Control::PinAsset)));
}
fn view_proposal(a: &mut App) {
    // Synthetic aim, not writer setup: floor [-1, 0.15], all three old/other/proposed
    // footprints in view, independent of toolbox/HUD. Same camera for before/cancel.
    a.pose = Pose { position: [-1., 2.8], yaw: 0., pitch: (-1.65_f32 / 2.65).atan(), ..Default::default() };
}
fn check_disclosures(a: &mut App, old: &Decoration, frozen: bool) {
    let ui = a.place_ui();
    header_label(a, &ui, 0, "REPOSITION DECORATION | FLOOR ONLY");
    header_label(a, &ui, 2, "Existing Follow theme (save tag only)");
    header_label(a, &ui, 3, "Relocate only; conversion not supported");
    header_label(a, &ui, 5, PARTIAL_LABEL);
    header_label(a, &ui, 6, if frozen { FROZEN_LABEL } else { PENDING_LABEL });
    header_label(a, &ui, 7, "I full status (cancels) | Esc cancel | P exit");
    header_label(a, &ui, 8, &format!("Expected ID: {}", old.placement_id));
    if frozen {
        assert_eq!(a.toolbox_controls().iter().map(|(_, _, s)| *s).collect::<Vec<_>>(),
            ["Enter: save reposition", "Esc: cancel / exit", "P: exit Place Mode"]);
        assert!(!a.input.captured);
    }
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().preview_reason.is_none());
}
// Explicit independent saved-state witnesses keep every existing invariant visible.
#[allow(clippy::too_many_arguments)]
fn check_pinned(a: &App, old: &Decoration, saved: &hair::placement_store::LayoutRead,
    host: &hair::host_scene::HostScene, data: &Path,
    pinned: &std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)>, file: &[u8], other_file: &[u8]) {
    invariant(a, saved, host, data, pinned);
    assert_eq!(file_mesh(a, "A.txt"), file);
    assert_eq!(file_mesh(a, "Z.txt"), other_file);
    assert!(hair::player::blocked_at(&a.themes[0].host, old.at, 0.));
    assert!(a.themes[0].host.blocked(old.at));
    // Use a separate pose rather than moving the preview proposal: target authority
    // must still resolve old saved record while its appearance is omitted.
    let pose = Pose { position: [old.at[0], old.at[1] + 1.1], yaw: 0.,
        pitch: ((old.size[1] * 0.5 - a.pose.eye_height()) / 1.1).atan(), ..a.pose };
    assert_eq!(hair::interaction::decoration_target(&a.themes[0].host, &pose, &a.world), Some(old));
}
fn preview_negatives(a: &mut App, output: &Path, phase: &str, backend: &str,
    old: &Decoration, other: &Decoration, positive: &[u8]) {
    let scene = a.place_scene_at(0.);
    let full = a.themes[0].vertices_for(&a.world);
    let old_mesh = a.themes[0].decoration_preview(old).unwrap();
    let other_mesh = a.themes[0].decoration_preview(other).unwrap();
    assert_eq!(mesh_bytes(&scene), mesh_bytes(&substitute(&full, &old_mesh, &[])),
        "production omission must remove only exact expected instance");
    assert!(scene.windows(other_mesh.len()).any(|m| mesh_bytes(m) == mesh_bytes(&other_mesh)));
    // Existing unchanged helper checks exact real authored proposal projection and
    // renders both missing-proposal and wrong-authored-proposal controls (>100 ROI).
    preview_controls(a, output, phase, backend, &scene, positive, PENDING);
    for (name, wrong) in [
        ("old-retained", full.clone()),
        ("wrong-old-omission", substitute(&full, &other_mesh, &[])),
    ] {
        // Negative only. Keep actual overlay/pose/world/state constant.
        a.renderer.as_mut().unwrap().set_scene(&wrong);
        let negative = shot(a, output, &format!("{phase}-{name}-control"), backend, &wrong);
        let changed = changed_world_pixels(positive, &negative);
        println!("SLICE5C_CONTROL phase={phase} kind={name} world_roi_changed_pixels={changed}");
        std::fs::write(output.join(format!("{phase}-{name}-control-roi.txt")),
            format!("world_roi_changed_pixels={changed}\nthreshold=100\npositive_scene_sha256={}\nnegative_scene_sha256={}\n",
                hash(mesh_bytes(&scene)), hash(mesh_bytes(&wrong)))).unwrap();
        assert!(changed > 100, "wrong omission must materially alter world ROI, not HUD");
    }
    // Restore only by actual production path, not harness-built omission.
    a.place_scene_refresh();
    a.update_hud();
}

#[test]
#[ignore = "explicit serialized Metal; fresh HAIR_SLICE5C_OUTPUT and HAIR_SLICE5C_FIXTURES"]
fn slice5c_actual_x_metal_reposition_preview_review_save_reopen_and_cancel() {
    let output = PathBuf::from(std::env::var_os("HAIR_SLICE5C_OUTPUT").expect("unique capture directory"));
    let fixtures = PathBuf::from(std::env::var_os("HAIR_SLICE5C_FIXTURES").expect("unique retained owned fixtures"));
    let mut a = app();
    assert!(output.is_absolute() && fixtures.is_absolute());
    assert_eq!(output.parent(), a.root.parent());
    assert_eq!(fixtures.parent().unwrap().canonicalize().unwrap(), std::env::temp_dir().canonicalize().unwrap());
    assert!(!output.starts_with(&a.root) && !fixtures.starts_with(&a.root));
    std::fs::create_dir(&output).unwrap();
    std::fs::create_dir(&fixtures).unwrap();
    let data = fixtures.join("owned-folder");
    std::fs::create_dir(&data).unwrap();
    for name in ["A.txt", "Z.txt"] { std::fs::write(data.join(name), name.as_bytes()).unwrap(); }
    a.complete_folder(Ok(Some(data.clone()))).unwrap();
    let original = records(&data);
    let backend = attach(&mut a); // Attached before any create/select/save actions.
    std::fs::write(output.join("fixture-provenance.txt"),
        "Owned synthetic two-file folder. All two-decoration and saved-file setup via actual App keys/pointer/create/review/save actions, no writer-only setup. Authentic bundled office chair/desk, no resource changes. Synthetic aim assignments, not physical input. Production frame consumers + actual App scene path, no native App::redraw/surface. Positive shot never set_scene. Negative GPU-only controls deliberately remove/replace proposal or retain old/remove unrelated appearance. No native/human comfort or visual acceptance. Parent inspection pending; package_ready=false.\n").unwrap();
    // Include an independently saved file plus an untouched generated file.
    aim_file(&mut a, "A.txt");
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    aim_floor(&mut a, [2., 0.]); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    for (choice, at) in [(0, [-2., 0.]), (1, [0., -1.])] {
        if a.place_mode.is_none() { tap(&mut a, KeyCode::KeyP); }
        tap(&mut a, KeyCode::KeyB);
        for _ in 0..2 { tap(&mut a, KeyCode::ArrowUp); }
        for _ in 0..choice { tap(&mut a, KeyCode::ArrowDown); }
        tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Escape);
        click(&mut a, place_mode_native::Control::PlaceDecoration);
        aim_floor(&mut a, at);
        if !a.decoration_position().unwrap().collision { tap(&mut a, KeyCode::KeyH); }
        tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    }
    settle_fixture(&mut a);
    let saved = layout(&a);
    assert_eq!(saved.decorations.len(), 2);
    let old = saved.decorations.iter().find(|d| d.tag == "hair.seating.chair").unwrap().clone();
    let other = saved.decorations.iter().find(|d| d.tag == "hair.furniture.desk").unwrap().clone();
    let pinned = records(&data);
    let host = a.themes[0].host.clone();
    let file = file_mesh(&a, "A.txt"); let other_file = file_mesh(&a, "Z.txt");
    for (p, value) in &original { assert_eq!(pinned.get(p), Some(value)); }
    std::fs::write(output.join("original-metadata.txt"), format!("layout={saved:#?}\nrecords={pinned:#?}\n")).unwrap();
    tap(&mut a, KeyCode::KeyP);
    view_proposal(&mut a);
    capture(&mut a, &output, "before-selection", &backend);

    // Independent cancel witness BEFORE the successful relocation.
    select(&mut a, &old); view_proposal(&mut a);
    click(&mut a, place_mode_native::Control::Right); tap(&mut a, KeyCode::KeyH);
    let proposed = a.decoration_position().unwrap();
    assert_eq!(proposed.placement_id, old.placement_id);
    assert_eq!(proposed.tag, old.tag); assert_eq!(proposed.size, old.size);
    assert_ne!(proposed.at, old.at); assert_ne!(proposed.yaw, old.yaw); assert!(!proposed.collision);
    let cancel_preview = capture(&mut a, &output, "cancel-preview", &backend);
    check_disclosures(&mut a, &old, false);
    check_pinned(&a, &old, &saved, &host, &data, &pinned, &file, &other_file);
    preview_negatives(&mut a, &output, "cancel-preview", &backend, &old, &other, &cancel_preview);
    tap(&mut a, KeyCode::Escape);
    assert!(a.place_mode.as_ref().is_none_or(|m| m.decoration_edit.is_none()));
    view_proposal(&mut a);
    let cancelled = capture(&mut a, &output, "cancel-restored-original", &backend);
    check_pinned(&a, &old, &saved, &host, &data, &pinned, &file, &other_file);
    assert_eq!(mesh_bytes(&a.place_scene_at(0.)), mesh_bytes(&a.themes[0].vertices_for(&a.world)));
    // Controls for restored old mesh independently of changed overlay/HUD.
    controls(&mut a, &output, "cancel-restored", &backend, &cancelled);

    select(&mut a, &old); view_proposal(&mut a);
    click(&mut a, place_mode_native::Control::Right); tap(&mut a, KeyCode::KeyH);
    assert_eq!(a.decoration_position().unwrap(), proposed);
    let preview = capture(&mut a, &output, "reposition-preview", &backend);
    check_disclosures(&mut a, &old, false);
    preview_negatives(&mut a, &output, "reposition-preview", &backend, &old, &other, &preview);
    check_pinned(&a, &old, &saved, &host, &data, &pinned, &file, &other_file);
    // Hold first Enter, including duplicate non-repeat and repeat keydown: no save.
    a.key(KeyCode::Enter, true, false).unwrap();
    assert!(a.local_status.contains("frozen decoration reposition review"), "{}", a.local_status);
    for repeat in [false, true] { a.key(KeyCode::Enter, true, repeat).unwrap(); }
    for key in [KeyCode::ArrowLeft, KeyCode::KeyH, KeyCode::KeyB] { tap(&mut a, key); }
    assert_eq!(a.decoration_position().unwrap(), proposed);
    let frozen = capture(&mut a, &output, "frozen-reposition-review", &backend);
    check_disclosures(&mut a, &old, true);
    check_pinned(&a, &old, &saved, &host, &data, &pinned, &file, &other_file);
    preview_negatives(&mut a, &output, "frozen-review", &backend, &old, &other, &frozen);
    a.key(KeyCode::Enter, false, false).unwrap();
    tap(&mut a, KeyCode::Enter); // Fresh second press/release, real checked relocate.
    assert!(a.local_status.contains("decoration reposition saved"), "{}", a.local_status);
    settle_fixture(&mut a);
    let after = layout(&a);
    assert_eq!(after.decorations.len(), 2); assert_eq!(after.positions, saved.positions);
    assert!(after.decorations.contains(&proposed) && after.decorations.contains(&other));
    assert_eq!(a.themes[0].host.decorations[&old.placement_id], proposed);
    for at in [old.at, proposed.at] {
        assert!(!hair::player::blocked_at(&a.themes[0].host, at, 0.));
        assert!(!a.themes[0].host.blocked(at));
    }
    let selected_record = data.join(format!(".hair-layout/dc-{}/decoration.toml", old.placement_id));
    let committed = records(&data);
    let mut expected = pinned.clone(); expected.remove(&selected_record);
    let mut actual = committed.clone(); actual.remove(&selected_record);
    assert_eq!(actual, expected, "only selected metadata can change bytes/inode");
    assert_ne!(committed[&selected_record], pinned[&selected_record]);
    assert_eq!(file_mesh(&a, "A.txt"), file); assert_eq!(file_mesh(&a, "Z.txt"), other_file);
    capture(&mut a, &output, "saved-new-position", &backend);
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::F5); view_proposal(&mut a);
    settle_fixture(&mut a);
    assert_eq!(layout(&a), after); assert_eq!(records(&data), committed);
    let refreshed = capture(&mut a, &output, "saved-refreshed", &backend);
    controls(&mut a, &output, "saved-refreshed", &backend, &refreshed);
    let committed_host = a.themes[0].host.clone();
    a.renderer.take().unwrap().wait().unwrap(); drop(a);
    let mut a = app(); a.complete_folder(Ok(Some(data.clone()))).unwrap();
    settle_fixture(&mut a);
    invariant(&a, &after, &committed_host, &data, &committed);
    assert_eq!(file_mesh(&a, "A.txt"), file); assert_eq!(file_mesh(&a, "Z.txt"), other_file);
    let backend = attach(&mut a); view_proposal(&mut a);
    let reopened = capture(&mut a, &output, "cold-reopen-new-position", &backend);
    controls(&mut a, &output, "cold-reopen", &backend, &reopened);
    invariant(&a, &after, &committed_host, &data, &committed);
    for (p, value) in &original { assert_eq!(records(&data).get(p), Some(value)); }
    a.renderer.take().unwrap().wait().unwrap();
    std::fs::write(output.join("WITNESS-PASS.txt"), format!(
        "Actual-X offscreen Metal witness assertions PASS. selected_expected={old:#?}\nproposal={proposed:#?}\nunrelated={other:#?}\nAdapter={backend}\nOriginal file bytes/inodes preserved; only selected decoration metadata changed at commit. Real old collision and actual targeting retained through preview/review. Collision OFF follows saved proposal after commit. Cancel restores original scene/record. All positive scenes from production App path, not diagnostic omission assembly. Missing/wrong proposal and retained-old/wrong-old-omission GPU controls all exceed unchanged >100 world-ROI threshold. Source-mode, exact expected-ID, Follow-theme/fresh Enter/Escape rendered text verified.\nNo native/human visual or comfort acceptance; parent inspection/review/fullsuite pending; package_ready=false.\n")).unwrap();
}
