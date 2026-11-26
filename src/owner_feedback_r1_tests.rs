//! Owner follow-up: real App consumers, owned fixtures, no injected preview results.
use super::*;
use hair::assets::Vertex;

fn records(root: &Path) -> std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)> {
    let mut out = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() { out.extend(records(&path)); }
        else { let bytes = std::fs::read(&path).unwrap(); out.insert(path, (meta.ino(), bytes)); }
    }
    out
}

fn image_labels(a: &App, seconds: f64) {
    let theme = &a.themes[a.theme_index];
    let view = a.world.folder_view.as_ref().unwrap();
    for (id, bound) in &view.bindings {
        if hair::file_items::Class::of(&bound.entry.name) != hair::file_items::Class::Image { continue; }
        let mesh = theme.file_item_shape_at(id, &bound.entry.name, seconds);
        assert!(!mesh.is_empty());
        let lo: [f32; 3] = std::array::from_fn(|i| mesh.iter().map(|v| v.position[i]).fold(f32::INFINITY, f32::min));
        let hi: [f32; 3] = std::array::from_fn(|i| mesh.iter().map(|v| v.position[i]).fold(f32::NEG_INFINITY, f32::max));
        let cell = ((hi[0] - lo[0]).max(0.8) / 150.).min(0.012);
        let at = [(lo[0] + hi[0]) * 0.5, hi[1] + 16. * cell, hi[2]];
        let name = hair::folder_scene::display(&bound.entry.name);
        let scene = theme.vertices_for_at(&a.world, seconds);
        let kind = if theme.folder_preview_admitted(id, a.object_previews.cache.get(id), None) { "PNG image" } else { hair::folder_scene::kind(&bound.entry) };
        for (text, at) in [(name.as_str(), at), (kind, [at[0], at[1] - 8. * cell, at[2]])] {
            let mut expected = hair::text::object_label(text, [0.; 3], 25);
            for v in &mut expected { v.position = [at[0] + v.position[0] * cell / 0.012, at[1] + v.position[1] * cell / 0.012, at[2]]; }
            assert!(!expected.is_empty());
            assert!(scene.windows(expected.len()).any(|w| bytemuck::cast_slice::<Vertex, u8>(w) == bytemuck::cast_slice::<Vertex, u8>(&expected)),
                "OWNER_R1_DETACHED_IMAGE_LABEL id={id} name={name} label={text} mesh_lo={lo:?} mesh_hi={hi:?} expected_at={at:?}");
        }
    }
}
fn images_ready(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        a.poll_local_image().unwrap();
        if a.object_previews.cache.len() >= 2 { break; }
        assert!(Instant::now() < deadline, "real preview queue failed: {}", a.object_previews.status);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![];
    {
        let mut e = png::Encoder::new(&mut bytes, width, height);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&vec![127; (width * height * 3) as usize]).unwrap();
    }
    bytes
}
#[test]
fn owner_r1_initial_async_png_labels_share_item_pose_through_refresh_and_reposition() {
    let f = fixture("owner-r1-labels");
    let originals = [("Feature List.png", png(48, 24)), ("Tall.png", png(12, 48))];
    for (name, bytes) in &originals { std::fs::write(f.0.join(name), bytes).unwrap(); }
    let mut a = app();
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    image_labels(&a, 0.); // Before the production asynchronous queue publishes.
    images_ready(&mut a);
    image_labels(&a, 0.);
    std::fs::write(f.0.join("AAA.txt"), b"slot insertion").unwrap();
    tap(&mut a, KeyCode::F5);
    images_ready(&mut a);
    image_labels(&a, 0.);
    aim_file(&mut a, "Feature List.png");
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
    begin_positioning(&mut a);
    aim_floor(&mut a, [2., 0.]);
    tap(&mut a, KeyCode::ArrowRight); tap(&mut a, KeyCode::KeyJ);
    tap(&mut a, KeyCode::Enter);
    assert_eq!(a.editor_phase(), "Review");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("reposition saved"), "{}", a.local_status);
    image_labels(&a, 0.); image_labels(&a, 3.);
    let before = records(&f.0);
    tap(&mut a, KeyCode::F5); images_ready(&mut a); image_labels(&a, 3.);
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    images_ready(&mut reopened); image_labels(&reopened, 3.);
    assert_eq!(records(&f.0), before);
    for (name, bytes) in originals { assert_eq!(std::fs::read(f.0.join(name)).unwrap(), bytes); }
}

