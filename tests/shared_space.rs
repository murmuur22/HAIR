use hair::{
    Action, Pose, World,
    theme::{AssetCache, Theme},
};
use std::path::Path;

#[test]
fn same_host_two_skins_preserve_markers_collision_and_content() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cache = AssetCache::default();
    let a = Theme::load(&root.join("packs/office.toml"), &mut cache).unwrap();
    let b = Theme::load(&root.join("packs/archive.toml"), &mut cache).unwrap();
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&a.vertices),
        bytemuck::cast_slice::<_, u8>(&b.vertices)
    );
    assert_eq!(a.document, b.document);
    assert_eq!(a.portal, b.portal);
    for x in -16..=16 {
        for z in -16..=16 {
            let p = [x as f32 * 0.25, z as f32 * 0.25];
            assert_eq!(a.blocked(p), b.blocked(p), "shared navigation at {p:?}");
        }
    }
    let mut w = World::load_site(&root.join("examples/site-a")).unwrap();
    a.act(&mut w, Action::Inspect, a.document);
    let before = (
        w.current.clone(),
        w.document().unwrap().to_owned(),
        w.target_description(),
        w.reading,
        w.selected,
    );
    let pose = Pose {
        position: [2.0, 2.0],
        yaw: 0.25,
        pitch: -0.2,
    };
    // Switching appearance must not require respawn, and both skins permit the same pose.
    assert!(!a.blocked(pose.position));
    assert!(!b.blocked(pose.position));
    b.act(&mut w, Action::Inspect, b.document);
    assert_eq!(
        before,
        (
            w.current.clone(),
            w.document().unwrap().to_owned(),
            w.target_description(),
            w.reading,
            w.selected
        )
    );
}

#[test]
#[ignore = "finite actual executable offscreen Metal tracer"]
fn executable_reskin_keeps_pose_scene_anchors_and_collision() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = std::env::temp_dir().join(format!("hair-shared-tracer-{}", std::process::id()));
    std::fs::create_dir(&out).unwrap();
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--site")
        .arg(root.join("examples/site-a"))
        .args(["--headless-smoke", "--smoke-output"])
        .arg(&out)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&p.stdout),
        String::from_utf8_lossy(&p.stderr)
    );
    println!("{text}\nTRACER_CAPTURES={}", out.display());
    assert!(p.status.success(), "{text}");
    assert!(
        text.contains("SHARED_RESKIN_PRESERVED pose=true scene=true anchors=true collision=true"),
        "{text}"
    );
    assert_ne!(
        std::fs::read(out.join("office.png")).unwrap(),
        std::fs::read(out.join("reader-workshop.png")).unwrap()
    );
}
