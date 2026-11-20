use hair::{host_scene::HostScene, theme::Theme};
use std::{fs, path::Path};
pub(crate) fn glb(points: &[[f32; 3]]) -> Vec<u8> {
    let mut bin: Vec<u8> = points
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let position_bytes = bin.len();
    for _ in points {
        for v in [0.0f32, 0.0, 1.0] {
            bin.extend(v.to_le_bytes());
        }
    }
    for i in 0..points.len() as u32 {
        bin.extend(i.to_le_bytes());
    }
    let mut j=format!(r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":{}}},{{"buffer":0,"byteOffset":{},"byteLength":{}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":{},"type":"VEC3","min":[-1,0,-1],"max":[1,1,1]}},{{"bufferView":1,"componentType":5126,"count":{},"type":"VEC3"}},{{"bufferView":2,"componentType":5125,"count":{},"type":"SCALAR"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,bin.len(),position_bytes,position_bytes,position_bytes,position_bytes*2,points.len()*4,points.len(),points.len(),points.len()).into_bytes();
    while !j.len().is_multiple_of(4) {
        j.push(b' ');
    }
    let mut b = Vec::new();
    for n in [
        0x46546c67u32,
        2,
        (28 + j.len() + bin.len()) as u32,
        j.len() as u32,
        0x4e4f534a,
    ] {
        b.extend(n.to_le_bytes());
    }
    b.extend(j);
    b.extend((bin.len() as u32).to_le_bytes());
    b.extend(0x004e4942u32.to_le_bytes());
    b.extend(bin);
    b
}
fn manifest(yaw: f32, pivot: [f32; 3], scale: f32) -> String {
    format!(
        "version=3\nname='trim test'\n[[roles]]\nrole='door-trim'\nfile='trim.glb'\nmax_extent=10.0\nscale={scale}\nyaw_degrees={yaw}\npivot={pivot:?}\n"
    )
}
pub(crate) fn safe_points() -> Vec<[f32; 3]> {
    vec![
        [-0.49, 0.01, 0.45],
        [-0.41, 0.01, 0.45],
        [-0.41, 0.89, 0.45],
        [0.41, 0.01, 0.45],
        [0.49, 0.01, 0.45],
        [0.41, 0.89, 0.45],
        [-0.39, 0.91, 0.45],
        [0.39, 0.91, 0.45],
        [0.39, 0.99, 0.45],
    ]
}
#[test]
fn consumer_rejects_triangle_crossing_aperture_even_with_vertices_in_trim_slots() {
    let base = std::env::temp_dir().join(format!("hair-door-red-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    // Each vertex is in a jamb or lintel; the triangle interior covers the passage.
    fs::write(
        base.join("trim.glb"),
        glb(&[[-0.45, 0.1, 0.0], [0.45, 0.1, 0.0], [0.0, 0.95, 0.0]]),
    )
    .unwrap();
    fs::write(base.join("pack.toml"), manifest(0.0, [0.0; 3], 1.0)).unwrap();
    assert!(
        Theme::load(&base.join("pack.toml"), &mut Default::default()).is_err(),
        "unsafe diagonal triangle was silently accepted"
    );
    fs::remove_dir_all(base).unwrap();
}
#[cfg(feature = "workshop")]
#[test]
fn actual_door_role_click_native_import_cancel_transform_and_unsafe_preserve_last_good() {
    use hair::workshop::{Command, DialogKind, Workshop};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-door-ui-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    fs::write(base.join("trim.glb"), glb(&safe_points())).unwrap();
    let c = egui::Context::default();
    let mut a = Workshop::new(vec![root.into()]);
    a.dispatch(Command::NewMaterial);
    fn click(c: &egui::Context, a: &mut Workshop, id: &str) {
        let raw = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1800.0),
            )),
            ..Default::default()
        };
        for _ in 0..3 {
            c.run_ui(raw(), |u| a.ui(u, None))
                .drop_without_applying_deltas();
        }
        let pos = a.controls[id].center();
        for pressed in [true, false] {
            let mut r = raw();
            r.events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                },
            ];
            c.run_ui(r, |u| a.ui(u, None))
                .drop_without_applying_deltas();
        }
    }
    click(&c, &mut a, "door-trim");
    assert!(a.project.as_ref().unwrap().door_active);
    let revision = a.revision;
    click(&c, &mut a, "import");
    assert_eq!(a.begin_dialog(), Some(DialogKind::Import));
    a.finish_dialog(Ok(None));
    assert_eq!(revision, a.revision);
    click(&c, &mut a, "import");
    assert_eq!(a.begin_dialog(), Some(DialogKind::Import));
    a.finish_dialog(Ok(Some(base.join("trim.glb"))));
    if a.confirmation_pending() {
        click(&c, &mut a, "confirm-discard");
    }
    assert!(a.error.is_none(), "{:?}", a.error);
    assert!(a.project.as_ref().unwrap().door_active);
    let before = a.preview.as_ref().unwrap().vertices.clone();
    let revision = a.revision;
    let dirty = a.dirty;
    click(&c, &mut a, "rotate-step");
    assert!(a.error.is_none(), "unsupported whole-frame yaw is disabled");
    assert_eq!(a.revision, revision);
    assert_eq!(a.dirty, dirty);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&before),
        bytemuck::cast_slice::<_, u8>(&a.preview.as_ref().unwrap().vertices)
    );
    fs::remove_dir_all(base).unwrap();
}
#[test]
fn consumer_adverse_trim_controls_and_warm_cache() {
    let base = std::env::temp_dir().join(format!("hair-door-adverse-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let pack = base.join("pack.toml");
    let model = base.join("trim.glb");
    let mut cache = Default::default();
    fs::write(&model, glb(&safe_points())).unwrap();
    fs::write(&pack, manifest(0.0, [0.0; 3], 1.0)).unwrap();
    Theme::load(&pack, &mut cache).unwrap();
    for (label, yaw, pivot, scale) in [
        ("rotation", 45.0, [0.0; 3], 1.0),
        ("misaligned", 0.0, [0.2, 0.0, 0.0], 1.0),
        ("overscale", 0.0, [0.0; 3], 4.0),
        ("nonfinite", f32::NAN, [0.0; 3], 1.0),
    ] {
        fs::write(&pack, manifest(yaw, pivot, scale)).unwrap();
        assert!(Theme::load(&pack, &mut cache).is_err(), "{label}");
    }
    fs::write(&pack, manifest(0.0, [0.0; 3], 1.0)).unwrap();
    let mut cube = Vec::new();
    for axis in 0..3 {
        for side in [0.0, 1.0] {
            let other: Vec<_> = (0..3).filter(|a| *a != axis).collect();
            for (u, v) in [
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 1.0),
                (0.0, 0.0),
                (1.0, 1.0),
                (0.0, 1.0),
            ] {
                let mut p = [0.0; 3];
                p[axis] = side;
                p[other[0]] = u;
                p[other[1]] = v;
                p[0] -= 0.5;
                p[2] -= 0.5;
                cube.push(p);
            }
        }
    }
    let mut boundary = safe_points();
    boundary[1][0] = -0.399;
    let mut nonfinite = safe_points();
    nonfinite[0][0] = f32::INFINITY;
    for (label, points) in [
        ("opaque cube", cube),
        ("boundary crossing", boundary),
        ("nonfinite actual positions", nonfinite),
    ] {
        fs::write(&model, glb(&points)).unwrap();
        assert!(Theme::load(&pack, &mut cache).is_err(), "{label}");
    }
    fs::write(&model, vec![0; 8_000_001]).unwrap();
    assert!(Theme::load(&pack, &mut cache).is_err());
    fs::write(&model, glb(&safe_points())).unwrap();
    Theme::load(&pack, &mut cache).unwrap();
    fs::remove_dir_all(base).unwrap();
}
#[test]
fn third_role_aggregate_budget_counts_unmatched_and_warm_cache() {
    let base = std::env::temp_dir().join(format!("hair-door-budget-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let mut text = "version=3\nname='third role budget'\n".to_string();
    for (role, z) in [
        ("document", 0.41),
        ("decoration", 0.42),
        ("door-trim", 0.43),
    ] {
        let tri = [[-0.49, 0.01, z], [-0.41, 0.01, z], [-0.41, 0.89, z]];
        let bytes = glb(&tri);
        let old = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json = std::str::from_utf8(&bytes[20..20 + old])
            .unwrap()
            .replace("\"byteLength\":84", "\"byteLength\":400080")
            .replace("\"byteLength\":12", "\"byteLength\":400008")
            .replace(
                "\"componentType\":5125,\"count\":3",
                "\"componentType\":5125,\"count\":100002",
            );
        let mut json = json.into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = bytes[28 + old..28 + old + 72].to_vec();
        for i in 0..100002u32 {
            bin.extend((i % 3).to_le_bytes());
        }
        let mut b = Vec::new();
        for n in [
            0x46546c67u32,
            2,
            (28 + json.len() + bin.len()) as u32,
            json.len() as u32,
            0x4e4f534a,
        ] {
            b.extend(n.to_le_bytes());
        }
        b.extend(json);
        b.extend((bin.len() as u32).to_le_bytes());
        b.extend(0x004e4942u32.to_le_bytes());
        b.extend(bin);
        fs::write(base.join(format!("{role}.glb")), b).unwrap();
        text.push_str(&format!("[[roles]]\nrole='{role}'\nfile='{role}.glb'\nmax_extent=10.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"));
    }
    let mut cache = Default::default();
    for label in ["door-trim", "future-trim"] {
        fs::write(
            base.join("pack.toml"),
            text.replace("role='door-trim'", &format!("role='{label}'")),
        )
        .unwrap();
        for _ in 0..2 {
            let err = Theme::load(&base.join("pack.toml"), &mut cache)
                .err()
                .expect("third model cannot bypass aggregate budget");
            assert!(err.contains("aggregate whole-pack"), "{err}");
        }
    }
    fs::remove_dir_all(base).unwrap();
}
#[test]
fn transformed_triangle_centroid_crosses_actual_aperture() {
    let r = glam::Mat4::from_rotation_y(45.0f32.to_radians());
    let t = safe_points();
    let p = (r.transform_point3(glam::Vec3::from_array(t[0]))
        + r.transform_point3(glam::Vec3::from_array(t[1]))
        + r.transform_point3(glam::Vec3::from_array(t[2])))
        / 3.0;
    assert!(
        p.x.abs() < 0.4 && p.y > 0.0 && p.y < 0.9,
        "actual triangle interior must cross aperture: {p:?}"
    );
    let mut m = hair::assets::decode(&glb(&safe_points()), 10.0).unwrap();
    assert!(hair::door_trim::qualify(&m, [0.0; 3], 45.0, 1.0).is_err());
    m.vertices.clear();
    assert!(hair::door_trim::qualify(&m, [0.0; 3], 0.0, 1.0).is_err());
}
#[test]
fn door_role_roundtrip_preserves_other_models_and_materials() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-door-roundtrip-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    fs::write(base.join("trim.glb"), glb(&safe_points())).unwrap();
    let mut p = hair::authoring::Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    p.update(hair::authoring::Edit {
        document_role: false,
        ..p.edit()
    })
    .unwrap();
    p.select_door().unwrap();
    assert!(p.door_active);
    assert!(p.model.vertices.is_empty());
    let mut d = hair::authoring::Project::import(&base.join("trim.glb")).unwrap();
    d.assign_import(&p);
    d.preview().unwrap();
    d.update(hair::authoring::Edit {
        yaw_degrees: 0.1,
        at: [0.001, 0.0, 0.0],
        ..d.edit()
    })
    .unwrap();
    let wall = hair::materials::Surface {
        surface: "wall".into(),
        file: None,
        tint: [0.2, 0.4, 0.6],
        meters_per_tile: 2.0,
    };
    d.set_surface(wall.clone()).unwrap();
    let saved = d.save_new(&base.join("saved"), &[root.into()]).unwrap();
    let mut q = hair::authoring::Project::open(&saved).unwrap();
    assert!(q.door_active);
    assert_eq!(q.yaw_degrees, 0.1);
    assert_eq!(q.at, [0.001, 0.0, 0.0]);
    q.select_model(true).unwrap();
    assert_eq!(q.source(), base.join("saved/model.glb"));
    assert_eq!(q.surfaces(), vec![wall]);
    let pack = q.export(&base.join("export"), &[root.into()]).unwrap();
    assert_eq!(
        Theme::load(&pack, &mut Default::default())
            .unwrap()
            .loaded_roles
            .len(),
        3
    );
    fs::remove_dir_all(base).unwrap();
}
#[test]
fn bounded_trim_applies_to_all_greenleaf_portals_without_changing_host() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-door-safe-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    fs::write(base.join("trim.glb"), glb(&safe_points())).unwrap();
    fs::write(base.join("pack.toml"), manifest(0.0, [0.0; 3], 1.0)).unwrap();
    let w = hair::World::load_site(&root.join("examples/GreenleafReadingRoom-appearance")).unwrap();
    for s in &w.spaces.as_ref().unwrap().spaces {
        let t = Theme::load_for_scene(&base.join("pack.toml"), &mut Default::default(), &s.scene)
            .unwrap();
        assert_eq!(t.loaded_roles, vec!["door-trim"]);
        assert_eq!(t.host, s.scene);
        assert!(t.feedback(&w).contains("HERE 2/4 HOST 6/12"));
    }
    assert_eq!(
        HostScene::shared_default().anchor("portal").size,
        [1.4, 2.5, 0.2]
    );
    fs::remove_dir_all(base).unwrap();
}