#[test]
fn owner_r1_image_labels_follow_real_page_theme_and_pending_admission_boundaries() {
    let f = fixture("owner-r1a-pages");
    for n in 0..14 { std::fs::write(f.0.join(format!("Image-{n:02}.png")), png(24, 24)).unwrap(); }
    let before = records(&f.0);
    let mut a = app();
    a.themes.push(hair::theme::Theme::load(&a.root.join("packs/archive.toml"), &mut Default::default()).unwrap());
    a.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    image_labels(&a, 0.); images_ready(&mut a); image_labels(&a, 0.);
    // Same entry-N IDs are reused on another page. Real E portal input must
    // retire the former names/pixels before asynchronous admission of this page.
    let portal = |a: &mut App, id: &str| {
        let host = &a.themes[a.theme_index].host;
        let anchor = host.anchors.iter().find(|x| x.id == id).unwrap();
        let n = host.portal_inward(id);
        a.pose = Pose { position: [anchor.at[0] + 1.05 * n[0], anchor.at[2] + 1.05 * n[1]],
            yaw: (-n[0]).atan2(n[1]), pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(), ..Default::default() };
        hair::interaction::refresh(host, &a.pose, &mut a.world);
        assert_eq!(a.world.interaction.as_ref().unwrap().anchor, id);
        tap(a, KeyCode::KeyE);
    };
    portal(&mut a, "folder-next");
    assert_eq!(a.world.folder_view.as_ref().unwrap().page, 1);
    assert!(a.object_previews.cache.is_empty()); image_labels(&a, 0.);
    images_ready(&mut a); image_labels(&a, 0.);
    a.menu = Some(hair::presentation::Menu::settings());
    a.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(a.theme_index, 1); image_labels(&a, 0.);
    a.menu = None;
    portal(&mut a, "folder-previous");
    assert_eq!(a.world.folder_view.as_ref().unwrap().page, 0);
    image_labels(&a, 0.); images_ready(&mut a); image_labels(&a, 0.);
    let mut cold = app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    image_labels(&cold, 0.); images_ready(&mut cold); image_labels(&cold, 0.);
    assert_eq!(records(&f.0), before);
}

