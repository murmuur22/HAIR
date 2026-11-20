use hair::{
    host_scene::HostScene,
    theme::{AssetCache, Theme},
};
use std::{fs, path::PathBuf};
fn dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hair-material-{label}-{}", std::process::id()));
    fs::create_dir(&p).unwrap();
    p
}
fn png(path: &std::path::Path, rgba: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(file, 2, 2);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(rgba).unwrap();
}
#[test]
fn png_surfaces_use_world_units_and_opaque_linear_alpha_without_mesh_changes() {
    let d = dir("png");
    let p = d.join("pack.toml");
    png(
        &d.join("wall.png"),
        &[
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 0, 255,
        ],
    );
    png(
        &d.join("floor.png"),
        &[
            0, 255, 255, 255, 255, 0, 255, 255, 128, 128, 0, 255, 32, 64, 128, 255,
        ],
    );
    fs::write(&p,"version=3\nname='PNG'\nroles=[]\n[[surfaces]]\nsurface='wall'\nfile='wall.png'\ntint=[0.8,0.1,0.2]\nmeters_per_tile=2.0\n[[surfaces]]\nsurface='floor'\nfile='floor.png'\ntint=[0.1,0.7,0.2]\nmeters_per_tile=0.5\n").unwrap();
    let t = Theme::load(&p, &mut AssetCache::default()).unwrap();
    assert_eq!(t.textures.len(), 2);
    assert!(
        t.textures
            .iter()
            .all(|t| t.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255))
    );
    assert_eq!(&t.textures[0].rgba[8..12], &[255, 255, 255, 255]);
    assert!(t.textures[0].rgba[4] > 180 && t.textures[0].rgba[4] < 190);
    for v in &t.vertices[..36] {
        assert_eq!(v.uv, [v.position[0] / 0.5, v.position[2] / 0.5]);
        assert_eq!(v.texture, 2);
    }
    for face in t.vertices[36..144].as_chunks::<6>().0.iter() {
        let a = glam::Vec3::from_array(face[1].position) - glam::Vec3::from_array(face[0].position);
        let b = glam::Vec3::from_array(face[2].position) - glam::Vec3::from_array(face[0].position);
        let n = a.cross(b).abs();
        for v in face {
            let q = v.position;
            let uv = if n.y > n.x && n.y > n.z {
                [q[0] / 2.0, q[2] / 2.0]
            } else if n.x > n.z {
                [q[2] / 2.0, q[1] / 2.0]
            } else {
                [q[0] / 2.0, q[1] / 2.0]
            };
            assert_eq!(v.uv, uv);
            assert_eq!(v.texture, 1);
        }
    }
    // Cache warmth and aliases cannot turn a now-invalid declared texture into a partial pack.
    let mut cache = AssetCache::default();
    Theme::load(&p, &mut cache).unwrap();
    fs::write(d.join("floor.png"), b"bad PNG").unwrap();
    assert!(Theme::load(&p, &mut cache).is_err());
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn malformed_material_pack_refuses_every_declaration_without_partial_application() {
    let d = dir("strict");
    let p = d.join("pack.toml");
    let base = "version=3\nname='strict'\nroles=[]\n[[surfaces]]\nsurface='wall'\ntint=[1.0,0.5,0.0]\nmeters_per_tile=1.0\n";
    for bad in [
        base.replace("version=3", "version=4"),
        base.replace("version=3", "version=2"),
        base.replace("1.0,0.5", "nan,0.5"),
        base.replace("1.0,0.5", "1.1,0.5"),
        base.replace("tile=1.0", "tile=0.0"),
        base.replace("'wall'", "'ceiling'"),
        format!("{base}shader='x'\n"),
        format!("{base}[[surfaces]]\nsurface='wall'\ntint=[1,1,1]\nmeters_per_tile=1.0\n"),
        format!("{base}file='../escape.png'\n"),
        format!("{base}file='https://example.com/x.png'\n"),
        format!(
            "{base}[[roles]]\nrole='future'\nfile='missing.glb'\nmax_extent=1.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"
        ),
    ] {
        fs::write(&p, &bad).unwrap();
        assert!(
            Theme::load(&p, &mut AssetCache::default()).is_err(),
            "accepted {bad}"
        );
    }
    png(&d.join("good.png"), &[255; 16]);
    std::os::unix::fs::symlink(d.join("good.png"), d.join("alias.png")).unwrap();
    fs::write(&p, format!("{base}file='alias.png'\n")).unwrap();
    assert!(Theme::load(&p, &mut AssetCache::default()).is_err());
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn project_materials_roundtrip_keep_both_models_and_original_texture_bytes() {
    let d = dir("project");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    png(
        &d.join("input.png"),
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    );
    let mut p = hair::authoring::Project::import(&root.join("assets/kenney/desk.glb")).unwrap();
    p.update(hair::authoring::Edit {
        document_role: false,
        ..p.edit()
    })
    .unwrap();
    for (name, tint, tile) in [
        ("wall", [0.8, 0.2, 0.1], 2.0),
        ("floor", [0.1, 0.5, 0.8], 0.5),
    ] {
        p.set_surface(hair::materials::Surface {
            surface: name.into(),
            file: None,
            tint,
            meters_per_tile: tile,
        })
        .unwrap();
        p.import_surface(name, &d.join("input.png")).unwrap();
    }
    let before = p.preview().unwrap();
    let saved = p.save_new(&d.join("saved"), &[root.into()]).unwrap();
    let q = hair::authoring::Project::open(&saved).unwrap();
    let out = q
        .export(&d.join("export"), &[root.into(), d.join("saved")])
        .unwrap();
    let after = Theme::load(&out, &mut AssetCache::default()).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&before.vertices),
        bytemuck::cast_slice::<_, u8>(&after.vertices)
    );
    assert_eq!(before.loaded_roles, after.loaded_roles);
    assert_eq!(after.loaded_roles.len(), 2);
    assert_eq!(p.surfaces(), q.surfaces());
    assert_eq!(
        fs::read(d.join("export/wall.png")).unwrap(),
        fs::read(d.join("input.png")).unwrap()
    );
    assert!(p.export(&d.join("input.png/nested"), &[]).is_err());
    let old = p.surfaces();
    assert!(
        p.set_surface(hair::materials::Surface {
            surface: "floor".into(),
            file: None,
            tint: [f32::NAN, 0.0, 0.0],
            meters_per_tile: 1.0
        })
        .is_err()
    );
    assert_eq!(p.surfaces(), old);
    fs::remove_dir_all(d).unwrap();
}
#[test]
fn independent_surface_tints_change_only_appearance_not_host_mesh_or_collision() {
    let d = dir("tints");
    let p = d.join("pack.toml");
    fs::write(&p,"version=3\nname='surfaces'\nroles=[]\n[[surfaces]]\nsurface='wall'\ntint=[0.8,0.1,0.2]\nmeters_per_tile=2.0\n[[surfaces]]\nsurface='floor'\ntint=[0.1,0.7,0.2]\nmeters_per_tile=0.5\n").unwrap();
    let host = HostScene::shared_default();
    let original = host.vertices();
    let t = Theme::load_for_scene(&p, &mut AssetCache::default(), &host).unwrap();
    assert_eq!(t.host, host);
    for (a, b) in original.iter().zip(&t.vertices) {
        assert_eq!(a.position, b.position);
    }
    assert!(t.vertices[..36].iter().all(|v| v.color == [0.1, 0.7, 0.2]));
    assert!(
        t.vertices[36..144]
            .iter()
            .all(|v| v.color == [0.8, 0.1, 0.2])
    );
    assert!(
        t.vertices[144..original.len()]
            .iter()
            .zip(&original[144..])
            .all(|(a, b)| bytemuck::bytes_of(a) == bytemuck::bytes_of(b))
    );
    for x in -32..32 {
        for z in -32..32 {
            assert_eq!(
                t.blocked([x as f32 / 8.0, z as f32 / 8.0]),
                host.blocked([x as f32 / 8.0, z as f32 / 8.0])
            );
        }
    }
    fs::remove_dir_all(d).unwrap();
}
