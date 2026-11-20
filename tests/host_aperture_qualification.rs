//! Qualification of existing apertures/placement only; parent must compile/run.
use hair::{
    assets::{self, Vertex},
    door_trim, folder_scene,
    host_scene::{Anchor, HostScene},
    local_explorer::{Entry, Kind, Snapshot},
    theme::{AssetCache, Theme},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn host() -> HostScene {
    let snapshot = Snapshot {
        folder: "/ApertureFixture".into(),
        entries: (0..25)
            .map(|i| Entry {
                name: format!("Child{i:03}").into(),
                path: format!("/ApertureFixture/Child{i:03}").into(),
                kind: Kind::Folder,
            })
            .collect(),
    };
    let mut closed = folder_scene::build(&snapshot, 1, false).unwrap().1;
    // These tests qualify actual closed-wall apertures, not local platform presentation.
    // Preserve their original route/material adverses on an explicitly closed host.
    closed.open_platform = false;
    closed
}
fn parse(h: &HostScene) -> hair::Result<HostScene> {
    HostScene::parse(&toml::to_string(h).unwrap())
}
fn refused(h: &HostScene, reason: &str, label: &str) {
    let e = parse(h).unwrap_err();
    assert!(
        e.contains(reason),
        "{label}: expected {reason:?}, got {e:?}"
    );
}
fn anchor_mut<'a>(h: &'a mut HostScene, id: &str) -> &'a mut Anchor {
    h.anchors.iter_mut().find(|a| a.id == id).unwrap()
}
#[test]
fn aperture_identity_role_alignment_and_finite_size_adverses_are_refused() {
    let base = host();
    assert_eq!(base.wall_apertures.len(), 3);
    assert_eq!(parse(&base).unwrap(), base);
    let mut h = base.clone();
    h.wall_apertures[0].anchor = "absent-anchor".into();
    refused(&h, "absent anchor", "dangling");
    let mut h = base.clone();
    h.wall_apertures[0].anchor = "vocabulary-document".into();
    refused(
        &h,
        "unique wall and floor-level explicit portal",
        "wrong role",
    );
    let mut h = base.clone();
    h.wall_apertures[1] = h.wall_apertures[0].clone();
    refused(
        &h,
        "unique wall and floor-level explicit portal",
        "duplicate anchor/wall",
    );
    let mut h = base.clone();
    h.wall_apertures[1].wall = h.wall_apertures[0].wall;
    refused(
        &h,
        "unique wall and floor-level explicit portal",
        "duplicate wall different anchor",
    );
    let mut h = base.clone();
    h.wall_apertures.push(h.wall_apertures[0].clone());
    refused(&h, "at most three", "opening count");
    let mut legacy = HostScene::shared_default();
    legacy.wall_apertures.push(base.wall_apertures[0].clone());
    refused(&legacy, "version 2", "v1 may not acquire openings");
    for (label, normal_offset, floor) in [
        ("floating far", 0.3, 0.0),
        ("too close to wall", -0.2, 0.0),
        ("nonfloor", 0.0, 0.1),
    ] {
        let mut h = base.clone();
        let a = anchor_mut(&mut h, "folder-next");
        a.at[0] += normal_offset;
        a.at[1] = floor;
        refused(
            &h,
            if floor == 0.0 {
                "envelope/alignment/opening size"
            } else {
                "floor-level explicit portal"
            },
            label,
        );
    }
    for (label, width, height) in [
        ("nan width", f32::NAN, 2.2),
        ("infinite height", 1.65, f32::INFINITY),
        ("negative width", -1.0, 2.2),
        ("zero height", 1.65, 0.0),
        ("undersized width", 1.0, 2.2),
        ("oversized width", 2.4, 2.2),
        ("undersized height", 1.65, 1.8),
        ("oversized height", 1.65, 2.5),
    ] {
        let mut h = base.clone();
        h.wall_apertures[0].width = width;
        h.wall_apertures[0].height = height;
        refused(&h, "envelope/alignment/opening size", label);
    }
    let mut h = base.clone();
    anchor_mut(&mut h, "folder-next").at[0] = f32::NAN;
    refused(
        &h,
        "anchor identity, role, binding or envelope",
        "nonfinite anchor",
    );
    let mut h = base;
    anchor_mut(&mut h, "folder-next").size[0] = 0.9;
    refused(
        &h,
        "envelope/alignment/opening size",
        "normal thickness budget",
    );
}
#[test]
fn corner_lintel_quarter_wall_and_total_width_budgets_remain_bounded() {
    let base = host();
    let mut h = base.clone();
    let width = h
        .wall_apertures
        .iter()
        .find(|a| a.anchor == "folder-next")
        .unwrap()
        .width;
    let corner = h.room[1] + width * 0.5 + 0.39;
    anchor_mut(&mut h, "folder-next").at[2] = corner;
    refused(
        &h,
        "envelope/alignment/opening size",
        "corner leaves less than .4m",
    );
    let mut h = base.clone();
    h.height = 2.3;
    refused(
        &h,
        "envelope/alignment/opening size",
        "lintel leaves less than .2m",
    );

    // A complete three-role host isolates the quarter-wall budget from all
    // inherited front slots. The wide-room positive is parsed before shrinking.
    let mut short = HostScene::shared_default();
    short.version = 2;
    short.room = [-5.0, -4.0, 5.0, 4.0];
    short.spawn = [0.0, 1.5];
    short.anchors.retain(|a| a.role != "decoration");
    for a in &mut short.anchors {
        match a.role.as_str() {
            "document" => {
                a.at = [0.0, 0.0, 0.0];
                a.size = [0.4, 0.8, 0.4];
                a.binding = "explicit-page".into();
            }
            "portal" => {
                a.at = [5.7, 0.0, 0.0];
                a.size = [0.6, 2.0, 1.4];
                a.binding = "explicit-door".into();
            }
            "image" => {
                a.at = [3.0, 0.0, -4.5];
                a.size = [0.6, 0.8, 0.1];
            }
            _ => unreachable!(),
        }
    }
    short.wall_apertures = vec![hair::host_scene::WallAperture {
        anchor: "main-link".into(),
        wall: hair::host_scene::Wall::Right,
        width: 1.65,
        height: 2.2,
    }];
    parse(&short).unwrap();
    short.room[1] = -2.0;
    short.room[3] = 2.0;
    refused(
        &short,
        "envelope/alignment/opening size",
        "quarter-wall width",
    );

    let mut total = base;
    for opening in &mut total.wall_apertures {
        opening.width = 2.0;
    }
    for id in [
        folder_scene::PARENT_PORTAL,
        "folder-next",
        "folder-previous",
    ] {
        let a = anchor_mut(&mut total, id);
        if id == folder_scene::PARENT_PORTAL {
            a.size[0] = 1.8;
        } else {
            a.size[2] = 1.8;
        }
    }
    parse(&total).expect("exactly 6m total positive control");
    for opening in &mut total.wall_apertures {
        opening.width = 2.05;
    }
    refused(&total, "total opening width exceeds 6m", "aggregate width");
}
#[test]
fn removing_each_necessary_aperture_keeps_anchor_but_refuses_route() {
    let base = host();
    for opening in &base.wall_apertures {
        let mut h = base.clone();
        h.wall_apertures.retain(|a| a.anchor != opening.anchor);
        assert_eq!(h.anchors, base.anchors, "do not hide the unreachable goal");
        assert!(h.walls().len() < base.walls().len());
        refused(
            &h,
            "document/portal unreachable from spawn",
            &opening.anchor,
        );
    }
}
// Independent test-only six-face cube recipe. Explicit signed corners and
// triangle winding pin the legacy byte-for-byte oracle without calling the
// crate-private production mesh helper or expanding its visibility.
fn legacy_cube(out: &mut Vec<Vertex>, at: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let corners = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [0.5, 0.5, 0.5],
    ]
    .map(|sign: [f32; 3]| {
        std::array::from_fn::<_, 3, _>(|axis| at[axis] + size[axis] * sign[axis])
    });
    let triangles = [
        [0, 1, 3],
        [0, 3, 2],
        [4, 6, 7],
        [4, 7, 5],
        [0, 4, 5],
        [0, 5, 1],
        [2, 3, 7],
        [2, 7, 6],
        [0, 2, 6],
        [0, 6, 4],
        [1, 5, 7],
        [1, 7, 3],
    ];
    for triangle in triangles {
        for index in triangle {
            out.push(Vertex {
                position: corners[index],
                color,
                uv: [0.0; 2],
                texture: 0,
            });
        }
    }
}
#[test]
fn absent_aperture_metadata_preserves_exact_legacy_slabs_and_shell_vertices() {
    let legacy = HostScene::shared_default();
    assert!(legacy.wall_apertures.is_empty());
    let [x0, z0, x1, z1] = legacy.room;
    let expected = vec![
        hair::host_scene::Geometry {
            at: [(x0 + x1) * 0.5, 0.0, z1 + 0.2],
            size: [x1 - x0 + 0.4, legacy.height, 0.2],
        },
        hair::host_scene::Geometry {
            at: [x0 - 0.2, 0.0, (z0 + z1) * 0.5],
            size: [0.2, legacy.height, z1 - z0],
        },
        hair::host_scene::Geometry {
            at: [x1 + 0.2, 0.0, (z0 + z1) * 0.5],
            size: [0.2, legacy.height, z1 - z0],
        },
    ];
    assert_eq!(legacy.walls(), expected);
    let mut shell = Vec::new();
    legacy_cube(
        &mut shell,
        [(x0 + x1) * 0.5, 0.0, (z0 + z1) * 0.5],
        [x1 - x0 + 0.4, 0.1, z1 - z0 + 0.4],
        [0.22, 0.24, 0.21],
    );
    for (i, g) in expected.iter().enumerate() {
        legacy_cube(
            &mut shell,
            [g.at[0], g.at[1] + g.size[1] * 0.5, g.at[2]],
            g.size,
            if i == 0 {
                [0.50, 0.52, 0.46]
            } else {
                [0.46, 0.49, 0.43]
            },
        );
    }
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&legacy.vertices()),
        bytemuck::cast_slice::<_, u8>(&shell)
    );
    let encoded = toml::to_string(&legacy).unwrap();
    assert!(!encoded.contains("wall_apertures"));
    assert_eq!(parse(&legacy).unwrap(), legacy);
}
struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(temp.starts_with(root), "parent must set owned TMPDIR");
        let path = temp.join(format!("hair-aperture-{label}-{}", std::process::id()));
        fs::create_dir(&path).expect("exclusive fixture, never reuse leftovers");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn material_application_treats_every_segment_face_without_rewriting_geometry() {
    let f = Fixture::new("materials");
    let pack = f.0.join("pack.toml");
    fs::write(&pack, "version=3\nname='segmented walls'\nroles=[]\n[[surfaces]]\nsurface='floor'\ntint=[0.2,0.3,0.4]\nmeters_per_tile=2.0\n[[surfaces]]\nsurface='wall'\ntint=[0.7,0.6,0.5]\nmeters_per_tile=0.5\n").unwrap();
    let h = host();
    assert_eq!(h.walls().len(), 9, "three jamb/jamb/lintel triples");
    let shell = h.vertices();
    let theme = Theme::load_for_folder_scene(&pack, &mut AssetCache::default(), &h).unwrap();
    assert_eq!(theme.host, h);
    let end = (1 + h.walls().len()) * 36;
    assert_eq!(shell.len(), end);
    for (i, (actual, original)) in theme.vertices[..end].iter().zip(&shell).enumerate() {
        assert_eq!(
            actual.position.map(f32::to_bits),
            original.position.map(f32::to_bits)
        );
        assert_eq!(
            actual.color,
            if i < 36 {
                [0.2, 0.3, 0.4]
            } else {
                [0.7, 0.6, 0.5]
            }
        );
        assert_eq!(actual.texture, 0);
        assert!(actual.uv.iter().all(|v| v.is_finite()));
    }
    for (i, face) in theme.vertices[..end].as_chunks::<6>().0.iter().enumerate() {
        let a = glam::Vec3::from_array(face[1].position) - glam::Vec3::from_array(face[0].position);
        let b = glam::Vec3::from_array(face[2].position) - glam::Vec3::from_array(face[0].position);
        let n = a.cross(b).abs();
        for v in face {
            let p = v.position;
            let uv = if i < 6 || (n.y > n.x && n.y > n.z) {
                [p[0], p[2]]
            } else if n.x > n.z {
                [p[2], p[1]]
            } else {
                [p[0], p[1]]
            };
            let tile = if i < 6 { 2.0 } else { 0.5 };
            assert_eq!(v.uv, uv.map(|v| v / tile), "segmented face {i}");
        }
    }
}
fn place(vertices: &mut [Vertex], a: &Anchor, n: [f32; 2]) {
    for v in vertices {
        let [x, y, z] = v.position;
        v.position = [
            a.at[0] + n[1] * x + n[0] * z,
            a.at[1] + y,
            a.at[2] - n[0] * x + n[1] * z,
        ];
    }
}
#[test]
fn canonical_custom_and_fallback_trim_follow_all_host_normals_and_guards() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let f = Fixture::new("trim");
    let model_path = root.join("examples/door-trim-starter/model.glb");
    let mut raw = assets::load(&model_path, 10.0).unwrap();
    let trim = door_trim::qualify(&raw, [0.0; 3], 0.0, 1.0)
        .expect("production starter satisfies existing triangle-slot guard");
    assert_eq!(trim.len(), raw.vertices.len());
    assert!(!trim.is_empty());
    fs::copy(&model_path, f.0.join("trim.glb")).unwrap();
    let h = host();
    for custom in [false, true] {
        let pack = f.0.join(if custom {
            "custom.toml"
        } else {
            "fallback.toml"
        });
        let declaration = if custom {
            "[[roles]]\nrole='door-trim'\nfile='trim.glb'\nmax_extent=10.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0,0,0]\n"
        } else {
            "roles=[]\n"
        };
        fs::write(
            &pack,
            format!("version=3\nname='oriented trim'\ndoor_trim=true\n{declaration}"),
        )
        .unwrap();
        let theme = Theme::load_for_folder_scene(&pack, &mut AssetCache::default(), &h).unwrap();
        assert_eq!(theme.host, h);
        assert!(!theme.load_failed);
        if custom {
            assert!(theme.loaded_roles.iter().any(|r| r == "door-trim"));
        } else {
            assert!(
                theme
                    .diagnostics
                    .iter()
                    .any(|d| d.starts_with("ROLE_FALLBACK door-trim absent"))
            );
        }
        let shell = h.vertices();
        assert!(
            theme
                .vertices
                .iter()
                .zip(&shell)
                .all(|(a, b)| a.position.map(f32::to_bits) == b.position.map(f32::to_bits))
        );
        let actual: Vec<_> = theme.vertices[shell.len()..]
            .iter()
            .filter(|v| v.color != [0.08, 0.6, 0.18])
            .copied()
            .collect();
        let mut expected = Vec::new();
        let mut normals = std::collections::BTreeSet::new();
        for a in h
            .anchors
            .iter()
            .filter(|a| a.role == "portal" && a.id != folder_scene::INERT_PORTAL)
        {
            let n = h.portal_inward(&a.id);
            normals.insert(n.map(|v| v as i32));
            let mut canonical = a.clone();
            canonical.at = [0.0; 3];
            if n[0] != 0.0 {
                canonical.size.swap(0, 2);
            }
            let start = expected.len();
            door_trim::frame(&mut expected, &canonical);
            assert_eq!(expected.len() - start, 3 * 36, "complete open host frame");
            if custom {
                door_trim::append(&mut expected, &trim, &canonical, 0);
            }
            place(&mut expected[start..], a, n);
        }
        assert_eq!(
            normals,
            [[0, 1], [0, -1], [1, 0], [-1, 0]].into_iter().collect()
        );
        assert_eq!(
            actual.len(),
            expected.len(),
            "complete frames/custom models, no shell/panel substitutes"
        );
        for (a, e) in actual.iter().zip(&expected) {
            assert!((0..3).all(|i| (a.position[i] - e.position[i]).abs() < 0.0001));
            assert_eq!(a.color, e.color);
            assert_eq!(a.uv, e.uv);
            assert_eq!(a.texture, e.texture);
        }
    }
    // Reuse actual decoded production model, but replace one triangle to test
    // the existing all-three-vertices-in-ONE-slot rule (not a new trim fitter).
    raw.vertices[0].position = [-0.45, 0.1, 0.0];
    raw.vertices[1].position = [0.45, 0.1, 0.0];
    raw.vertices[2].position = [0.0, 0.95, 0.0];
    assert!(door_trim::qualify(&raw, [0.0; 3], 0.0, 1.0).is_err());
    assert!(door_trim::qualify(&raw, [f32::NAN, 0.0, 0.0], 0.0, 1.0).is_err());
}