#[test]
fn owner_r1_authored_chair_contacts_rendered_platform_in_preview_saved_and_cold_open() {
    let (f, mut a) = open_fixture("owner-r1-chair");
    tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Enter);
    begin_positioning(&mut a);
    aim_floor(&mut a, [-2., 0.]);
    let d = a.decoration_position().unwrap();
    assert_eq!(d.tag, "hair.seating.chair");
    let source = &a.themes[0].decorations.asset("office-chair").unwrap().model;
    println!("AUTHORED_CHAIR vertices={} min={:?} max={:?}", source.vertices.len(), source.min, source.max);
    let check = |a: &App, d: &hair::placement_store::Decoration, mesh: &[Vertex]| {
        let floor = a.themes[0].host.vertices()[..36].iter().map(|v| v.position[1]).fold(f32::NEG_INFINITY, f32::max);
        let bottom = mesh.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        let buried = mesh.iter().filter(|v| v.position[1] < floor - 0.000001).count();
        println!("CHAIR_CONTACT floor={floor} bottom={bottom} buried_vertices={buried} bounds={:?}", d.bounds().unwrap());
        assert!(bottom >= floor - 0.000001 && (bottom - floor).abs() < 0.000001,
            "OWNER_R1_CHAIR_BELOW_REAL_FLOOR: {buried} authored vertices below support top");
        let b = d.bounds().unwrap();
        assert_eq!(b.at[1], 0., "historic logical bottom is not the visible slab top");
        assert_eq!(b.at[1] + b.size[1], d.size[1], "saved logical top cannot move");
        assert!(mesh.iter().all(|v| v.position[1] <= d.size[1] + 0.000001));
        let (_, asset) = d.resolve(&a.themes[0].decorations).unwrap();
        let turn = glam::Mat3::from_rotation_y(asset.spec.yaw_degrees.to_radians());
        let authored: Vec<_> = asset.model.vertices.iter().map(|v| Vertex {
            position: (turn * glam::Vec3::from_array(v.position)).to_array(), ..*v
        }).collect();
        let expected = hair::placement::grounded(&authored, d.at, d.yaw,
            [d.size[0], d.size[1] - floor, d.size[2]]).unwrap();
        assert_eq!(mesh.len(), expected.len());
        for (v, expected) in mesh.iter().zip(expected) {
            assert!(glam::Vec3::from_array(v.position).distance(
                glam::Vec3::from_array(expected.position) + glam::Vec3::Y * floor) < 0.000002,
                "uniform authored proportions within above-slab subvolume");
        }
    };
    check(&a, &d, &a.place_preview().unwrap());
    for yaw in [0., 0.7, 1.57] {
        for size in [[0.8, 0.85, 0.6], [0.4, 1.2, 0.7]] {
            let mut rotated = d.clone(); rotated.yaw = yaw; rotated.size = size;
            check(&a, &rotated, &a.themes[0].decoration_preview(&rotated).unwrap());
        }
    }
    tap(&mut a, KeyCode::Enter); check(&a, &d, &a.place_preview().unwrap());
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    let before = records(&f.0);
    tap(&mut a, KeyCode::F5);
    check(&a, &d, &a.themes[0].decoration_preview(&d).unwrap());
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    check(&reopened, &d, &reopened.themes[0].decoration_preview(&d).unwrap());
    assert_eq!(records(&f.0), before);
}

