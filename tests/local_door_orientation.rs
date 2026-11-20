use hair::{
    assets::{self, Vertex},
    folder_scene::{self, Continuation},
    host_scene::Anchor,
    local_explorer::{Entry, Kind, Snapshot},
    theme::{AssetCache, Theme},
};
use std::{collections::BTreeMap, path::Path};

// Subtract the unchanged authoritative shell, including aperture jambs/lintels.
// Identity here is exact position bits plus multiplicity: surface materials may
// legitimately change shell color/UV/texture, never its geometry. Check the
// production shell prefix first so a model cannot accidentally pay a shell debt.
fn without_shell(actual: &[Vertex], shell: &[Vertex]) -> Vec<Vertex> {
    assert!(actual.len() >= shell.len(), "Theme lost the host shell");
    let key = |v: &Vertex| v.position.map(f32::to_bits);
    assert!(
        actual.iter().zip(shell).all(|(a, b)| key(a) == key(b)),
        "Theme shell geometry/order changed; mesh isolation is invalid"
    );
    let mut counts = BTreeMap::<[u32; 3], usize>::new();
    for v in shell {
        *counts.entry(key(v)).or_default() += 1;
    }
    let mut added = Vec::new();
    for v in actual {
        if let Some(n) = counts.get_mut(&key(v))
            && *n > 0
        {
            *n -= 1;
        } else {
            added.push(*v);
        }
    }
    assert!(counts.values().all(|n| *n == 0));
    added
}

fn mesh_at(vertices: &[Vertex], a: &Anchor) -> Vec<Vertex> {
    // All other anchors are far outside this envelope. This is only applied
    // AFTER shell subtraction: nearby aperture solids must not qualify as doors.
    // Count against the bundled model below to detect clipping/marker substitutes.
    vertices
        .iter()
        .filter(|v| {
            v.color != [0.08, 0.6, 0.18]
                && (v.position[0] - a.at[0]).abs() <= a.size[0] * 0.5 + 0.02
                && (a.at[1] - 0.02..=a.at[1] + a.size[1] + 0.02).contains(&v.position[1])
                && (v.position[2] - a.at[2]).abs() <= a.size[2] * 0.5 + 0.02
        })
        .copied()
        .collect()
}

fn bounds(mesh: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for v in mesh {
        assert!(v.position.iter().all(|p| p.is_finite()));
        for axis in 0..3 {
            min[axis] = min[axis].min(v.position[axis]);
            max[axis] = max[axis].max(v.position[axis]);
        }
    }
    (min, max)
}

#[test]
fn fitted_side_portals_are_human_scale_rigid_rotations_of_front_mesh() {
    const EYE: f32 = 1.65; // Production renderer/route eye height.
    const EPS: f32 = 0.0001;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // In-memory names/paths only; no filesystem enumeration or fixture creation.
    let snapshot = Snapshot {
        folder: "/OrientationFixture".into(),
        entries: (0..25)
            .map(|i| Entry {
                name: format!("Child {i:03}").into(),
                path: format!("/OrientationFixture/Child {i:03}").into(),
                kind: Kind::Folder,
            })
            .collect(),
    };
    let (world, host) = folder_scene::build(&snapshot, 1, false).unwrap();
    let view = world.folder_view.as_ref().unwrap();
    assert_eq!((view.page, view.pages(), view.count), (1, 3, 25));
    assert_eq!(view.continuations.len(), 2);
    assert_eq!(view.bindings.len(), 12);
    let front = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
    let shell = host.vertices();
    let mut cache = AssetCache::default();
    let mut failures = Vec::new();
    for (pack, file) in [
        ("office", "wallDoorway.glb"),
        ("archive", "bookcaseOpen.glb"),
    ] {
        let raw = assets::load(&root.join("packs").join(file), 3.0).unwrap();
        assert!(!raw.vertices.is_empty());
        let theme = Theme::load_for_folder_scene(
            &root.join(format!("packs/{pack}.toml")),
            &mut cache,
            &host,
        )
        .unwrap();
        assert_eq!(
            theme.host, host,
            "appearance must not rewrite host geometry"
        );
        assert!(!theme.load_failed);
        assert!(theme.loaded_roles.iter().any(|r| r == "portal"));
        assert!(
            theme
                .diagnostics
                .iter()
                .all(|d| !d.starts_with("ROLE_FALLBACK")),
            "bundled assets must load, not exercise placeholders"
        );
        // Static Theme only: dynamic labels/highlights cannot fake mesh height.
        let props = without_shell(&theme.vertices, &shell);
        let canonical = mesh_at(&props, front);
        assert_eq!(
            canonical.len(),
            raw.vertices.len(),
            "{pack}: canonical crop must contain the complete actual model, no panel"
        );
        for (id, action) in &view.continuations {
            assert!(action.page(view).is_some());
            let a = host.anchors.iter().find(|a| &a.id == id).unwrap();
            assert_eq!(a.role, "portal");
            let sign = match action {
                Continuation::Previous => {
                    assert!(a.at[0] < host.room[0]);
                    1.0
                }
                Continuation::Next => {
                    assert!(a.at[0] > host.room[2]);
                    -1.0
                }
            };
            assert_eq!(host.portal_inward(id), [sign, 0.0]);
            let mesh = mesh_at(&props, a);
            let (min, max) = bounds(&mesh);
            let depth = max[0] - min[0];
            let height = max[1] - min[1];
            let width = max[2] - min[2];
            println!(
                "SIDE_PORTAL_FITTED {pack}/{id} vertices={}/{} envelope={:?} min={min:?} max={max:?} tangent_z={width} height={height} normal_x={depth} eye={EYE}",
                mesh.len(),
                raw.vertices.len(),
                a.size
            );
            if mesh.len() != raw.vertices.len()
                || !(EYE * 1.05..=a.size[1] + 0.02).contains(&height)
                || !(EYE * 0.45..=a.size[2] + 0.02).contains(&width)
                || !(0.0..=a.size[0] + 0.02).contains(&depth)
                || width <= depth
                || (min[1] - a.at[1]).abs() > 0.02
                || max[1] <= a.at[1] + EYE
            {
                failures.push(format!(
                    "{pack}/{id}: complete human-scale side mesh required; vertices={}/{}, tangent_z={width}, height={height}, normal_x={depth}, bottom={} (eye={EYE})",
                    mesh.len(), raw.vertices.len(), min[1]
                ));
            }
            // Rigid quarter-turn of the actual front fit, not a second copy of
            // the fitter. Canonical +Z faces inward; left maps it to +X and
            // right to -X. Preserve every vertex, color and UV in model order.
            // Texture indices are scene-relative, so only compare untextured vs
            // textured here (the two bundled models currently need no remap).
            let rigid = mesh.len() == canonical.len()
                && mesh.iter().zip(&canonical).all(|(side, front_vertex)| {
                    let p =
                        std::array::from_fn::<_, 3, _>(|i| front_vertex.position[i] - front.at[i]);
                    let expected = [a.at[0] + sign * p[2], a.at[1] + p[1], a.at[2] - sign * p[0]];
                    (0..3).all(|i| (side.position[i] - expected[i]).abs() <= EPS)
                        && side.color == front_vertex.color
                        && side.uv == front_vertex.uv
                        && (side.texture == 0) == (front_vertex.texture == 0)
                });
            if !rigid {
                failures.push(format!(
                    "{pack}/{id}: actual model is not the rigid inward quarter-turn of its canonical front fit (pack yaw/scale/uniform fit must be preserved)"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "actual side portal mesh fitting/orientation is wrong (shell, green panel and labels excluded): {failures:#?}"
    );
}
