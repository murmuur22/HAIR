//! R1: refuse an unfittable local model before successful publication.
//! Real owned files -> App polling -> Theme, with an unchanged tetrahedron control.
use super::*;
use hair::assets::{Model, Vertex};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};

#[allow(dead_code)]
#[path = "model_safety_fixture.rs"]
pub(crate) mod fixture;

struct Owned(PathBuf);
impl Owned {
    fn new(bytes: &[u8], case: &str) -> Self {
        // Same ownership/no-overlap/exclusive/no-follow guards as the existing
        // model fixtures; distinct prefix avoids parallel test-module collisions.
        let base = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR"))
            .canonicalize()
            .unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        let path = base.join(format!("r1-model-{}-{case}", std::process::id()));
        fs::create_dir(&path).expect("exclusive absent R1 fixture; never reuse leftovers");
        let owned = Self(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(owned.0.join("m.glb"))
            .unwrap();
        file.write_all(bytes).unwrap();
        file.flush().unwrap();
        owned
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn collapsed_bytes() -> Vec<u8> {
    let original = fixture::binary();
    let mut bin = original.clone();
    for position in bin[..48].as_chunks_mut::<12>().0 {
        for (word, value) in position
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip([1.0_f32, 2.0, 3.0])
        {
            word.copy_from_slice(&value.to_le_bytes());
        }
    }
    assert_eq!(
        &bin[48..],
        &original[48..],
        "normals and 12 indices unchanged"
    );
    let doc = fixture::replace(&fixture::document(), "\"max\":[5,4,9]", "\"max\":[1,2,3]");
    fixture::glb(&doc, &bin)
}
fn tap(app: &mut App, key: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(key), down, false, None).unwrap());
    }
}
fn refresh(app: &mut App) {
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    app.update_hud();
}
fn settled(app: &mut App, acquisitions: usize) -> Vec<Vertex> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        assert!(!app.world.reading, "publication must settle before E/I");
        let (attempts, idle) = app.object_previews.test_model_fallback_progress();
        assert!(attempts <= acquisitions, "unexpected automatic retry");
        if idle {
            assert_eq!(attempts, acquisitions, "fixture must actually be acquired");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "scheduler failed to drain: {}",
            app.object_previews.status
        );
        // Yield only: completion is established by queue/active/joined-worker
        // state and acquisition counts, never by elapsed sleep alone.
        std::thread::sleep(Duration::from_millis(1));
    }
    refresh(app);
    let scene = app.themes[0].vertices_for(&app.world);
    let status = app.object_previews.status.clone();
    for _ in 0..32 {
        app.poll_local_image().unwrap();
        assert_eq!(
            app.object_previews.test_model_fallback_progress(),
            (acquisitions, true)
        );
        assert_eq!(
            app.object_previews.status, status,
            "terminal result remains stable"
        );
        assert!(same_scene(&scene, &app.themes[0].vertices_for(&app.world)));
    }
    scene
}
fn same_scene(a: &[Vertex], b: &[Vertex]) -> bool {
    bytemuck::cast_slice::<Vertex, u8>(a) == bytemuck::cast_slice::<Vertex, u8>(b)
}
fn source_vertex(v: &Vertex, model: &Model) -> bool {
    v.texture == 0 && v.color == model.vertices[0].color
}
fn check_normal(scene: &[Vertex], baseline: &[Vertex], model: &Model, app: &App) {
    assert_eq!(app.object_previews.models.len(), 1);
    assert!(app.object_previews.cache.is_empty());
    let a = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap();
    let size = [0.8, 0.85, 0.28];
    let scale = (0..3)
        .map(|i| size[i] / (model.max[i] - model.min[i]))
        .fold(f32::INFINITY, f32::min);
    let center = [
        a.at[0],
        a.at[1] + a.size[1] + 0.12 + (model.max[1] - model.min[1]) * scale * 0.5,
        a.at[2],
    ];
    assert!(scale.is_finite() && scale > 0.0);
    let actual: Vec<_> = scene
        .iter()
        .filter(|v| source_vertex(v, model))
        .copied()
        .collect();
    assert_eq!(
        actual.len(),
        12,
        "positive control must remain a real source mesh"
    );
    for (v, source) in actual.iter().zip(&model.vertices) {
        for (i, center) in center.into_iter().enumerate() {
            let expected =
                center + (source.position[i] - (model.min[i] + model.max[i]) * 0.5) * scale;
            assert!(
                (v.position[i] - expected).abs() < 0.0001,
                "normal uniform XYZ fit"
            );
        }
        assert_eq!(
            (v.color, v.uv, v.texture),
            (source.color, source.uv, source.texture)
        );
    }
    assert!(!same_scene(scene, baseline));
    super::local_model_preview_tests::assert_presentation_preservation(
        baseline,
        scene,
        &app.themes[0],
        &app.world,
        &actual,
    );
}
fn raw_bytes(document: &str) -> Option<Vec<u8>> {
    document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0
        .lines()
        .filter_map(|line| line.strip_prefix("HEX "))
        .flat_map(str::split_whitespace)
        .map(|token| u8::from_str_radix(token, 16).ok())
        .collect()
}