#[test]
fn owner_r1_legacy_authored_desk_surface_restores_without_moving_saved_file() {
    use hair::{placement_store as store, placement::surface, host_scene::Geometry};
    // Durable owned fixture: retain original-format bytes/inodes even on RED.
    let root = std::env::temp_dir().join(format!("owner-r1a-legacy-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let a = app();
    let theme = &a.themes[0];
    let entry = theme.decorations.entries.iter().find(|e| e.tag == "hair.furniture.desk").unwrap();
    let d = store::Decoration::from_entry(entry, [-2., 0.], 0., [2., 0.8, 2.]).unwrap();
    let (_, asset) = d.resolve(&theme.decorations).unwrap();
    // Exact former implementation is preserved in worker-owner-feedback-r1.log:
    // LoadedAsset::placed called grounded(..., d.size); bounds bottom was zero.
    let turn = glam::Mat3::from_rotation_y(asset.spec.yaw_degrees.to_radians());
    let authored: Vec<_> = asset.model.vertices.iter().map(|v| Vertex {
        position: (turn * glam::Vec3::from_array(v.position)).to_array(), ..*v
    }).collect();
    let old_art = hair::placement::grounded(&authored, d.at, d.yaw, d.size).unwrap();
    let pose = Pose { position: [-2., 1.1], pitch: ((0.8_f32 - 1.65) / 1.1).atan(), ..Default::default() };
    let contact = surface::target(&pose, &[&old_art]).unwrap().unwrap();
    assert!(glam::Vec3::from_array(contact.normal).distance(glam::Vec3::Y) < 0.000001);
    assert!((contact.at[1] - d.size[1]).abs() < 0.00001, "actual authored desk top, not similar Y inference: {contact:?}");
    let p = store::Position { at: [contact.at[0], contact.at[2]], yaw: 0., size: [0.15; 3],
        allow_overlap: false, surface: Some(contact), properties: Default::default() };
    let e = hair::local_explorer::Explorer::new(&root).unwrap();
    let outcome = store::publish(&e, std::ffi::OsStr::new("supported.txt"), b"owned legacy surface file", &p, &std::sync::atomic::AtomicBool::new(false));
    assert!(matches!(outcome, store::Outcome::Committed { .. }), "{outcome:?}");
    // Use only the storage primitive to author historical decoration bytes.
    // This is NOT current placement permission. Prove former clearance/routes
    // with its exact logical envelope through real production file restoration.
    let outcome = store::place_decoration_checked(&e, &d, |_| Ok(()));
    assert!(matches!(outcome, store::Outcome::Committed { .. }), "{outcome:?}");
    let pinned = records(&root);
    let saved = store::read_layout(&e).unwrap();
    assert_eq!(saved.positions, vec![("supported.txt".into(), p.clone())]);
    assert_eq!(saved.decorations, vec![d.clone()]);
    let fresh = hair::local_explorer::Explorer::new(&root).unwrap();
    let (world, base) = hair::folder_scene::build(fresh.current(), 0, true).unwrap();
    let mut old_host = base.clone();
    old_host.geometry.push(Geometry { at: [-2., 0., 0.], size: d.size });
    let mut files = saved.clone(); files.decorations.clear();
    assert!(hair::placement_scene::apply(&files, &world, &mut old_host).unwrap().is_empty(), "former supported layout must admit with walking/sight routes");
    let mut support_only = old_host.clone(); support_only.file_placements.clear();
    surface::clearance(&support_only, &p.envelope_mesh().unwrap(), hair::placement::Overlap::Prevent).unwrap();
    let mut current = base.clone();
    let outcome = hair::placement_scene::apply(&saved, &world, &mut current);
    println!("OWNER_R1A_LEGACY fixture={} contact={contact:?} saved_desk={d:?} current_bounds={:?} restoration={outcome:?}", root.display(), d.bounds().unwrap());
    let mut cold = app();
    let cold_outcome = cold.complete_folder(Ok(Some(root.clone())));
    let id = cold.world.folder_view.as_ref().unwrap().bindings.iter()
        .find(|(_, b)| b.entry.name == std::ffi::OsStr::new("supported.txt")).unwrap().0.clone();
    println!("OWNER_R1B_LEGACY_COLD_APP outcome={cold_outcome:?} status={} host_decorations={:?} installed_file={:?}",
        cold.local_status, cold.themes[0].host.decorations, cold.themes[0].host.file_placements.get(&id));
    // The unchanged format also accepts positive envelopes smaller than the
    // visible slab inset. A fixed-old-top fit cannot silently pretend these
    // have positive above-floor space. Pin that decision boundary explicitly.
    let mut thin = d.clone(); thin.size[1] = hair::host_scene::FLOOR_TOP * 0.5;
    thin.validate().unwrap();
    println!("OWNER_R1A_LEGACY_FIT_BOUNDARY valid_saved_height={} visible_floor={}", thin.size[1], hair::host_scene::FLOOR_TOP);
    assert_eq!(records(&root), pinned, "legacy bytes/inodes/pose pinned, no migration");
    assert!(outcome.is_ok(), "OWNER_R1A_REAL_LEGACY_SUPPORT_REGRESSION: {outcome:?}; fixture retained at {}", root.display());
    cold_outcome.unwrap();
    assert_eq!(cold.themes[0].host.decorations.get(&d.placement_id), Some(&d));
    assert_eq!(cold.themes[0].host.file_placements[&id].position, p);
    assert!(!cold.local_status.contains("unavailable") && !cold.local_status.contains("ignored"));
    tap(&mut cold, KeyCode::F5);
    assert_eq!(cold.themes[0].host.file_placements[&id].position, p);
    assert_eq!(records(&root), pinned);
}

#[test]
fn owner_r1_tiny_legacy_envelopes_keep_finite_art_records_and_disclose_floor_occlusion() {
    let a = app();
    let floor = hair::host_scene::FLOOR_TOP;
    for height in [f32::MIN_POSITIVE, f32::from_bits(1), 0.025, floor, f32::from_bits(floor.to_bits() + 1), 0.051] {
        let f = fixture("owner-r1b-tiny");
        std::fs::write(f.0.join("readable.txt"), b"retained readable source").unwrap();
        let d = hair::placement_store::Decoration::from_entry(&a.themes[0].decorations.entries[0], [-2., 0.], 0., [0.8, height, 0.6]).unwrap();
        let e = hair::local_explorer::Explorer::new(&f.0).unwrap();
        assert!(matches!(hair::placement_store::place_decoration_checked(&e, &d, |_| Ok(())), hair::placement_store::Outcome::Committed { .. }));
        let pinned = records(&f.0);
        let mut cold = app(); cold.complete_folder(Ok(Some(f.0.clone()))).unwrap();
        assert_eq!(cold.themes[0].host.decorations.get(&d.placement_id), Some(&d), "tiny must not trigger generated-room fallback: {}", cold.local_status);
        let mesh = cold.themes[0].decoration_preview(&d).unwrap();
        assert!(!mesh.is_empty() && mesh.iter().all(|v| v.position.iter().all(|x| x.is_finite())));
        assert_eq!(d.bounds().unwrap().at[1], 0.);
        let lo = mesh.iter().map(|v| v.position[1]).fold(f32::INFINITY, f32::min);
        let hi = mesh.iter().map(|v| v.position[1]).fold(f32::NEG_INFINITY, f32::max);
        if height <= floor {
            assert!(lo >= 0. && hi <= height, "no above-floor proxy or invented 3D fit");
            assert!(cold.local_status.contains("Above-floor fit unavailable"), "{}", cold.local_status);
            cold.pose.pitch = 0.8; tap(&mut cold, KeyCode::KeyI);
            assert!(cold.world.document().unwrap().contains("Above-floor fit unavailable"));
            tap(&mut cold, KeyCode::Escape);
        } else {
            assert!((lo - floor).abs() <= f32::EPSILON && hi <= height + f32::EPSILON);
        }
        tap(&mut cold, KeyCode::F5);
        assert_eq!(cold.themes[0].host.decorations.get(&d.placement_id), Some(&d));
        let missing = missing_app(&f.0);
        let box_mesh = missing.themes[0].decoration_preview(&d).unwrap();
        assert!(box_mesh.iter().all(|v| v.position.iter().all(|x| x.is_finite())));
        assert!(box_mesh[..36].iter().all(|v| v.position[1] <= height + f32::EPSILON));
        assert_eq!(records(&f.0), pinned);
        println!("OWNER_R1B_TINY height={height:e} body_y={lo:e}..{hi:e} status={}", cold.local_status);
    }
}

#[test]
fn owner_r1_fit_retains_previously_placeable_thin_source_triangles() {
    let a = app();
    let theme = &a.themes[0];
    let d = hair::placement_store::Decoration::from_entry(&theme.decorations.entries[0], [0.; 2], 0., [16.; 3]).unwrap();
    let mut asset = d.resolve(&theme.decorations).unwrap().1.clone();
    let mut model = asset.model.as_ref().clone();
    // Synthetic source-area boundary control, not substitute authored chair art.
    model.vertices = [[0., 0., 0.], [1., 0., 0.], [0., 0.00000001, 0.]].map(|position| Vertex {
        position, color: [1.; 3], uv: [0.; 2], texture: 0
    }).to_vec();
    assert!(hair::placement::grounded(&model.vertices, d.at, d.yaw, d.size).is_ok());
    assert!(hair::placement::grounded(&model.vertices, d.at, d.yaw, [1.; 3]).is_err());
    asset.model = std::sync::Arc::new(model); asset.spec.yaw_degrees = 0.;
    let mesh = asset.placed(&d).unwrap();
    assert!(mesh.iter().all(|v| v.position.iter().all(|p| p.is_finite())));
}

fn missing_app(folder: &Path) -> App {
    let mut a = app();
    a.themes = vec![hair::theme::Theme::load(&a.root.join("packs/archive.toml"), &mut Default::default()).unwrap()];
    a.complete_folder(Ok(Some(folder.to_path_buf()))).expect("OWNER_R1_MISSING_TAG_MUST_NOT_PREVENT_FOLDER_ENTRY");
    a
}
fn missing_boxes(a: &App) {
    let theme = &a.themes[a.theme_index];
    theme.validate_decorations().unwrap();
    let layout = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    theme.validate_decoration_candidate_layout(&layout).unwrap();
    for d in &layout.decorations {
        let mesh = theme.decoration_preview(d).unwrap();
        assert!(mesh.len() > 36, "box must disclose missing art and identity");
        assert!(mesh[..36].iter().all(|v| v.color == [0.5; 3] && v.texture == 0));
        let b = d.bounds().unwrap();
        let floor = hair::host_scene::FLOOR_TOP;
        let display_scale = if d.size[1] > floor { (d.size[1] - floor) / d.size[1] } else { 1. };
        for axis in 0..3 {
            let lo = mesh[..36].iter().map(|v| v.position[axis]).fold(f32::INFINITY, f32::min);
            let hi = mesh[..36].iter().map(|v| v.position[axis]).fold(f32::NEG_INFINITY, f32::max);
            assert!((hi - lo - b.size[axis] * display_scale).abs() < 0.00001, "uniform saved rotated proportions within historic bounds, not a generic cube");
        }
        let scene = theme.vertices_for(&a.world);
        assert!(scene.windows(mesh.len()).any(|w| bytemuck::cast_slice::<Vertex, u8>(w) == bytemuck::cast_slice::<Vertex, u8>(&mesh)));
    }
}
#[test]
#[ignore = "child-only owned missing-art fresh-process open"]
fn owner_r1_missing_art_cold_process() {
    let folder = PathBuf::from(std::env::var_os("HAIR_OWNER_R1_FOLDER").unwrap());
    assert!(folder.starts_with(std::env::temp_dir()));
    let before = records(&folder);
    let a = missing_app(&folder); missing_boxes(&a);
    assert_eq!(records(&folder), before);
}
#[test]
fn owner_r1_missing_art_folder_refresh_theme_roundtrip_reposition_and_fresh_process() {
    let (f, mut a) = open_fixture("owner-r1-missing");
    for at in [[-2., 0.], [0., -1.]] {
        if a.place_mode.is_none() { tap(&mut a, KeyCode::KeyP); }
        tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Enter);
        a.legacy_follow_fixture();
        begin_positioning(&mut a); aim_floor(&mut a, at);
        tap(&mut a, KeyCode::ArrowRight); tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
    }
    let before = records(&f.0);
    let saved = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
    let actual = a.themes[0].decoration_preview(&saved.decorations[0]).unwrap();
    let mut missing = missing_app(&f.0); missing_boxes(&missing);
    tap(&mut missing, KeyCode::F5); missing_boxes(&missing);
    assert_eq!(records(&f.0), before);
    missing.themes.push(hair::theme::Theme::load_for_folder_scene(&a.root.join("packs/office.toml"), &mut Default::default(), &missing.themes[0].host).unwrap());
    missing.menu = Some(hair::presentation::Menu::settings());
    missing.activate_menu(hair::presentation::Item::Theme);
    assert_eq!(missing.theme_index, 1);
    assert_eq!(bytemuck::cast_slice::<Vertex, u8>(&missing.themes[1].decoration_preview(&saved.decorations[0]).unwrap()), bytemuck::cast_slice::<Vertex, u8>(&actual));
    missing.activate_menu(hair::presentation::Item::Theme); assert_eq!(missing.theme_index, 0); missing_boxes(&missing);
    assert_eq!(records(&f.0), before);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "directory_map_app_tests::place_mode_app_tests::owner_feedback_r1::owner_r1_missing_art_cold_process", "--ignored", "--nocapture", "--test-threads=1"])
        .env("HAIR_OWNER_R1_FOLDER", &f.0).status().unwrap();
    assert!(status.success());
    // The generated box is not a pin source; X can deliberately relocate the
    // same Follow record, and cancel/review remain nonmutating.
    missing.menu = None;
    let d = saved.decorations[0].clone();
    missing.pose = Pose { position: [d.at[0], d.at[1] + 1.1], yaw: 0., pitch: ((d.bounds().unwrap().at[1] + d.size[1] * 0.5 - 1.65) / 1.1).atan(), ..Default::default() };
    tap(&mut missing, KeyCode::KeyP); tap(&mut missing, KeyCode::KeyX);
    assert!(missing.local_status.contains("Missing artwork"), "{}", missing.local_status);
    assert!(!missing.toolbox_controls().iter().any(|(c, _, _)| matches!(c, place_mode_native::Control::PinAsset)));
    begin_positioning(&mut missing);
    aim_floor(&mut missing, [2., 0.]);
    let proposed = missing.decoration_position().unwrap();
    assert_eq!(proposed.placement_id, d.placement_id); assert_eq!(proposed.tag, d.tag); assert_eq!(proposed.size, d.size);
    tap(&mut missing, KeyCode::Enter); assert!(missing.local_status.contains("frozen"), "{}", missing.local_status);
    assert_eq!(records(&f.0), before);
    tap(&mut missing, KeyCode::Escape); assert_eq!(records(&f.0), before);
    assert_eq!(missing.editor_phase(), "Editing");
    tap(&mut missing, KeyCode::Escape);
    assert_eq!(missing.editor_phase(), "Toolbox");
    missing.pose = Pose { position: [d.at[0], d.at[1] + 1.1], yaw: 0., pitch: ((d.bounds().unwrap().at[1] + d.size[1] * 0.5 - 1.65) / 1.1).atan(), ..Default::default() };
    tap(&mut missing, KeyCode::KeyX); begin_positioning(&mut missing); aim_floor(&mut missing, [2., 0.]);
    tap(&mut missing, KeyCode::Enter); tap(&mut missing, KeyCode::Enter);
    assert!(missing.local_status.contains("reposition saved"), "{}", missing.local_status);
    let after = hair::placement_store::read_layout(&missing.local.as_ref().unwrap().explorer).unwrap();
    assert!(after.decorations.contains(&proposed));
    assert!(after.decorations.iter().any(|other| other.placement_id != d.placement_id && saved.decorations.contains(other)));
    missing_boxes(&missing);
}

