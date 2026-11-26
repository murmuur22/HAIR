use hair::{
    Pose,
    assets::Vertex,
    folder_scene,
    host_scene::Anchor,
    interaction,
    local_explorer::{Entry, Kind, Snapshot},
    theme::{AssetCache, Theme},
};
use std::path::Path;

fn near(v: &Vertex, a: &Anchor) -> bool {
    (v.position[0] - a.at[0]).abs() <= a.size[0] * 0.5 + 0.06
        && (v.position[2] - a.at[2]).abs() <= a.size[2] * 0.5 + 0.06
}

fn local(v: &Vertex, a: &Anchor, n: [f32; 2]) -> [f32; 3] {
    let x = v.position[0] - a.at[0];
    let z = v.position[2] - a.at[2];
    [
        n[1] * x - n[0] * z,
        v.position[1] - a.at[1],
        n[0] * x + n[1] * z,
    ]
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

#[test]
fn rear_mesh_and_all_portal_cues_share_the_prompt_wall_basis() {
    let snapshot = Snapshot {
        folder: "/CueFixture".into(),
        entries: (0..25)
            .map(|i| Entry {
                name: format!("Child {i:03}").into(),
                path: format!("/CueFixture/Child {i:03}").into(),
                kind: Kind::Folder,
            })
            .collect(),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (mut world, host) = folder_scene::build(&snapshot, 1, false).unwrap();
    let shell = host.vertices();
    let front = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
    let rear = host
        .anchors
        .iter()
        .find(|a| a.id == folder_scene::PARENT_PORTAL)
        .unwrap();
    let mut cache = AssetCache::default();
    for pack in ["office", "archive"] {
        let theme = Theme::load_for_folder_scene(
            &root.join(format!("packs/{pack}.toml")),
            &mut cache,
            &host,
        )
        .unwrap();
        assert_eq!(theme.host, host);
        assert!(
            theme
                .diagnostics
                .iter()
                .all(|d| !d.starts_with("ROLE_FALLBACK"))
        );
        assert!(
            theme
                .vertices
                .iter()
                .zip(&shell)
                .all(|(a, b)| a.position == b.position)
        );
        let props = &theme.vertices[shell.len()..];
        let mesh = |a: &Anchor| -> Vec<Vertex> {
            props
                .iter()
                .filter(|v| near(v, a) && v.color != [0.08, 0.6, 0.18])
                .copied()
                .collect()
        };
        let f = mesh(front);
        let r = mesh(rear);
        assert!(!f.is_empty());
        assert_eq!(f.len(), r.len());
        for (front_vertex, rear_vertex) in f.iter().zip(&r) {
            let f = local(front_vertex, front, [0.0, 1.0]);
            let r = local(rear_vertex, rear, [0.0, -1.0]);
            for i in 0..3 {
                close(f[i], r[i]);
            }
            assert_eq!(front_vertex.color, rear_vertex.color);
            assert_eq!(front_vertex.uv, rear_vertex.uv);
            assert_eq!(front_vertex.texture == 0, rear_vertex.texture == 0);
        }
        for id in [
            "entry-0",
            folder_scene::PARENT_PORTAL,
            "folder-previous",
            "folder-next",
        ] {
            let a = host.anchors.iter().find(|a| a.id == id).unwrap();
            let n = host.portal_inward(id);
            let panel: Vec<_> = props
                .iter()
                .filter(|v| near(v, a) && v.color == [0.08, 0.6, 0.18])
                .collect();
            assert_eq!(panel.len(), 36, "complete panel required for {pack}/{id}");
            for v in panel {
                let p = local(v, a, n);
                close(p[0].abs(), 0.225);
                close((p[1] - 1.0).abs(), 0.125);
                close((p[2] - 0.32).abs(), 0.01);
            }
            // Isolate only this door's labels without changing the validated host,
            // shell, static appearance, or eligibility rules.
            let saved = world.folder_view.clone();
            let view = world.folder_view.as_mut().unwrap();
            view.bindings.retain(|key, _| key == id);
            view.continuations.retain(|key, _| key == id);
            if view.parent.as_deref() != Some(id) {
                view.parent = None;
            }
            world.interaction = None;
            world.explicit_target = None;
            let labels = theme.vertices_for(&world);
            let labels = &labels[theme.vertices.len()..];
            assert!(!labels.is_empty(), "missing label for {id}");
            for v in labels {
                let p = local(v, a, n);
                close(p[2], 0.32);
                assert!(p[1] > 2.0 && p[1] < 2.3);
            }
            let tri = &labels[..3];
            let u =
                glam::Vec3::from_array(tri[1].position) - glam::Vec3::from_array(tri[0].position);
            let v =
                glam::Vec3::from_array(tri[2].position) - glam::Vec3::from_array(tri[0].position);
            // Preserve object_label's canonical clockwise winding (-Z), rather
            // than reflecting the glyphs to change their facing convention.
            assert!(u.cross(v).dot(glam::Vec3::new(n[0], 0.0, n[1])) < 0.0);
            let pose = Pose {
                eye_offset: 1.65,
                feet_y: 0.0,
                position: [a.at[0] + n[0] * 1.05, a.at[2] + n[1] * 1.05],
                yaw: (-n[0]).atan2(n[1]),
                pitch: ((1.0_f32 - 1.65) / 1.05).atan(),
            };
            assert!(!host.blocked(pose.position));
            interaction::refresh(&host, &pose, &mut world);
            let target = world
                .interaction
                .clone()
                .expect("real aimed prompt required");
            assert_eq!(target.anchor, id);
            // Production cycling selects the highlight; never synthesize a Target.
            interaction::cycle(&host, &pose, &mut world, false);
            assert_eq!(world.explicit_target.as_ref(), Some(&target));
            assert_eq!(world.interaction.as_ref(), Some(&target));
            let highlighted = theme.vertices_for(&world);
            assert_eq!(highlighted.len(), theme.vertices.len() + labels.len() + 72);
            let highlight = &highlighted[theme.vertices.len() + labels.len()..];
            for v in highlight {
                assert_eq!(v.color, [0.55, 0.95, 0.65]);
                let p = local(v, a, n);
                close((p[0].abs() - 0.25).abs(), 0.0125);
                close((p[1] - 1.0).abs(), 0.145);
                close((p[2] - 0.34).abs(), 0.12);
            }
            world.folder_view = saved;
            world.interaction = None;
            world.explicit_target = None;
        }
    }
}