#[test]
fn collapsed_glb_is_refused_without_success_presentation() {
    let normal = fixture::control();
    let collapsed = collapsed_bytes();
    // Shared decoder/count controls, NOT local admission expectations: the
    // eventual narrow local-profile refusal must leave these fixture controls valid.
    let normal_model = hair::assets::decode(&normal, 10.0).expect("valid normal GLB");
    let collapsed_model = hair::assets::decode(&collapsed, 10.0).expect("valid collapsed GLB");
    assert_eq!(
        (normal_model.min, normal_model.max),
        ([1.0, 2.0, 3.0], [5.0, 4.0, 9.0])
    );
    assert_eq!(
        (collapsed_model.min, collapsed_model.max),
        ([1.0, 2.0, 3.0], [1.0, 2.0, 3.0])
    );
    for model in [&normal_model, &collapsed_model] {
        assert_eq!(
            model.vertices.len(),
            12,
            "four indexed triangles, not empty output"
        );
        assert!(model.textures.is_empty());
        assert!(model.vertices.iter().all(|v| source_vertex(v, model)));
        assert!(model.vertices[0].color[0] > 4.0 * model.vertices[0].color[1]);
    }
    assert!(
        collapsed_model
            .vertices
            .iter()
            .all(|v| v.position == [1.0, 2.0, 3.0])
    );
    hair::assets::decode_local_model(&normal).expect("unchanged local-profile positive control");
    let mut failures = Vec::new();
    // Exercise normal first, so an invalid control cannot be mistaken for R1 RED.
    for (case, bytes, model, refusal) in [
        ("normal", &normal, &normal_model, false),
        ("collapsed", &collapsed, &collapsed_model, true),
    ] {
        assert!(bytes.len() < hair::local_explorer::READER_CHUNK_BYTES);
        let owned = Owned::new(bytes, case);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let theme =
            hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default())
                .unwrap();
        let mut app = App::new(
            root.clone(),
            vec![theme],
            0,
            World::load(&root).unwrap(),
            false,
        );
        app.complete_folder(Ok(Some(owned.0.clone()))).unwrap();
        let view = app.world.folder_view.as_ref().unwrap().clone();
        assert_eq!(view.bindings.len(), 1);
        let bound = &view.bindings["entry-0"];
        assert_eq!(bound.entry.name, "m.glb");
        assert_eq!(bound.entry.path, owned.0.join("m.glb"));
        let host = app.themes[0].host.clone();
        let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
        assert_eq!(a.role, "document");
        let inward = host.portal_inward(&a.id);
        app.pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [a.at[0] + 1.05 * inward[0], a.at[2] + 1.05 * inward[1]],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / 1.05).atan(),
        };
        assert!(!host.blocked(app.pose.position));
        refresh(&mut app);
        assert!(app.world.explicit_target.is_none());
        assert_eq!(app.world.interaction.as_ref().unwrap().anchor, "entry-0");
        assert_eq!(app.object_previews.test_model_fallback_progress().0, 0);
        let baseline = app.themes[0].vertices_for(&app.world);
        assert!(
            baseline.len() > app.themes[0].vertices.len(),
            "ordinary filename/kind labels included"
        );
        assert!(same_scene(
            &baseline[..host.vertices().len()],
            &app.themes[0].vertices[..host.vertices().len()]
        ));
        for vertex in app.themes[0].collection_shape("entry-0") {
            assert!(
                baseline
                    .iter()
                    .any(|v| bytemuck::bytes_of(v) == bytemuck::bytes_of(&vertex))
            );
        }
        assert!(!baseline.iter().any(|v| source_vertex(v, model)));
        let source = hair::folder_scene::display(bound.entry.path.as_os_str());
        let current = app.world.current.clone();
        let navigation = app.world.navigation.clone();
        let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
        for (stage_index, stage) in ["before-e", "escape-return", "inspect-return"]
            .into_iter()
            .enumerate()
        {
            let scene = settled(&mut app, stage_index + 1);
            if refusal {
                // FULL scene equality: no geometry/color/spatial/text masks. It
                // includes pack content, ordinary labels/status and every host vertex.
                if !same_scene(&scene, &baseline) {
                    failures.push(format!("{stage}: R1_FALLBACK_PRESENTATION_CHANGED: pack content, ordinary filename/kind/status and host must all remain byte-identical"));
                }
                if !app.object_previews.models.is_empty() || !app.object_previews.cache.is_empty() {
                    failures.push(format!("{stage}: R1_UNFIT_MODEL_PUBLISHED: explicit refusal must precede successful cache/Theme publication"));
                }
                if !app.object_previews.status.starts_with("object fallback:") {
                    failures.push(format!(
                        "{stage}: R1_EXPLICIT_REFUSAL_MISSING: {}",
                        app.object_previews.status
                    ));
                }
                assert!(
                    !scene.iter().any(|v| source_vertex(v, model)),
                    "collapsed source cannot be a rendered mesh"
                );
            } else {
                check_normal(&scene, &baseline, model, &app);
            }
            assert_eq!(app.themes[0].host, host);
            assert_eq!(
                app.world.folder_view.as_ref().unwrap().bindings,
                view.bindings
            );
            assert_eq!(app.world.current, current);
            assert_eq!(app.world.navigation, navigation);
            assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
            assert_eq!(app.world.interaction.as_ref().unwrap().anchor, "entry-0");
            if stage_index == 0 {
                tap(&mut app, KeyCode::KeyE);
                assert!(app.world.reading && app.world.reader_link.is_none());
                let document = app.world.document().unwrap();
                assert!(document.starts_with("RAW STORED BYTES\n"));
                assert_eq!(raw_bytes(document).as_deref(), Some(bytes.as_slice()));
                assert!(document.contains(&source));
                tap(&mut app, KeyCode::Escape);
                assert!(!app.world.reading);
            } else if stage_index == 1 {
                tap(&mut app, KeyCode::KeyI);
                assert!(app.world.reading && app.world.reader_link.is_none());
                let document = app.world.document().unwrap();
                let metadata = hair::folder_scene::metadata(&bound.entry);
                let after_metadata = document
                    .strip_prefix(&metadata)
                    .expect("original metadata remains exact prefix");
                // I now includes the full action status before the unchanged
                // preview snapshot. Assert both sections exactly rather than
                // mistaking this additive disclosure for preview success.
                let action_status = if app.local_status.is_empty() {
                    String::new()
                } else {
                    format!("\nSTATUS (full):\n{}\n", app.local_status)
                };
                let snapshot = after_metadata
                    .strip_prefix(&action_status)
                    .expect("full action status remains exact before preview status");
                let expected_stage = if refusal {
                    "refused; held-source acquisition or preview processing refused; raw bytes retained\nTheme: not admitted (CPU admission map only)"
                } else {
                    "CPU cache ready; not proof of a visible image\nTheme: admitted (CPU admission map only)"
                };
                assert_eq!(
                    snapshot,
                    format!(
                        "\nPREVIEW STATUS (snapshot at inspection; not live monitoring):\n{expected_stage}\nRendering/presentation: unverified; no owner-process thumbnail claim\n"
                    )
                );
                assert!(document.contains("\"m.glb\"") && document.contains(&source));
                assert!(
                    document
                        .contains("GLB suffix (bounded static mesh or fallback; E reads bytes)")
                );
                tap(&mut app, KeyCode::Escape);
                assert!(!app.world.reading);
            }
        }
        assert_eq!(fs::read(owned.0.join("m.glb")).unwrap(), *bytes);
        drop(app); // join real owned workers before removing only our fixture
    }
    // Delayed only to establish controls, raw E, I identity and both lifecycle
    // returns on the unfixed product; never turn setup failure into a product RED.
    assert!(
        failures.is_empty(),
        "R1_HONEST_FALLBACK_RED\n{}",
        failures.join("\n")
    );
}
