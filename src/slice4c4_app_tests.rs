//! Explicit offscreen witness only. Synthetic input/aim and tag-adapted fixture;
//! authentic bundled meshes, real App/Renderer, no native/human visual acceptance.
use super::*;
use hair::assets::Vertex;
use sha2::{Digest, Sha256};
#[path = "slice8c_metal_tests.rs"]
mod recovery_metal;

const VIEW: [u32; 2] = [1280, 960];
const SMALL_VIEW: [u32; 2] = [640, 480];
// Independent intended semantics, not sampled from the production overlay.
const PENDING: [f32; 3] = [0.65, 0.7, 0.8];
const REFUSED: [f32; 3] = [1., 0.2, 0.15];
const PARTIAL_LABEL: &str = "Snapshot: partial current-room checks";
const PENDING_LABEL: &str = "Pending fresh review + final validation";
const FROZEN_LABEL: &str = "FROZEN: no writes; fresh Enter saves";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn mesh_bytes(mesh: &[Vertex]) -> &[u8] {
    bytemuck::cast_slice(mesh)
}
#[derive(serde::Serialize)]
struct Shot {
    purpose: String,
    path: String,
    viewport: [u32; 2],
    backend_adapter: String,
    theme: String,
    pose: String,
    app_state: String,
    layout: String,
    preview_record: String,
    snapshot_reason: String,
    effective_controls: Vec<String>,
    footer: String,
    full_document: String,
    preview_mesh_sha256: String,
    scene_sha256: String,
    overlay_sha256: String,
    mesh_sha256_by_placement: std::collections::BTreeMap<String, String>,
    source_sha256: std::collections::BTreeMap<String, String>,
    generic_result: String,
    png_sha256: String,
    visual_acceptance: String,
}
fn sources(a: &App) -> std::collections::BTreeMap<String, String> {
    [
        "src/main.rs", "src/render.rs", "src/resources.rs", "src/theme.rs",
        "src/place_decoration_native.rs", "src/place_mode_native.rs",
        "src/presentation_native.rs", "src/decoration.rs", "src/placement_scene.rs",
        "src/slice4c_app_tests.rs", "src/slice4c4_app_tests.rs", "src/slice4c6_app_tests.rs",
        "src/folder_native.rs", "src/placement/toolbox.rs",
        "packs/office.toml", "packs/archive.toml", "packs/chairDesk.glb",
        "packs/desk.glb", "packs/bookcaseClosedDoors.glb", "packs/bookcaseOpen.glb",
        "packs/bookcaseOpenLow.glb",
    ].into_iter().map(|p| (p.into(), hash(&std::fs::read(a.root.join(p)).unwrap()))).collect()
}
fn attach(a: &mut App) -> String {
    assert!(a.window.is_none() && a.surface.is_none());
    let instance = render::instance();
    let adapter = pollster::block_on(render::adapter(&instance, None)).unwrap();
    let info = adapter.get_info();
    println!("SLICE4C4_ADAPTER {info:?}");
    assert_eq!(info.backend, wgpu::Backend::Metal);
    let mut r = pollster::block_on(render::Renderer::new(
        &adapter, wgpu::TextureFormat::Rgba8UnormSrgb, VIEW,
        &a.themes[a.theme_index].vertices_for(&a.world),
    )).unwrap();
    r.set_theme_appearance(&a.themes[a.theme_index]).unwrap();
    a.renderer = Some(r);
    format!("{info:?}")
}
fn pixels(path: &Path, size: [u32; 2]) -> Vec<u8> {
    let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info().unwrap();
    let mut out = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut out).unwrap();
    assert_eq!([info.width, info.height], size);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    out.truncate(info.buffer_size());
    out
}
fn shot(a: &mut App, output: &Path, label: &str, backend: &str, scene: &[Vertex]) -> Vec<u8> {
    shot_with_overlay(a, output, label, backend, scene, None)
}
fn capture_controls(a: &App) -> Vec<(place_mode_native::Control, [f32; 4], &'static str)> {
    // Receipts mirror place_ui's actual render context. The toolbox is an
    // active-Place-Mode API, not an exploration control inventory.
    if a.place_mode.is_none() || !a.place_context() { return vec![]; }
    a.toolbox_controls()
}
fn shot_with_overlay(a: &mut App, output: &Path, label: &str, backend: &str, scene: &[Vertex], overlay: Option<&[Vertex]>) -> Vec<u8> {
    // State is journaled before drawing so a renderer failure leaves tight evidence.
    a.update_hud();
    if let Some(overlay) = overlay {
        a.renderer.as_mut().unwrap().placement_overlay = overlay.to_vec();
    }
    let dir = output.join(label);
    std::fs::create_dir(&dir).unwrap();
    let target = resources::smoke_output(&a.root, &dir).unwrap();
    let theme = &a.themes[a.theme_index];
    let mut receipt = Shot {
        purpose: label.into(), path: dir.join("office.png").display().to_string(),
        viewport: a.renderer.as_ref().unwrap().size, backend_adapter: backend.into(), theme: theme.name.clone(),
        pose: format!("{:?}", a.pose),
        app_state: format!("focused={} place_mode={} menu={:?} inventory={} reading={} theme_index={} status={}",
            a.focused, a.place_mode.is_some(), a.menu, a.inventory_open, a.world.reading,
            a.theme_index, a.local_status),
        layout: format!("{:?}", layout(a)), scene_sha256: hash(mesh_bytes(scene)),
        preview_record: format!("{:?}", a.decoration_position().ok()),
        snapshot_reason: format!("{:?}", a.place_mode.as_ref().and_then(|m| m.decoration_edit.as_ref()).and_then(|e| e.preview_reason.as_ref())),
        effective_controls: capture_controls(a).into_iter().map(|(_, _, label)| label.into()).collect(),
        footer: a.renderer.as_ref().unwrap().pack_feedback.clone(),
        full_document: if a.world.reading { a.world.document().unwrap().into() } else { String::new() },
        preview_mesh_sha256: a.place_preview().map(|m| hash(mesh_bytes(&m))).unwrap_or_default(),
        overlay_sha256: hash(mesh_bytes(&a.renderer.as_ref().unwrap().placement_overlay)),
        mesh_sha256_by_placement: theme.host.decorations.values().map(|d|
            (d.placement_id.clone(), hash(mesh_bytes(&theme.decoration_preview(d).unwrap())))).collect(),
        source_sha256: sources(a), generic_result: "not attempted".into(), png_sha256: String::new(),
        visual_acceptance: "PENDING parent human inspection; no native comfort or package qualification".into(),
    };
    let receipt_path = dir.join("capture.toml");
    std::fs::write(&receipt_path, toml::to_string(&receipt).unwrap()).unwrap();
    match a.renderer.as_ref().unwrap().capture(&target, "office.png", &a.pose, &a.world) {
        Ok(()) => receipt.generic_result = "passed unchanged production guard".into(),
        Err(e) => {
            receipt.generic_result = format!("refused: {e}; diagnostic pixels only, NOT smoke acceptance");
            std::fs::write(&receipt_path, toml::to_string(&receipt).unwrap()).unwrap();
            println!("SLICE4C4_GENERIC_REFUSAL {label}: {e}");
            assert!(e.starts_with("GPU image suspiciously flat:"), "non-sparse failure; stop: {e}");
            // Existing protected no-follow diagnostic writer, unchanged camera/mesh.
            let fallback = format!("{label}-sparse-diagnostic");
            assert!(overlay.is_none(), "sparse altered-overlay control cannot use helper that refreshes HUD");
            crate::player_app_tests::diagnostic_capture(a, output, &fallback);
            receipt.path = output.join(fallback).join("office.png").display().to_string();
        }
    }
    receipt.png_sha256 = hash(&std::fs::read(&receipt.path).unwrap());
    std::fs::write(&receipt_path, toml::to_string(&receipt).unwrap()).unwrap();
    println!("SLICE4C4_CAPTURE {} sha256={} purpose={label}", receipt.path, receipt.png_sha256);
    pixels(Path::new(&receipt.path), receipt.viewport)
}
fn overview(a: &mut App) {
    a.pose = Pose { position: [-1., 2.8], pitch: -0.30, ..Default::default() };
}
fn menu_item(a: &mut App, item: hair::presentation::Item) {
    let n = a.menu.unwrap().items().len();
    for _ in 0..n {
        if a.menu.unwrap().focused() == item { break; }
        tap(a, KeyCode::ArrowDown);
    }
    assert_eq!(a.menu.unwrap().focused(), item);
    tap(a, KeyCode::Enter);
}
fn switch_by_input(a: &mut App) {
    use hair::presentation::Item;
    assert!(a.renderer.is_some(), "must exercise production GPU branch");
    assert!(a.place_mode.is_none() && a.menu.is_none());
    tap(a, KeyCode::Escape);
    menu_item(a, Item::Settings);
    menu_item(a, Item::Appearance);
    menu_item(a, Item::Theme);
    for _ in 0..3 { tap(a, KeyCode::Escape); }
    assert!(a.menu.is_none());
}
fn invariant(a: &App, saved: &hair::placement_store::LayoutRead, host: &hair::host_scene::HostScene,
             data: &Path, bytes: &std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)>) {
    assert_eq!(&layout(a), saved);
    assert_eq!(&a.themes[a.theme_index].host, host);
    assert_eq!(&records(data), bytes, "all source and metadata bytes/inodes must remain pinned");
    for d in &saved.decorations {
        assert_eq!(a.themes[a.theme_index].host.decorations[&d.placement_id], *d);
        assert_eq!(hair::player::blocked_at(host, d.at, 0.), d.collision);
        assert_eq!(host.blocked(d.at), d.collision);
        assert!(a.themes[a.theme_index].decoration_preview(d).unwrap().len() > 36);
    }
}
fn substitute(scene: &[Vertex], old: &[Vertex], new: &[Vertex]) -> Vec<Vertex> {
    let matches = scene.windows(old.len()).enumerate()
        .filter_map(|(i, w)| (mesh_bytes(w) == mesh_bytes(old)).then_some(i)).collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "exact authored placed mesh must occur once in real scene");
    let mut out = scene.to_vec();
    out.splice(matches[0]..matches[0] + old.len(), new.iter().copied());
    out
}
fn changed_world_pixels(a: &[u8], b: &[u8]) -> usize {
    changed_world_pixels_at(a, b, VIEW)
}
fn changed_world_pixels_at(a: &[u8], b: &[u8], size: [u32; 2]) -> usize {
    // Exclude top/bottom HUD, sidebar and right toolbox; control state is otherwise identical.
    let [w, h] = size.map(|n| n as usize);
    assert_eq!(a.len(), w * h * 4);
    assert_eq!(a.len(), b.len());
    let mut n = 0;
    for y in h * 200 / 960..h - 180 {
        for x in w / 4..(w * 3 / 4).min(w - 320) {
            let i = (y * w + x) * 4;
            if a[i..i + 3] != b[i..i + 3] { n += 1; }
        }
    }
    n
}
fn header_label(a: &App, mesh: &[Vertex], row: usize, label: &str) {
    let size = a.inventory_frame().full;
    let x = (size[0] as f32 - 316.).max(0.) + 8.;
    let y = 80. + row as f32 * 10.;
    let template = hair::placement::toolbox::paint(size, &[], &[(
        [x - 6., y - 7., x + 304., y + 13.], label,
    )], None);
    let expected = &template[12..]; // Exclude panel/control rectangles, keep complete literal text.
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|v| v.position[0].abs() <= 1. && v.position[1].abs() <= 1.));
    assert!(mesh.windows(expected.len()).any(|part| mesh_bytes(part) == mesh_bytes(expected)),
        "intended complete production label missing: {label}");
}
fn pending_labels(a: &mut App, frozen: bool) {
    let mesh = a.place_ui();
    header_label(a, &mesh, 5, PARTIAL_LABEL);
    header_label(a, &mesh, 6, if frozen { FROZEN_LABEL } else { PENDING_LABEL });
    assert!(!mesh.iter().any(|v| v.color == [0.1, 0.9, 0.5]), "partial snapshot must not advertise green admission");
    assert!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().preview_reason.is_none());
    if frozen {
        let controls = a.toolbox_controls();
        assert_eq!(controls.iter().map(|(_, _, label)| *label).collect::<Vec<_>>(),
            ["Enter: save decoration", "Esc: cancel / exit", "P: exit Place Mode"]);
        assert!(controls.iter().all(|(c, _, _)| matches!(c,
            place_mode_native::Control::Enter | place_mode_native::Control::Cancel | place_mode_native::Control::Exit)));
    }
}
fn preview_controls(a: &mut App, output: &Path, choice: &str, backend: &str, scene: &[Vertex], positive: &[u8], color: [f32; 3]) {
    let d = a.decoration_position().unwrap();
    let theme = &a.themes[a.theme_index];
    let source = theme.decoration_preview(&d).unwrap();
    assert_eq!(mesh_bytes(&a.place_preview().unwrap()), mesh_bytes(&source));
    let project = |mesh: &[Vertex]| {
        let tinted = mesh.iter().map(|v| Vertex { color, texture: 0, ..*v }).collect::<Vec<_>>();
        hair::collection::Effect::new(&tinted, &a.pose, a.sidebar_frame()).unwrap().paint(0., false)
    };
    let expected = project(&source);
    let actual = &a.renderer.as_ref().unwrap().placement_overlay;
    assert!(expected.len() > 36);
    assert_eq!(mesh_bytes(&actual[..expected.len()]), mesh_bytes(&expected),
        "GPU overlay must begin with actual selected authored mesh projection, not a cuboid or UI-only frame");
    let missing = actual[expected.len()..].to_vec();
    let other = theme.decorations.entries.iter().find(|e| e.tag != d.tag).unwrap();
    let wrong_asset = theme.decorations.asset(&other.asset).unwrap();
    let mut wrong = project(&wrong_asset.placed(&d).unwrap());
    wrong.extend_from_slice(&missing);
    for (kind, overlay) in [("missing", missing), ("wrong-mesh", wrong)] {
        let negative = shot_with_overlay(a, output, &format!("preview-{choice}-{kind}-control"), backend, scene, Some(&overlay));
        let size = a.renderer.as_ref().unwrap().size;
        let changed = changed_world_pixels_at(positive, &negative, size);
        println!("SLICE4C4_PREVIEW_CONTROL choice={choice} kind={kind} viewport={size:?} world_roi_changed_pixels={changed}");
        assert!(changed > 100, "selected preview must materially reach GPU world ROI");
    }
    a.update_hud();
}
fn controls(a: &mut App, output: &Path, phase: &str, backend: &str, positive: &[u8]) {
    let theme = &a.themes[a.theme_index];
    let scene = theme.vertices_for(&a.world);
    let ds = layout(a).decorations;
    let mut cases = vec![];
    for d in &ds {
        let mesh = theme.decoration_preview(d).unwrap();
        assert!(mesh.iter().all(|v| v.texture == 0), "control assumes bundled untextured meshes");
        cases.push((format!("{phase}-missing-{}", d.placement_id), substitute(&scene, &mesh, &[])));
        let other = ds.iter().find(|other| other.tag != d.tag).unwrap();
        let (_, wrong_asset) = other.resolve(&theme.decorations).unwrap();
        let wrong = wrong_asset.placed(d).unwrap();
        assert_ne!(hash(mesh_bytes(&mesh)), hash(mesh_bytes(&wrong)));
        cases.push((format!("{phase}-wrong-mesh-{}", d.placement_id), substitute(&scene, &mesh, &wrong)));
    }
    for (label, altered) in cases {
        // Deliberately corrupted GPU scene only, NOT a production mutation or accepted positive.
        a.renderer.as_mut().unwrap().set_scene(&altered);
        let negative = shot(a, output, &label, backend, &altered);
        let changed = changed_world_pixels(positive, &negative);
        println!("SLICE4C4_CONTROL {label} world_roi_changed_pixels={changed}");
        assert!(changed > 100, "missing/wrong mesh control must materially affect visible world ROI");
    }
    a.renderer.as_mut().unwrap().set_scene(&scene);
}

