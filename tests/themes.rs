use hair::{
    Action, Input, Pose, World,
    theme::{AssetCache, Theme},
};
use std::path::Path;
#[test]
fn two_role_packs_are_distinct_reachable_and_do_not_own_content_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cache = AssetCache::default();
    let office = Theme::load(&root.join("packs/office.toml"), &mut cache).unwrap();
    let archive = Theme::load(&root.join("packs/archive.toml"), &mut cache).unwrap();
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&office.vertices),
        bytemuck::cast_slice::<_, u8>(&archive.vertices)
    );
    let mut w = World::load_site(&root.join("examples/site-a")).unwrap();
    office.act(&mut w, Action::Navigate, office.portal);
    assert_eq!(w.current, "notes/workshop.md");
    office.act(&mut w, Action::Inspect, office.document);
    let before = (
        w.current.clone(),
        w.document().unwrap().to_owned(),
        w.target_description(),
        w.reading,
        w.selected,
    );
    let mut pose = Pose::default();
    archive.respawn(&mut pose);
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
    assert!(!archive.blocked(pose.position));
    let input = Input {
        forward: true,
        captured: true,
        ..Default::default()
    };
    for _ in 0..1000 {
        archive.locomote(&mut pose, &input, 1.0);
        assert!(!archive.blocked(pose.position));
    }
    assert!(Theme::load(&root.join("packs/missing.toml"), &mut cache).is_err());
    assert_eq!(w.current, "notes/workshop.md");
    assert!(cache.loads < 20, "shared models should load once");
}
