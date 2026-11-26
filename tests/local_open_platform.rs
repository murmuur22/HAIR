//! File-only tracer preparation: parent owns the actual compile/RED/GREEN receipts.
use hair::{
    Pose,
    assets::{self, Vertex},
    folder_scene,
    host_scene::{Anchor, HostScene},
    interaction,
    local_explorer::{Entry, Kind, Snapshot},
    theme::{AssetCache, Theme},
};
use std::path::Path;

const EPS: f32 = 0.0001;
const CUE: [f32; 3] = [0.08, 0.6, 0.18];

#[test]
fn open_platform_requires_explicit_version_two_host() {
    let mut legacy = HostScene::shared_default();
    assert!(!legacy.open_platform);
    legacy.open_platform = true;
    let text = toml::to_string(&legacy).unwrap();
    let result = HostScene::parse(&text);
    assert!(
        result.is_err(),
        "version-one host unexpectedly accepted new open-platform semantics"
    );
}

fn same_positions(actual: &[Vertex], expected: &[Vertex]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(a, b)| a.position.map(f32::to_bits) == b.position.map(f32::to_bits))
}

fn model_at(props: &[Vertex], a: &Anchor) -> Vec<Vertex> {
    // Only after the authoritative host mesh has been excluded: aperture jambs
    // cannot qualify as models, and panels/labels cannot pay missing model debt.
    props
        .iter()
        .filter(|v| {
            v.color != CUE
                && (v.position[0] - a.at[0]).abs() <= a.size[0] * 0.5 + 0.02
                && (a.at[1] - 0.02..=a.at[1] + a.size[1] + 0.02).contains(&v.position[1])
                && (v.position[2] - a.at[2]).abs() <= a.size[2] * 0.5 + 0.02
        })
        .copied()
        .collect()
}

fn in_legacy_wall_band(v: &Vertex, room: [f32; 4]) -> bool {
    // Spatial negative oracle, independent of walls(): doors are beyond these
    // narrow boundary bands. Check actual composed vertices, not a new flag.
    let [x, y, z] = v.position;
    let [left, front, right, rear] = room;
    y > 0.1
        && (((left - 0.31..=left - 0.09).contains(&x)
            || (right + 0.09..=right + 0.31).contains(&x))
            && (front - EPS..=rear + EPS).contains(&z)
            || (rear + 0.09..=rear + 0.31).contains(&z)
                && (left - 0.21..=right + 0.21).contains(&x))
}