#[test]
fn owner_r1_missing_art_spends_actual_scene_vertices_without_model_authority() {
    let a = app();
    let theme = &a.themes[0];
    let mut d = hair::placement_store::Decoration::from_entry(&theme.decorations.entries[0], [-2., 0.], 0.7, [0.8, 0.85, 0.6]).unwrap();
    d.tag = format!("hair.{}", "w".repeat(91));
    let mesh = theme.decoration_preview(&d).unwrap();
    assert!(mesh.len() > 36 && mesh.len() <= 32766);
    assert!(mesh.iter().all(|v| v.texture == 0));
    let before = (theme.decorations.entries.len(), theme.decorations.assets.len(), theme.textures.len());
    let mut candidate = hair::placement_store::LayoutRead::default();
    for n in 0..256 {
        let mut next = d.clone(); next.placement_id = format!("{n:032x}"); candidate.decorations.push(next);
    }
    assert!(mesh.len() * candidate.decorations.len() > 300_000);
    let error = theme.validate_decoration_candidate_layout(&candidate).unwrap_err();
    assert!(error.contains("300000 vertices"), "actual procedural geometry must spend scene budget: {error}");
    assert_eq!((theme.decorations.entries.len(), theme.decorations.assets.len(), theme.textures.len()), before);
    d.tag = "hair..malformed".into();
    assert!(theme.decoration_preview(&d).unwrap_err().contains("Invalid Follow-theme"));
}
