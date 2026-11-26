//! Disposable explicit-root, actual App input vertical. Never reads HOME.
mod qualification;
mod boundary;
mod asynchronous;
use super::*;
use std::os::unix::fs::DirBuilderExt;
struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("storage-documents-{label}-{}", std::process::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn directory(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); }
}
fn app() -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    app_at_resources(&root)
}
fn app_at_resources(resources: &Path) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let world = World::load(&root).unwrap();
    App::new(resources.into(), vec![theme], 0, world, false)
}
fn tap(a: &mut App, key: KeyCode) {
    drain(a);
    assert!(!a.keyboard_input(Some(key), true, false, None).unwrap());
    assert!(!a.keyboard_input(Some(key), false, false, None).unwrap());
    drain(a);
}
/// Explicit fixture driver for the SAME production poll hook used by redraw.
fn drain(a: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.inventory.is_busy() {
        assert!(Instant::now() < deadline, "Documents worker stuck: {}", a.local_status);
        a.poll_documents();
        std::thread::yield_now();
    }
}
fn aim(a: &mut App) {
    let anchor = a.themes[0].host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
    a.pose = Pose {
        position: [anchor.at[0], anchor.at[2] + 1.05], yaw: 0.0,
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
        ..Default::default()
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
}
#[test]
fn storage_documents_input_collect_reopen_visible_confirm_remove_reopen() {
    let f = Fixture::new("tracer");
    let cache = f.directory("cache");
    let source = f.directory("source");
    let room = f.directory("independent-room");
    let body = b"original and independent room remain intact";
    std::fs::write(source.join("Note.txt"), body).unwrap();
    std::fs::write(room.join("Note.txt"), body).unwrap();
    let mut a = app();
    a.restore_documents_at(&cache).unwrap();
    a.complete_folder(Ok(Some(source.clone()))).unwrap();
    aim(&mut a);
    tap(&mut a, KeyCode::KeyX);
    assert_eq!(a.inventory.items().len(), 1, "actual input must collect: {}", a.local_status);
    let id = a.inventory.items()[0].id;
    drop(a);
    let mut b = app();
    b.restore_documents_at(&cache).unwrap();
    tap(&mut b, KeyCode::KeyB);
    assert_eq!(b.inventory.items().len(), 1, "fresh App must list committed disk metadata");
    assert_eq!(b.inventory.items()[0].id, id);
    assert_eq!(b.inventory.items()[0].name, "Note.txt");
    assert!(!b.inventory_overlay_for([640, 480]).unwrap().is_empty());
    tap(&mut b, KeyCode::Delete);
    assert_eq!(b.inventory.items().len(), 1, "planning is not deletion");
    tap(&mut b, KeyCode::Enter);
    assert!(b.inventory.items().is_empty(), "fresh confirmation removes only selected cache record: {}", b.local_status);
    drop(b);
    let mut c = app();
    c.restore_documents_at(&cache).unwrap();
    assert!(c.inventory.items().is_empty());
    assert_eq!(std::fs::read(source.join("Note.txt")).unwrap(), body);
    assert_eq!(std::fs::read(room.join("Note.txt")).unwrap(), body);
}
