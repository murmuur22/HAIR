use hair::{
    assets::{self, Vertex},
    folder_scene,
    host_scene::HostScene,
    local_explorer::{Entry, Kind, Snapshot},
    theme::{AssetCache, Theme},
};
use std::{collections::BTreeMap, path::Path};

// These are in-memory snapshots, not directories to enumerate or paths to open.
// The names are valid APFS Unicode/control-name cases; no invalid UTF-8 fixture.
fn snapshot(child: bool) -> Snapshot {
    let folder = if child { "/Fixture" } else { "/" };
    Snapshot {
        folder: folder.into(),
        entries: if child {
            vec![Entry {
                name: "Child\nMiXeD-É".into(),
                path: "/Fixture/Child\nMiXeD-É".into(),
                kind: Kind::Folder,
            }]
        } else {
            Vec::new()
        },
    }
}

#[test]
fn empty_local_theme_has_no_visible_inert_vocabulary_props() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (world, host) = folder_scene::build(&snapshot(false), 0, false).unwrap();
    let view = world.folder_view.as_ref().unwrap();
    assert!(view.bindings.is_empty() && view.continuations.is_empty() && view.parent.is_none());
    // Keep the production role/schema validator. Sentinels may remain in Host;
    // the contract is absence of their geometry in the actual local Theme.
    HostScene::parse(&toml::to_string(&host).unwrap()).unwrap();
    let shell = host.vertices();
    let mut cache = AssetCache::default();
    let mut failures = Vec::new();
    for pack in ["office", "archive"] {
        let path = root.join(format!("packs/{pack}.toml"));
        let local = Theme::load_for_folder_scene(&path, &mut cache, &host).unwrap();
        assert_eq!(
            local.host, host,
            "appearance must not rewrite validated host"
        );
        assert!(local.loaded_roles.iter().any(|r| r == "document"));
        assert!(local.loaded_roles.iter().any(|r| r == "portal"));
        assert!(!local.load_failed);
        assert!(
            local
                .diagnostics
                .iter()
                .all(|d| !d.starts_with("ROLE_FALLBACK"))
        );
        let visible = local.vertices_for(&world);
        // R8 adds a cosmetic suffix, not inert vocabulary props. Preserve the
        // exact shell prefix and require every added triangle on a metre-high edge.
        let boundary_only = local.vertices[shell.len()..].as_chunks::<3>().0.iter().all(|tri| {
            tri.iter().all(|v| {
                (0.049..=1.001).contains(&v.position[1]) && v.texture == 0 && v.uv == [0.0; 2]
            }) && [(0, 0), (0, 2), (2, 1), (2, 3)]
                .iter()
                .any(|&(axis, bound)| {
                    tri.iter()
                        .all(|v| (v.position[axis] - host.room[bound]).abs() <= 0.101)
                })
        });
        if bytemuck::cast_slice::<_, u8>(&local.vertices[..shell.len()])
            != bytemuck::cast_slice::<_, u8>(&shell)
            || bytemuck::cast_slice::<_, u8>(&visible)
                != bytemuck::cast_slice::<_, u8>(&local.vertices)
            || !boundary_only
        {
            failures.push(format!(
                "{pack}: shell={} static={} world={}",
                shell.len(),
                local.vertices.len(),
                visible.len()
            ));
        }
        // Negative control: generic authored hosts still apply their vocabulary.
        // Local omission must not silently disable the shared Workshop/site path.
        let authored = Theme::load_for_scene(&path, &mut cache, &host).unwrap();
        assert!(authored.vertices.len() > shell.len());
        assert_eq!(authored.host, host);
    }
    assert!(
        failures.is_empty(),
        "local Theme still draws unbound vocabulary document/image/portal geometry: {failures:?}"
    );
}