#[test]
#[ignore = "explicit serialized offscreen Metal; fresh HAIR_SLICE4C4_OUTPUT and HAIR_SLICE4C4_FIXTURES required"]
fn slice4c4_real_metal_decoration_preview_two_meshes_reopen_and_gpu_theme_witness() {
    let output = PathBuf::from(std::env::var_os("HAIR_SLICE4C4_OUTPUT").expect("explicit unique output"));
    let fixtures = PathBuf::from(std::env::var_os("HAIR_SLICE4C4_FIXTURES").expect("explicit owned fixtures"));
    let mut a = app();
    assert!(output.is_absolute() && fixtures.is_absolute());
    assert!(!output.starts_with(&a.root) && !fixtures.starts_with(&a.root));
    assert!(!output.starts_with(&fixtures) && !fixtures.starts_with(&output));
    std::fs::create_dir(&output).unwrap();
    std::fs::create_dir(&fixtures).unwrap();
    let data = fixtures.join("owned-folder");
    let pack = fixtures.join("owned-tag-adapted-archive");
    std::fs::create_dir(&data).unwrap();
    std::fs::create_dir(&pack).unwrap();
    for name in ["A.txt", "Z.txt"] { std::fs::write(data.join(name), name.as_bytes()).unwrap(); }
    a.complete_folder(Ok(Some(data.clone()))).unwrap();
    let original = records(&data);
    let backend = attach(&mut a);
    std::fs::write(output.join("fixture-provenance.txt"),
        "Synthetic owned two-file folder and input/aim; real bundled office chair/desk.\nSynthetic semantic-tag adaptation only: archive closed-bookcase -> hair.seating.chair; open-bookcase -> hair.furniture.desk. Authentic bundled GLBs copied unchanged.\nControl captures deliberately remove/substitute only GPU scene meshes; never accepted positives.\nNo native window, surface, audio playback or human visual acceptance.\n").unwrap();
    for (choice, at, collision) in [(0, [-2., 0.], false), (1, [0., -1.], true)] {
        if a.place_mode.is_none() { tap(&mut a, KeyCode::KeyP); }
        tap(&mut a, KeyCode::KeyB);
        // Browse from actual current palette cursor, never assign selection state.
        for _ in 0..2 { tap(&mut a, KeyCode::ArrowUp); }
        for _ in 0..choice { tap(&mut a, KeyCode::ArrowDown); }
        tap(&mut a, KeyCode::Enter);
        let selected = a.place_decoration_selection().unwrap().clone();
        assert_eq!(selected.tag, if choice == 0 { "hair.seating.chair" } else { "hair.furniture.desk" });
        tap(&mut a, KeyCode::Escape);
        click(&mut a, place_mode_native::Control::PlaceDecoration);
        aim_floor(&mut a, at);
        tap(&mut a, KeyCode::ArrowRight);
        tap(&mut a, KeyCode::KeyH);
        if a.decoration_position().unwrap().collision != collision { tap(&mut a, KeyCode::KeyH); }
        let preview = a.decoration_position().unwrap();
        assert_eq!(preview.tag, selected.tag);
        assert!(a.place_preview().unwrap().len() > 36);
        let scene = a.themes[0].vertices_for(&a.world);
        pending_labels(&mut a, false);
        let preview_pixels = shot(&mut a, &output, &format!("preview-{choice}"), &backend, &scene);
        // Exact neutral authored projection plus world-only pixel controls replaces
        // the obsolete broad green-pixel oracle; HUD pixels alone cannot pass.
        preview_controls(&mut a, &output, &choice.to_string(), &backend, &scene, &preview_pixels, PENDING);
        let before_small = records(&data);
        a.renderer.as_mut().unwrap().resize(SMALL_VIEW);
        pending_labels(&mut a, false);
        let small = shot(&mut a, &output, &format!("preview-{choice}-640x480"), &backend, &scene);
        preview_controls(&mut a, &output, &format!("{choice}-640x480"), &backend, &scene, &small, PENDING);
        assert_eq!(records(&data), before_small);
        a.renderer.as_mut().unwrap().resize(VIEW);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("frozen decoration review"), "{}", a.local_status);
        let frozen = a.decoration_position().unwrap();
        assert_eq!(frozen, preview);
        let frozen_bytes = records(&data);
        for key in [KeyCode::ArrowLeft, KeyCode::KeyH, KeyCode::KeyB] { tap(&mut a, key); }
        aim_floor(&mut a, [3., 1.]);
        assert_eq!(a.decoration_position().unwrap(), frozen);
        assert_eq!(records(&data), frozen_bytes);
        // Restore the correct viewing ray after the frozen-state movement control.
        // A moved-away UI-only frozen review frame is not a model witness.
        aim_floor(&mut a, at);
        pending_labels(&mut a, true);
        let frozen_pixels = shot(&mut a, &output, &format!("frozen-review-{choice}"), &backend, &scene);
        preview_controls(&mut a, &output, &format!("frozen-{choice}"), &backend, &scene, &frozen_pixels, PENDING);
        a.renderer.as_mut().unwrap().resize(SMALL_VIEW);
        pending_labels(&mut a, true);
        let small = shot(&mut a, &output, &format!("frozen-review-{choice}-640x480"), &backend, &scene);
        preview_controls(&mut a, &output, &format!("frozen-{choice}-640x480"), &backend, &scene, &small, PENDING);
        assert_eq!(a.decoration_position().unwrap(), frozen);
        assert_eq!(records(&data), frozen_bytes);
        a.renderer.as_mut().unwrap().resize(VIEW);
        tap(&mut a, KeyCode::Enter);
        assert!(a.local_status.contains("decoration saved"), "{}", a.local_status);
        assert_eq!(a.themes[0].host.decorations[&frozen.placement_id], frozen);
        for (path, value) in &original { assert_eq!(records(&data).get(path), Some(value)); }
    }
    let saved = layout(&a);
    assert_eq!(saved.decorations.len(), 2);
    assert_ne!(saved.decorations[0].tag, saved.decorations[1].tag);
    assert_ne!(saved.decorations[0].placement_id, saved.decorations[1].placement_id);
    let pinned = records(&data);
    let host = a.themes[0].host.clone();
    std::fs::write(output.join("saved-layout.txt"), format!("{saved:#?}")).unwrap();
    let pinned_text = pinned.iter().map(|(p, (ino, bytes))|
        format!("{} inode={ino} sha256={}\n", p.display(), hash(bytes))).collect::<String>();
    std::fs::write(output.join("pinned-records.txt"), pinned_text).unwrap();
    tap(&mut a, KeyCode::KeyP);
    overview(&mut a);
    let scene = a.themes[0].vertices_for(&a.world);
    shot(&mut a, &output, "two-saved-before-refresh", &backend, &scene);
    tap(&mut a, KeyCode::F5);
    invariant(&a, &saved, &host, &data, &pinned);
    overview(&mut a);
    let scene = a.themes[0].vertices_for(&a.world);
    shot(&mut a, &output, "two-saved-after-refresh", &backend, &scene);
    a.renderer.take().unwrap().wait().unwrap();
    drop(a);
    let mut a = app();
    a.complete_folder(Ok(Some(data.clone()))).unwrap();
    invariant(&a, &saved, &host, &data, &pinned);
    let backend = attach(&mut a);
    overview(&mut a);
    let scene = a.themes[0].vertices_for(&a.world);
    let reopened_pixels = shot(&mut a, &output, "two-saved-reopened-office", &backend, &scene);
    controls(&mut a, &output, "office", &backend, &reopened_pixels);
    invariant(&a, &saved, &host, &data, &pinned);
    for name in ["bookcaseOpenLow.glb", "bookcaseOpen.glb", "bookcaseClosedDoors.glb"] {
        std::fs::copy(a.root.join("packs").join(name), pack.join(name)).unwrap();
    }
    let text = std::fs::read_to_string(a.root.join("packs/archive.toml")).unwrap()
        .replace("hair.storage.closed-bookcase", "hair.seating.chair")
        .replace("hair.storage.open-bookcase", "hair.furniture.desk");
    std::fs::write(pack.join("pack.toml"), text).unwrap();
    let pack_pinned = records(&pack);
    let theme = hair::theme::Theme::load_for_folder_scene(&pack.join("pack.toml"), &mut Default::default(), &host).unwrap();
    for d in &saved.decorations {
        let (entry, asset) = d.resolve(&theme.decorations).unwrap();
        assert_eq!(entry.tag, d.tag);
        assert_eq!(asset.spec.file, if d.tag == "hair.seating.chair" { "bookcaseClosedDoors.glb" } else { "bookcaseOpen.glb" });
        let old = a.themes[0].decoration_preview(d).unwrap();
        let new = theme.decoration_preview(d).unwrap();
        assert_ne!(hash(mesh_bytes(&old)), hash(mesh_bytes(&new)));
        std::fs::write(output.join(format!("mesh-{}.txt", d.placement_id)),
            format!("placement={d:?}\nexact_tag={} authored_asset={} old_vertices={} old_sha256={} new_vertices={} new_sha256={}\n",
                entry.tag, asset.spec.file, old.len(), hash(mesh_bytes(&old)), new.len(), hash(mesh_bytes(&new)))).unwrap();
    }
    a.themes.push(theme);
    let pose = format!("{:?}", a.pose);
    switch_by_input(&mut a);
    assert_eq!(a.theme_index, 1, "{}", a.local_status);
    assert_eq!(format!("{:?}", a.pose), pose);
    invariant(&a, &saved, &host, &data, &pinned);
    let scene = a.themes[1].vertices_for(&a.world);
    // Do not upload here: this must be the GPU scene uploaded by production menu switching.
    let archive_pixels = shot(&mut a, &output, "two-saved-gpu-menu-archive", &backend, &scene);
    assert!(changed_world_pixels(&reopened_pixels, &archive_pixels) > 100);
    controls(&mut a, &output, "archive", &backend, &archive_pixels);
    invariant(&a, &saved, &host, &data, &pinned);
    // A genuinely composed exact-absent Follow theme uses procedural gray art,
    // not a stale CPU scene with its catalog mutated after composition.
    a.themes[0] = hair::theme::Theme::load_for_folder_scene(&a.root.join("packs/archive.toml"), &mut Default::default(), &host).unwrap();
    switch_by_input(&mut a);
    assert_eq!(a.theme_index, 0);
    for d in &saved.decorations {
        assert!(d.artwork(&a.themes[0].decorations).unwrap().is_none());
        assert!(a.themes[0].decoration_preview(d).unwrap()[..36].iter().all(|v| v.color == [0.5; 3] && v.texture == 0));
    }
    invariant(&a, &saved, &host, &data, &pinned);
    let missing_scene = a.themes[0].vertices_for(&a.world);
    let missing = shot(&mut a, &output, "missing-tag-grayboxes-same-saved-pose", &backend, &missing_scene);
    assert!(changed_world_pixels(&archive_pixels, &missing) > 100);
    switch_by_input(&mut a);
    assert_eq!(a.theme_index, 1);
    let returned = shot(&mut a, &output, "missing-tag-return-authentic-art-same-pose", &backend, &scene);
    assert_eq!(changed_world_pixels(&archive_pixels, &returned), 0);
    invariant(&a, &saved, &host, &data, &pinned);
    assert_eq!(records(&pack), pack_pinned);
    // Fresh candidate over a saved decoration: real snapshot + review refusal,
    // no malformed source fixture and no write/retry. Recover full reason via I.
    start(&mut a, [-2., 0.], false);
    a.update_hud();
    const REASON: &str = "Placement overlaps a saved decoration";
    assert_eq!(a.place_mode.as_ref().unwrap().decoration_edit.as_ref().unwrap().preview_reason.as_deref(), Some(REASON));
    header_label(&a, &a.renderer.as_ref().unwrap().placement_overlay, 8, &format!("Snapshot refused: {REASON}").chars().take(49).collect::<String>());
    tap(&mut a, KeyCode::Enter);
    let refusal = a.local_status.clone();
    assert!(refusal.starts_with(REASON), "{refusal}");
    let failed = shot(&mut a, &output, "failed-snapshot-review-refusal", &backend, &scene);
    preview_controls(&mut a, &output, "refused", &backend, &scene, &failed, REFUSED);
    assert_eq!(a.local_status, refusal);
    assert_eq!(records(&data), pinned);
    a.pose.pitch = 0.8; // Directory I, no new floor aim or save attempt.
    tap(&mut a, KeyCode::KeyI);
    assert!(a.place_mode.is_none() && a.world.reading);
    let document = a.world.document().unwrap();
    assert!(document.contains(&format!("STATUS (full):\n{refusal}")));
    assert!(document.contains(&format!("PREVIEW (partial snapshot): {REASON}")));
    shot(&mut a, &output, "failed-snapshot-full-I-recovery", &backend, &scene);
    invariant(&a, &saved, &host, &data, &pinned);
    assert_eq!(records(&pack), pack_pinned);
    a.renderer.take().unwrap().wait().unwrap();
    std::fs::write(output.join("WITNESS-PASS.txt"),
        "Offscreen Metal assertions passed. Neutral partial/pending preview and effective frozen-save controls at 1280x960 and 640x480; exact authored projections and missing/wrong-mesh world-ROI controls for every preview/frozen frame. Two distinct catalog meshes independently saved; refresh/reopen pinned source+metadata bytes/inodes, IDs/tags/sizes/transforms/collision. Real renderer attached during keyboard menu reskin; exact tags map to distinct authored meshes; per-instance missing/wrong-mesh world-ROI controls pass; absent Follow tags show grayboxes and return to authentic art at unchanged saved pose. Failed snapshot/review and full-I recovery retained without writes.\nSynthetic inputs/aim/tag adaptation; NOT physical input or human visual acceptance. Parent visual and independent review pending; package_ready=false.\n").unwrap();
}

#[path = "slice5c_app_tests.rs"]
mod slice5c_witness;

#[path = "slice6c_app_tests.rs"]
mod slice6c_witness;

#[path = "owner_r1b_chair_metal_tests.rs"]
mod owner_r1b_chair;
