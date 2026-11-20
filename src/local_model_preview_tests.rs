//! One real owned GLB -> App room scheduler -> normal Theme world-mesh tracer.
//! The reference decoder output is an oracle only, never installed into App.
use super::*;
use hair::assets::{Model, Vertex};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};

const EPS: f32 = 0.0001;
const POINTS: [[f32; 3]; 4] = [
    [1.0, 2.0, 3.0],
    [5.0, 2.0, 3.0],
    [1.0, 4.0, 3.0],
    [2.0, 2.5, 9.0],
];

fn synthetic_glb() -> Vec<u8> {
    // Off-origin asymmetric tetrahedron: extents [4,2,6], no UV/image/URI,
    // animation, skin, extension or script. All geometry is in this BIN chunk.
    let mut bin = Vec::new();
    for point in POINTS {
        for value in point {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    for _ in POINTS {
        for value in [0.0_f32, 1.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [0_u16, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3] {
        bin.extend_from_slice(&index.to_le_bytes());
    }
    assert_eq!(bin.len(), 120);
    let mut json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"buffers":[{"byteLength":120}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":48,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":96,"byteLength":24,"target":34963}],"accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[1,2,3],"max":[5,4,9]},{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":2,"componentType":5123,"count":12,"type":"SCALAR"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.91,0.13,0.67,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0,"mode":4}]}]}"#.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    for word in [
        0x4654_6c67_u32,
        2,
        (12 + 8 + json.len() + 8 + bin.len()) as u32,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    bytes.extend_from_slice(&bin);
    assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
    bytes
}

struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let base =
            PathBuf::from(std::env::var_os("TMPDIR").expect("explicit candidate-owned TMPDIR"))
                .canonicalize()
                .unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        let path = base.join(format!("m{}", std::process::id()));
        fs::create_dir(&path).expect("absent exclusive model fixture");
        let fixture = Self(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(fixture.0.join("m.glb"))
            .unwrap();
        file.write_all(bytes).unwrap();
        file.flush().unwrap();
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() <= EPS)
}
fn landmark(v: &Vertex, color: [f32; 3]) -> bool {
    v.texture == 0 && near(v.color, color)
}
fn tap(app: &mut App, key: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(key), down, false, None).unwrap());
    }
}
fn published_scene(app: &mut App, color: [f32; 3]) -> Vec<Vertex> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        // Actual normal-redraw consumer, including scheduler publication. Never
        // call Theme setters, loader path APIs or inject decoded model vertices.
        app.poll_local_image().unwrap();
        assert!(!app.world.reading, "model must appear BEFORE E");
        let scene = app.themes[app.theme_index].vertices_for(&app.world);
        if scene.iter().any(|v| landmark(v, color)) {
            return scene;
        }
        assert!(
            Instant::now() < deadline,
            "GLB_MODEL_BEFORE_E_MISSING: normal App room polling published no source-colored model mesh; status={}",
            app.object_previews.status
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn fitted_mesh(scene: &[Vertex], model: &Model, a: &hair::host_scene::Anchor) {
    let color = model.vertices[0].color;
    let mut remaining: Vec<_> = scene.iter().filter(|v| landmark(v, color)).collect();
    assert_eq!(
        remaining.len(),
        12,
        "actual tetrahedron, not a flat image quad"
    );
    let center = [a.at[0], a.at[1] + a.size[1] * 0.5, a.at[2]];
    let extent: [f32; 3] = std::array::from_fn(|i| model.max[i] - model.min[i]);
    let scale = (0..3)
        .map(|i| a.size[i] / extent[i])
        .fold(f32::INFINITY, f32::min);
    // Fixture deliberately makes Z limiting: fitting X/Y alone must overflow Z.
    assert!(a.size[2] / extent[2] < a.size[0] / extent[0]);
    assert!(a.size[2] / extent[2] < a.size[1] / extent[1]);
    for source in &model.vertices {
        let expected = std::array::from_fn(|i| {
            center[i] + (source.position[i] - (model.min[i] + model.max[i]) * 0.5) * scale
        });
        let found = remaining
            .iter()
            .position(|v| near(v.position, expected) && v.uv == source.uv)
            .unwrap_or_else(|| panic!("GLB_CENTERED_UNIFORM_XYZ_FIT: missing {expected:?}"));
        remaining.remove(found);
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in scene.iter().filter(|v| landmark(v, color)) {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
            assert!(v.position[i] >= center[i] - a.size[i] * 0.5 - EPS);
            assert!(v.position[i] <= center[i] + a.size[i] * 0.5 + EPS);
        }
    }
    for i in 0..3 {
        assert!(((min[i] + max[i]) * 0.5 - center[i]).abs() <= EPS);
        assert!(((max[i] - min[i]) / extent[i] - scale).abs() <= EPS);
    }
    assert!((max[2] - min[2] - a.size[2]).abs() <= EPS);
}
// Presentation successor: the original outside_bytes equality is archived in
// gpu/archive/local_model_preview_tests.original.rs, not silently preserved.
// Remove ONLY exact owned triangles, never all text/status/envelope geometry.
fn subtract_owned(scene: &[Vertex], owned: &[Vertex]) -> Vec<Vertex> {
    assert!(scene.len().is_multiple_of(3) && owned.len().is_multiple_of(3));
    let mut remaining: Vec<_> = owned.as_chunks::<3>().0.iter().collect();
    let mut out = Vec::new();
    for triangle in scene.as_chunks::<3>().0.iter() {
        let bytes = bytemuck::cast_slice::<Vertex, u8>(triangle);
        if let Some(i) = remaining
            .iter()
            .position(|t| bytemuck::cast_slice::<Vertex, u8>(*t) == bytes)
        {
            remaining.remove(i);
        } else {
            out.extend_from_slice(triangle);
        }
    }
    assert!(
        remaining.is_empty(),
        "exact owned composition missing triangles"
    );
    out
}
fn old_status(a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    let at = [a.at[0], a.at[1] + a.size[1] + 0.03, a.at[2]];
    let size = [0.45, 0.02, 0.2];
    let corners: [[f32; 3]; 8] = std::array::from_fn(|i| {
        std::array::from_fn(|j| at[j] + size[j] * if i & (1 << j) == 0 { -0.5 } else { 0.5 })
    });
    let mut out = Vec::new();
    for face in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(Vertex {
                position: corners[face[i]],
                color: [0.1, 0.8, 0.3],
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
    out
}
// Shared TEST-only preservation assertion for the CPU tracer and external GPU
// successor. Public existing Theme composition is used only for the unchanged
// baseline pack role, never as the GPU source/decoration reference.
pub(super) fn assert_presentation_preservation(
    baseline: &[Vertex],
    scene: &[Vertex],
    theme: &hair::theme::Theme,
    world: &World,
    source: &[Vertex],
) {
    let view = world.folder_view.as_ref().unwrap();
    assert_eq!(
        view.bindings.len(),
        1,
        "scoped one-anchor composition fixture"
    );
    assert!(world.explicit_target.is_none());
    let bound = &view.bindings["entry-0"];
    let a = theme
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap();
    let mut without = theme.host.clone();
    // Existing folder composition hides the schema vocabulary-document anchor.
    // Keep its required role/envelope in the observation-only host, changing
    // only its ID to that already-defined invisible vocabulary. Removing the
    // required document role outright would make Theme::anchor panic.
    without
        .anchors
        .iter_mut()
        .find(|anchor| anchor.id == a.id)
        .unwrap()
        .id = "vocabulary-document".into();
    // Exact per-anchor fixture differencing includes the entire office role,
    // even portions inside the envelope: nothing is discarded by spatial region.
    let other =
        hair::theme::Theme::load_for_folder_scene(&theme.source, &mut Default::default(), &without)
            .unwrap();
    let base_owned = subtract_owned(&theme.vertices, &other.vertices);
    assert!(!base_owned.is_empty());
    let status = old_status(a);
    let content = subtract_owned(&base_owned, &status);
    assert!(!content.is_empty());
    let name = hair::folder_scene::display(&bound.entry.name);
    let at = [
        a.at[0],
        a.at[1] + a.size[1] + 0.22,
        a.at[2] + a.size[2] * 0.5 + 0.02,
    ];
    let mut old_owned = content;
    old_owned.extend_from_slice(&status);
    old_owned.extend(hair::text::object_label(&name, at, 25));
    old_owned.extend(hair::text::object_label(
        hair::folder_scene::kind(&bound.entry),
        [at[0], at[1] - 0.12, at[2]],
        28,
    ));
    let mut new_owned = source.to_vec();
    assert_eq!(new_owned.len(), 12);
    let z = a.at[2] - a.size[2] * 0.5 - 0.35;
    new_owned.extend(status.into_iter().map(|mut v| {
        v.position[0] = a.at[0] + (v.position[0] - a.at[0]) * 0.5;
        v.position[1] -= 0.18;
        v.position[2] = z + (v.position[2] - a.at[2]) * 0.3;
        v
    }));
    // Declared filename/type layout successor only. Status/source/subtraction
    // and every unrelated-geometry equality above/below remain unchanged.
    let at = [a.at[0] + 1.0, a.at[1] + a.size[1] - 0.2, z];
    new_owned.extend(hair::text::object_label(&name, at, 25));
    new_owned.extend(hair::text::object_label(
        "GLB static mesh",
        [at[0], at[1] - 0.14, at[2]],
        25,
    ));
    let old_rest = subtract_owned(baseline, &old_owned);
    let new_rest = subtract_owned(scene, &new_owned);
    assert!(
        !old_rest.is_empty(),
        "unchanged host/portal/labels cannot disappear"
    );
    assert_eq!(
        bytemuck::cast_slice::<Vertex, u8>(&old_rest),
        bytemuck::cast_slice::<Vertex, u8>(&new_rest),
        "all non-content/non-own-decoration geometry byte-identical, including other labels and markers"
    );
}
fn raw_bytes(document: &str) -> Option<Vec<u8>> {
    let body = document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    let mut bytes = Vec::new();
    for line in body.lines().filter_map(|line| line.strip_prefix("HEX ")) {
        for token in line.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
    }
    Some(bytes)
}

#[test]
fn static_glb_presentation_preserves_source_fit_and_other_geometry() {
    let bytes = synthetic_glb();
    // A setup positive control through the EXISTING held-byte decoder, not the
    // assertion under test. Invalid fixture/profile must fail here, not as RED.
    let model = hair::assets::decode(&bytes, 10.0).expect("valid self-contained static test GLB");
    assert_eq!(model.min, [1.0, 2.0, 3.0]);
    assert_eq!(model.max, [5.0, 4.0, 9.0]);
    assert_eq!(model.vertices.len(), 12);
    assert!(model.textures.is_empty());
    let color = model.vertices[0].color;
    assert!(color[0] > color[1] * 4.0 && color[2] > color[1] * 3.0);
    assert!(model.vertices.iter().all(|v| landmark(v, color)));
    let fixture = Fixture::new(&bytes);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(fixture.0.clone()))).unwrap();
    let view = app.world.folder_view.as_ref().unwrap().clone();
    assert_eq!(view.bindings.len(), 1);
    let (id, bound) = view.bindings.iter().next().unwrap();
    assert_eq!(bound.entry.name, "m.glb");
    assert_eq!(bound.entry.path, fixture.0.join("m.glb"));
    let host = app.themes[0].host.clone();
    let anchor = host.anchors.iter().find(|a| a.id == *id).unwrap();
    assert_eq!(anchor.role, "document");
    let inward = host.portal_inward(id);
    app.pose = Pose {
        position: [
            anchor.at[0] + 1.05 * inward[0],
            anchor.at[2] + 1.05 * inward[1],
        ],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(&host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, *id);
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    let current = app.world.current.clone();
    let baseline = app.themes[0].vertices_for(&app.world);
    assert!(!baseline.iter().any(|v| landmark(v, color)));
    let materials = app.themes[0].textures.to_vec();

    let scene = published_scene(&mut app, color); // Intended baseline RED, before E.
    fitted_mesh(&scene, &model, anchor);
    assert_ne!(
        bytemuck::cast_slice::<Vertex, u8>(&scene),
        bytemuck::cast_slice::<Vertex, u8>(&baseline)
    );
    let source: Vec<_> = scene
        .iter()
        .filter(|v| landmark(v, color))
        .copied()
        .collect();
    assert_presentation_preservation(&baseline, &scene, &app.themes[0], &app.world, &source);
    assert_eq!(app.themes[0].host, host); // Includes anchors/collision/routes.
    assert_eq!(app.world.folder_view.as_ref().unwrap(), &view);
    assert_eq!(app.world.current, current);
    assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
    assert_eq!(app.themes[0].textures.len(), materials.len());
    assert!(
        materials
            .iter()
            .zip(&app.themes[0].textures)
            .all(|(a, b)| Arc::ptr_eq(a, b))
    );

    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    let document = app.world.document().unwrap();
    assert!(
        document.starts_with("RAW STORED BYTES\n"),
        "binary GLB retains the existing raw reader"
    );
    assert_eq!(raw_bytes(document).as_deref(), Some(bytes.as_slice()));
    assert!(document.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    let returned = published_scene(&mut app, color); // No F5/injection.
    fitted_mesh(&returned, &model, anchor);
    let source: Vec<_> = returned
        .iter()
        .filter(|v| landmark(v, color))
        .copied()
        .collect();
    assert_presentation_preservation(&baseline, &returned, &app.themes[0], &app.world, &source);
    assert_eq!(app.themes[0].host, host);
    assert_eq!(app.world.current, current);
    assert_eq!(
        app.world.folder_view.as_ref().unwrap().bindings,
        view.bindings
    );
    assert_eq!(fs::read(fixture.0.join("m.glb")).unwrap(), bytes);
    drop(app); // Retire/join real workers before deleting only the owned fixture.
}
