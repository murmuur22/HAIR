use hair::{
    Pose,
    host_scene::HostScene,
    theme::{AssetCache, Theme},
};
use std::{fs, path::Path};
#[test]
fn explicit_host_scene_controls_all_skins_and_malformed_intent_refuses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-host-load-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    fs::write(base.join("index.md"), "# Read only\n[next](next.md)\n").unwrap();
    fs::write(base.join("next.md"), "# Next\n").unwrap();
    let default = fs::read_to_string(root.join("host-scene.toml")).unwrap();
    let custom = default
        .replace("Shared demo room", "Host fixture")
        .replace("at = [0.0, 0.1, 0.3]", "at = [1.0, 0.1, 0.3]")
        .replace(
            "geometry = []",
            "geometry = [{at=[-2.0,0.1,-1.0],size=[0.8,1.0,0.8]}]",
        );
    fs::write(base.join("host-scene.toml"), &custom).unwrap();
    let h = HostScene::load_site(&base).unwrap();
    let mut cache = AssetCache::default();
    let a = Theme::load_for_scene(&root.join("packs/office.toml"), &mut cache, &h).unwrap();
    let b = Theme::load_for_scene(&root.join("packs/archive.toml"), &mut cache, &h).unwrap();
    assert_eq!(a.document, [1.0, 0.3]);
    assert_eq!(a.host, b.host);
    assert!(a.blocked([-2.0, -1.0]));
    let markers = |t: &Theme| {
        t.vertices
            .iter()
            .filter(|v| v.color == [0.1, 0.8, 0.3] || v.color == [0.08, 0.6, 0.18])
            .map(|v| v.position)
            .collect::<Vec<_>>()
    };
    assert_eq!(markers(&a), markers(&b));
    let pose = Pose {
        position: [2.0, 2.0],
        yaw: 0.5,
        pitch: -0.4,
    };
    b.reskin_from(&a, &pose).unwrap();
    let other = Theme::load(&root.join("packs/office.toml"), &mut cache).unwrap();
    assert!(b.reskin_from(&other, &pose).is_err());
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_hair"))
            .env("HAIR_RESOURCE_ROOT", root)
            .arg("--site")
            .arg(&base)
            .arg("--resource-report")
            .output()
            .unwrap()
    };
    let p = run();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert!(String::from_utf8_lossy(&p.stdout).contains("HOST_SCENE=Host fixture"));
    for bad in [
        custom.replace("version = 1", "version = 9"),
        custom.replace("spawn = [0.0, 3.0]", "spawn = [nan, 3.0]"),
        custom.replace("spawn = [0.0, 3.0]", "spawn = [1.0, 0.3]"),
        custom.replace("id = \"main-link\"", "id = \"main-document\""),
        custom.replace("current-document", "script"),
        format!("{custom}\n[[geometry]]\nat=[0.0,0.0,0.0]\nsize=[8.0,1.0,8.0]\n"),
        "x".repeat(32769),
    ] {
        fs::write(base.join("host-scene.toml"), bad.as_bytes()).unwrap();
        let p = run();
        assert!(!p.status.success());
        let error = String::from_utf8_lossy(&p.stderr);
        assert!(error.contains("host scene"), "{error}");
        assert_eq!(
            fs::read(base.join("host-scene.toml")).unwrap(),
            bad.as_bytes()
        );
    }
    fs::remove_file(base.join("host-scene.toml")).unwrap();
    std::os::unix::fs::symlink(root.join("host-scene.toml"), base.join("host-scene.toml")).unwrap();
    assert!(!run().status.success());
    fs::remove_file(base.join("host-scene.toml")).unwrap();
    assert_eq!(
        HostScene::load_site(&base).unwrap(),
        HostScene::shared_default()
    );
    assert!(!base.join("host-scene.toml").exists());
    assert_eq!(
        fs::read(base.join("index.md")).unwrap(),
        b"# Read only\n[next](next.md)\n"
    );
    fs::remove_dir_all(base).unwrap();
}