#[test]
fn local_theme_is_open_platform_with_fitted_live_doors_and_legacy_host_walls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Names/paths are synthetic identities only: no enumeration or file bodies.
    // Middle page exercises front child, both side continuations and rear Parent.
    let snapshot = Snapshot {
        folder: "/OpenPlatformFixture".into(),
        entries: (0..25)
            .map(|i| Entry {
                name: format!("Entry {i:03}").into(),
                path: format!("/OpenPlatformFixture/Entry {i:03}").into(),
                kind: if i == 13 { Kind::File } else { Kind::Folder },
            })
            .collect(),
    };
    let (world, local_scene) = folder_scene::build(&snapshot, 1, false).unwrap();
    let view = world.folder_view.as_ref().unwrap();
    assert_eq!((view.page, view.pages(), view.count), (1, 3, 25));
    assert_eq!(view.bindings.len(), 12);
    assert_eq!(view.continuations.len(), 2);
    assert_eq!(view.parent.as_deref(), Some(folder_scene::PARENT_PORTAL));
    for bound in view.bindings.values() {
        assert_eq!(bound.entry, snapshot.entries[bound.index]);
    }
    // This fixture has 24 folders: inherited count-derived width is capped at
    // 28m. Stable map cards must not change these physical room dimensions.
    assert_eq!(local_scene.room, [-14.0, -6.0, 14.0, 6.0]);
    assert_eq!(local_scene.spawn, [0.0, 5.65]);
    assert!(local_scene.geometry.is_empty() && local_scene.overhead.is_empty());
    let anchor = |id: &str| local_scene.anchors.iter().find(|a| a.id == id).unwrap();
    let document = anchor("entry-1");
    assert_eq!(document.at, [-1.5, 0.0, -3.0]);
    assert_eq!(document.size, [0.8, 0.8, 0.6]);
    assert_eq!(document.binding, "explicit-page");

    let pack = root.join("packs/office.toml");
    let mut cache = AssetCache::default();
    // Negative control is a real generic scene, NOT a local scene with metadata
    // stripped: arbitrary hosted scenes and Workshop must retain default walls.
    let legacy = HostScene::shared_default();
    assert_eq!(legacy.walls().len(), 3);
    let legacy_mesh = legacy.vertices();
    assert_eq!(legacy_mesh.len(), 4 * 36);
    let generic = Theme::load_for_scene(&pack, &mut cache, &legacy).unwrap();
    assert_eq!(generic.host, legacy);
    assert!(generic.vertices.len() >= legacy_mesh.len());
    assert!(same_positions(
        &generic.vertices[..legacy_mesh.len()],
        &legacy_mesh
    ));
    assert!(
        generic
            .vertices
            .iter()
            .any(|v| in_legacy_wall_band(v, legacy.room))
    );

    let theme = Theme::load_for_folder_scene(&pack, &mut cache, &local_scene).unwrap();
    assert_eq!(theme.host, local_scene);
    assert!(!theme.load_failed);
    assert!(theme.loaded_roles.iter().any(|r| r == "portal"));
    assert!(
        theme
            .diagnostics
            .iter()
            .all(|d| !d.starts_with("ROLE_FALLBACK"))
    );
    let host_mesh = local_scene.vertices();
    assert!(host_mesh.len() >= 36 && theme.vertices.len() >= host_mesh.len());
    assert!(same_positions(
        &theme.vertices[..host_mesh.len()],
        &host_mesh
    ));
    let visible = theme.vertices_for(&world);
    assert!(
        visible.len() > theme.vertices.len(),
        "live door/file labels must remain"
    );
    assert!(same_positions(&visible[..host_mesh.len()], &host_mesh));
    // File-item substitution replaces only the old marker, not stationary bodies.
    for a in local_scene.anchors.iter().filter(|a| a.role == "document") {
        for vertex in theme.collection_shape(&a.id) {
            assert!(
                visible
                    .iter()
                    .any(|v| bytemuck::bytes_of(v) == bytemuck::bytes_of(&vertex))
            );
        }
    }

    // Actual retained floor: six faces, finite vertices, full XZ platform and
    // original thickness. Do not pass by making the entire scene invisible.
    let floor = &host_mesh[..36];
    let [left, front, right, rear] = local_scene.room;
    let low = [left - 0.2, -0.05, front - 0.2];
    let high = [right + 0.2, 0.05, rear + 0.2];
    for axis in 0..3 {
        let min = floor
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::INFINITY, f32::min);
        let max = floor
            .iter()
            .map(|v| v.position[axis])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min - low[axis]).abs() < EPS && (max - high[axis]).abs() < EPS);
    }
    assert!(
        visible
            .iter()
            .all(|v| v.position.iter().all(|p| p.is_finite()))
    );
    assert!(!theme.blocked(local_scene.spawn));
    for outside in [
        [left - 0.01, 0.0],
        [right + 0.01, 0.0],
        [0.0, front - 0.01],
        [0.0, rear + 0.01],
    ] {
        assert!(
            theme.blocked(outside),
            "open visuals must not remove floor-edge bounds"
        );
    }

    let raw = assets::load(&root.join("packs/wallDoorway.glb"), 3.0).unwrap();
    assert!(!raw.vertices.is_empty());
    let props = &theme.vertices[host_mesh.len()..];
    let canonical_anchor = anchor("entry-0");
    let canonical = model_at(props, canonical_anchor);
    assert_eq!(
        canonical.len(),
        raw.vertices.len(),
        "complete actual front model"
    );
    for (id, at, size, inward) in [
        ("entry-0", [-8.8, 0.0, -6.5], [1.45, 2.0, 0.6], [0.0, 1.0]),
        (
            "folder-next",
            [14.7, 0.0, 0.0],
            [0.6, 2.0, 1.45],
            [-1.0, 0.0],
        ),
        (
            "folder-previous",
            [-14.7, 0.0, 0.0],
            [0.6, 2.0, 1.45],
            [1.0, 0.0],
        ),
        (
            folder_scene::PARENT_PORTAL,
            [0.0, 0.0, 6.7],
            [1.45, 2.0, 0.6],
            [0.0, -1.0],
        ),
    ] {
        let a = anchor(id);
        assert_eq!(a.at, at);
        assert_eq!(a.size, size);
        assert_eq!(a.binding, "explicit-door");
        assert_eq!(local_scene.portal_inward(id), inward);
        let mesh = model_at(props, a);
        assert_eq!(
            mesh.len(),
            raw.vertices.len(),
            "complete actual model at {id}"
        );
        let min_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (min_y - a.at[1]).abs() < 0.02 && max_y > 1.65,
            "fitted standing door at {id}"
        );
        for (v, c) in mesh.iter().zip(&canonical) {
            let p = std::array::from_fn::<_, 3, _>(|i| c.position[i] - canonical_anchor.at[i]);
            let expected = [
                a.at[0] + inward[1] * p[0] + inward[0] * p[2],
                a.at[1] + p[1],
                a.at[2] - inward[0] * p[0] + inward[1] * p[2],
            ];
            assert!((0..3).all(|i| (v.position[i] - expected[i]).abs() < EPS));
        }
        let cues: Vec<_> = props
            .iter()
            .filter(|v| {
                v.color == CUE
                    && (v.position[0] - a.at[0]).abs() < 0.5
                    && (v.position[2] - a.at[2]).abs() < 0.5
            })
            .collect();
        assert_eq!(cues.len(), 36, "actual inward cue panel at {id}");
        let normal_depth = if inward[0] == 0.0 {
            a.size[2]
        } else {
            a.size[0]
        };
        for axis in [0, 2] {
            let mean = cues.iter().map(|v| v.position[axis]).sum::<f32>() / cues.len() as f32;
            let n = inward[if axis == 0 { 0 } else { 1 }];
            assert!((mean - a.at[axis] - n * (normal_depth * 0.5 + 0.02)).abs() < EPS);
        }
        // Aim exactly at the real host target; no undocumented pitch contract.
        let distance = 1.25;
        let pose = Pose {
            eye_offset: 1.65,
            feet_y: 0.0,
            position: [
                a.at[0] + inward[0] * distance,
                a.at[2] + inward[1] * distance,
            ],
            yaw: (-inward[0]).atan2(inward[1]),
            pitch: ((a.at[1] + a.size[1] * 0.5 - 1.65) / distance).atan(),
        };
        assert!(!theme.blocked(pose.position));
        let target = interaction::resolve(&local_scene, &pose, &world).unwrap();
        assert_eq!(target.anchor, id);
        assert_eq!(target.kind, interaction::Kind::Enter);
        if let Some(bound) = view.bindings.get(id) {
            assert_eq!(
                target.destination,
                Some(folder_scene::display(bound.entry.path.as_os_str()))
            );
        } else {
            assert!(target.destination.is_none());
        }
    }

    // Intended baseline failure is collected LAST, after the retained floor,
    // complete live door meshes/cues, targeting and generic-host controls run.
    // walls() alone is insufficient: inspect both actual host and Theme consumers.
    let wall_band_vertices = visible
        .iter()
        .filter(|v| in_legacy_wall_band(v, local_scene.room))
        .count();
    assert!(
        local_scene.walls().is_empty() && host_mesh.len() == 36 && wall_band_vertices == 0,
        "local open platform still has enclosing-shell geometry: walls={} host_vertices={} composed_wall_band_vertices={wall_band_vertices}; expected 0, 36, 0",
        local_scene.walls().len(),
        host_mesh.len()
    );
}