// Preserve EVERY anchor/aperture and validate the complete host. Subtract its
// exact authoritative segmented shell with multiplicity; shell material fields
// may vary, positions/order may not. Models cannot pay missing shell debt.
fn without_shell(actual: &[Vertex], shell: &[Vertex]) -> Vec<Vertex> {
    let key = |v: &Vertex| v.position.map(f32::to_bits);
    assert!(actual.len() >= shell.len());
    assert!(actual.iter().zip(shell).all(|(a, b)| key(a) == key(b)));
    let mut counts = BTreeMap::new();
    for v in shell {
        *counts.entry(key(v)).or_insert(0_usize) += 1;
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
    assert!(
        counts.values().all(|n| *n == 0),
        "complete shell subtraction"
    );
    added
}
fn mesh_at(props: &[Vertex], a: &hair::host_scene::Anchor) -> Vec<Vertex> {
    props
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

#[test]
fn fitted_local_portal_mesh_is_human_scale_relative_to_visitor_eye() {
    // VisitorEyeDoorBand proposal: top above the production 1.65m eye, not a
    // waist-high miniature; width near a human doorway, below the 1.6m slot pitch.
    // This is an explicit geometry proposal, not native comfort certification.
    const EYE: f32 = 1.65; // Renderer::draw and HostScene route checks.
    const MIN_HEIGHT: f32 = EYE * 1.05;
    const MAX_HEIGHT: f32 = EYE * 1.5;
    const MIN_WIDTH: f32 = EYE * 0.45;
    const MAX_WIDTH: f32 = EYE * 0.9;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (_, host) = folder_scene::build(&snapshot(true), 0, false).unwrap();
    assert_eq!(
        host.anchors
            .iter()
            .filter(|a| a.id == "entry-0" || a.id == folder_scene::PARENT_PORTAL)
            .count(),
        2,
        "fixture must exercise both child and Parent, never a vacuous portal loop"
    );
    let mut cache = AssetCache::default();
    let mut failures = Vec::new();
    for (pack, file) in [
        ("office", "wallDoorway.glb"),
        ("archive", "bookcaseOpen.glb"),
    ] {
        // Inspect actual decoded model vertices/bounds, not manifest max_extent.
        let raw = assets::load(&root.join("packs").join(file), 3.0).unwrap();
        assert!(!raw.vertices.is_empty());
        println!(
            "PORTAL_ASSET {pack} {file} vertices={} min={:?} max={:?}",
            raw.vertices.len(),
            raw.min,
            raw.max
        );
        let path = root.join(format!("packs/{pack}.toml"));
        let actual = Theme::load_for_folder_scene(&path, &mut cache, &host).unwrap();
        assert!(!actual.load_failed);
        assert!(
            actual
                .diagnostics
                .iter()
                .all(|d| !d.starts_with("ROLE_FALLBACK"))
        );
        assert_eq!(actual.host, host);
        HostScene::parse(&toml::to_string(&host).unwrap()).unwrap();
        let props = without_shell(&actual.vertices, &host.vertices());
        let front = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
        let canonical = mesh_at(&props, front);
        assert_eq!(canonical.len(), raw.vertices.len(), "complete front model");
        // Both complete live portal models, with the unchanged shell/apertures.
        for anchor in host.anchors.iter().filter(|a| {
            a.role == "portal" && (a.id == "entry-0" || a.id == folder_scene::PARENT_PORTAL)
        }) {
            let mesh = mesh_at(&props, anchor);
            assert_eq!(
                mesh.len(),
                raw.vertices.len(),
                "complete decoded {pack}/{} model, panel excluded",
                anchor.id
            );
            let n = host.portal_inward(&anchor.id);
            for (v, c) in mesh.iter().zip(&canonical) {
                let p = std::array::from_fn::<_, 3, _>(|i| c.position[i] - front.at[i]);
                let expected = [
                    anchor.at[0] + n[1] * p[0] + n[0] * p[2],
                    anchor.at[1] + p[1],
                    anchor.at[2] - n[0] * p[0] + n[1] * p[2],
                ];
                assert!(
                    (0..3).all(|i| (v.position[i] - expected[i]).abs() < 0.0001),
                    "complete rigid rear/front fit"
                );
                assert_eq!(v.color, c.color);
                assert_eq!(v.uv, c.uv);
                assert_eq!(v.texture == 0, c.texture == 0);
            }
            assert!(!mesh.is_empty(), "missing fitted {pack}/{} mesh", anchor.id);
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for v in mesh {
                assert!(v.position.iter().all(|x| x.is_finite()));
                for axis in 0..3 {
                    min[axis] = min[axis].min(v.position[axis]);
                    max[axis] = max[axis].max(v.position[axis]);
                }
            }
            let width = max[0] - min[0];
            let height = max[1] - min[1];
            println!(
                "PORTAL_FITTED {pack}/{} envelope={:?} min={min:?} max={max:?} width={width} height={height} eye={EYE}",
                anchor.id, anchor.size
            );
            if !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height)
                || !(MIN_WIDTH..=MAX_WIDTH).contains(&width)
                || (min[1] - anchor.at[1]).abs() > 0.02
            {
                failures.push(format!(
                    "{pack}/{}: width={width} height={height} bottom={} (eye={EYE})",
                    anchor.id, min[1]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "fitted portal mesh is not human-scale (VisitorEyeDoorBand, panel/labels excluded): {failures:?}"
    );
}
