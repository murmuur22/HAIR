#![cfg(target_os = "macos")]
use hair::{local_explorer::Explorer, placement_store};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("hair-slice4b-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn dir(&self, name: &str) {
        std::fs::create_dir(self.0.join(name)).unwrap();
        std::fs::set_permissions(self.0.join(name), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn file(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).unwrap();
        std::fs::set_permissions(self.0.join(name), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); }
}
const ID: &str = "0123456789abcdef0123456789abcdef";
fn record() -> String {
    format!("version=1\nkind='decoration-follow-theme'\nplacement_id='{ID}'\ntag='hair.seating.chair'\ncollision=true\nat=[2.0,-1.0]\nyaw=0.5\nsize=[0.8,0.8,0.6]\n")
}
#[test]
fn slice4b_owned_decoration_record_reopens_without_a_source_file_or_xattr() {
    let f = Fixture::new();
    f.dir(".hair-layout");
    f.file(".hair-layout/lock", b"");
    let tx = format!(".hair-layout/dc-{ID}");
    f.dir(&tx);
    let path = format!("{tx}/decoration.toml");
    f.file(&path, record().as_bytes());
    let explorer = Explorer::new(&f.0).unwrap();
    let actual = placement_store::read_layout(&explorer);
    assert!(actual.is_ok(), "explicit owned decoration namespace must reopen without pretending to be a file: {actual:?}");
    let layout = actual.unwrap();
    assert!(layout.positions.is_empty());
    assert_eq!(layout.decorations, vec![toml::from_str::<placement_store::Decoration>(&record()).unwrap()]);
    assert_eq!(std::fs::read(f.0.join(path)).unwrap(), record().as_bytes());
}

fn decoration() -> placement_store::Decoration { toml::from_str(&record()).unwrap() }
fn place(e: &Explorer, d: &placement_store::Decoration) -> placement_store::Outcome {
    placement_store::place_decoration_checked(e, d, |_| Ok(()))
}
fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)> {
    use std::os::unix::fs::MetadataExt;
    fn walk(p: &std::path::Path, out: &mut std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)>) {
        for entry in std::fs::read_dir(p).unwrap() {
            let path = entry.unwrap().path();
            let m = std::fs::symlink_metadata(&path).unwrap();
            if m.is_dir() { walk(&path, out); }
            else if m.is_symlink() { out.insert(path.clone(), (m.ino(), std::fs::read_link(path).unwrap().as_os_str().as_encoded_bytes().to_vec())); }
            else { out.insert(path.clone(), (m.ino(), std::fs::read(path).unwrap())); }
        }
    }
    let mut out = Default::default(); walk(root, &mut out); out
}
#[test]
fn slice4b_two_same_tag_placements_commit_refresh_reopen_preserving_file_records_bytes_and_inodes() {
    use placement_store::{Outcome, Position};
    let f = Fixture::new();
    f.file("unrelated.txt", b"untouched original");
    let e = Explorer::new(&f.0).unwrap();
    let p = Position { at: [-3., 1.], yaw: 0.2, size: [0.8; 3], allow_overlap: false, surface: None, properties: Default::default() };
    assert_eq!(placement_store::publish(&e, std::ffi::OsStr::new("saved.txt"), b"original saved file", &p, &std::sync::atomic::AtomicBool::new(false)), Outcome::Committed { synced: true });
    let original = snapshot(&f.0);
    let mut a = decoration();
    let mut b = a.clone(); b.placement_id = "abcdef0123456789abcdef0123456789".into(); b.at = [-2., -1.]; b.yaw = -0.75; b.collision = false;
    let mut calls = 0;
    assert_eq!(placement_store::place_decoration_checked(&e, &a, |live| { calls += 1; assert!(live.decorations.is_empty()); assert!(placement_store::read_layout(&e).unwrap_err().contains("busy")); Ok(()) }), Outcome::Committed { synced: true });
    assert_eq!(calls, 2, "fresh final validation is required, not only initial review");
    assert_eq!(place(&e, &b), Outcome::Committed { synced: true });
    let mut reopened = Explorer::new(&f.0).unwrap(); reopened.refresh().unwrap();
    let saved = placement_store::read_layout(&reopened).unwrap();
    assert_eq!(saved.decorations, vec![a.clone(), b.clone()]);
    assert_eq!(saved.positions, vec![("saved.txt".into(), p.clone())]);
    for (name, value) in original { assert_eq!(snapshot(&f.0).get(&name), Some(&value), "unrelated source/old metadata changed: {name:?}"); }
    let before = snapshot(&f.0);
    assert!(matches!(place(&e, &a), Outcome::NotCommitted(_)), "same identity must never overwrite");
    assert_eq!(snapshot(&f.0), before);
    a.at = [4., 3.];
    assert!(matches!(placement_store::publish(&e, std::ffi::OsStr::new("forbidden.txt"), b"no", &p, &std::sync::atomic::AtomicBool::new(false)), Outcome::NotCommitted(_)));
    let target = placement_store::select_existing(&reopened, std::ffi::OsStr::new("saved.txt")).unwrap();
    assert!(matches!(placement_store::reposition_checked(&reopened, &target, &p, |_| Err("caller geometry refusal".into())), Outcome::NotCommitted(_)));
    assert_eq!(snapshot(&f.0), before, "unchecked publication and rejected caller validation stay closed");
    let (world, mut host) = hair::folder_scene::build(reopened.current(), 0, true).unwrap();
    let notes = hair::placement_scene::apply(&saved, &world, &mut host).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(host.decorations.len(), 2, "slice4C installs explicit saved geometry");
    assert!(host.blocked(saved.decorations[0].at));
    assert!(!host.blocked(saved.decorations[1].at));
}
#[test]
fn slice4b_preview_defaults_cancel_and_exact_theme_resolution_are_storage_free() {
    let f = Fixture::new();
    let office = hair::theme::Theme::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/office.toml"), &mut Default::default()).unwrap();
    let entry = &office.decorations.entries[0];
    let a = placement_store::Decoration::from_entry(entry, [2., -1.], 0.5, [0.8; 3]).unwrap();
    let b = placement_store::Decoration::from_entry(entry, [2., -1.], 0.5, [0.8; 3]).unwrap();
    assert_ne!(a.placement_id, b.placement_id);
    assert_eq!(a.tag, entry.tag); assert_eq!(a.collision, entry.defaults.collision);
    assert!(!f.0.join(".hair-layout").exists());
    let before = a.clone();
    let (resolved, asset) = a.resolve(&office.decorations).unwrap();
    assert_eq!(resolved.id, entry.id); assert!(!asset.model.vertices.is_empty());
    let mut changed = hair::theme::Theme::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/archive.toml"), &mut Default::default()).unwrap();
    // Real separately-loaded theme with the same semantic tag but different visual/defaults.
    changed.decorations.entries[0].tag = a.tag.clone();
    changed.decorations.entries[0].defaults.collision = !a.collision;
    changed.decorations.entries.reverse();
    let (new_entry, _) = a.resolve(&changed.decorations).unwrap();
    assert_ne!(new_entry.defaults.collision, a.collision);
    assert_eq!(a, before, "theme resolution must not mutate saved physics or transforms");
    changed.decorations.entries.retain(|e| e.tag != a.tag);
    assert!(a.resolve(&changed.decorations).err().unwrap().contains("missing decoration tag"));
    drop((a,b)); // Cancel has no layout operation to undo.
    assert!(snapshot(&f.0).is_empty());
}
#[test]
fn slice4b_validation_refusal_and_late_overlap_never_activate_a_record() {
    use placement_store::Outcome;
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); let a = decoration();
    let mut calls = 0;
    let out = placement_store::place_decoration_checked(&e, &a, |_| { calls += 1; if calls == 2 { Err("late concurrent overlap".into()) } else { Ok(()) } });
    assert_eq!(out, Outcome::NotCommitted("late concurrent overlap".into()));
    let layout = placement_store::read_layout(&e).unwrap();
    assert!(layout.decorations.is_empty()); assert_eq!(layout.incomplete_records, 1);
    let before = snapshot(&f.0);
    assert!(matches!(place(&e, &a), Outcome::NotCommitted(_)));
    assert_eq!(snapshot(&f.0), before, "no auto-repair or reuse of failed decoration attempts");
}
#[test]
fn slice4b_busy_lock_refuses_without_altering_any_saved_bytes() {
    use std::os::fd::AsRawFd;
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); let a = decoration();
    assert_eq!(place(&e, &a), placement_store::Outcome::Committed { synced: true });
    let lock = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    let before = snapshot(&f.0);
    let mut b = a.clone(); b.placement_id = "abcdef0123456789abcdef0123456789".into();
    assert!(matches!(place(&e, &b), placement_store::Outcome::NotCommitted(s) if s.contains("busy")));
    assert!(placement_store::read_layout(&e).is_err()); assert_eq!(snapshot(&f.0), before);
}
#[test]
fn slice4b_malformed_symlink_foreign_and_rebound_records_preserve_data() {
    for kind in ["malformed", "unknown", "wrong-id", "wrong-kind", "pin", "oversize", "record-link", "slot-link", "root-link", "foreign-mode", "extra-child", "hardlink", "root-file", "foreign-root"] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap(); let a = decoration();
        assert_eq!(place(&e, &a), placement_store::Outcome::Committed { synced: true });
        let root = f.0.join(".hair-layout"); let slot = root.join(format!("dc-{ID}")); let rec = slot.join("decoration.toml");
        match kind {
            "malformed" => std::fs::write(&rec, b"[broken").unwrap(),
            "unknown" => std::fs::write(&rec, format!("{}\nasset='/absolute/path'", record())).unwrap(),
            "wrong-id" => std::fs::write(&rec, record().replace(ID, "abcdef0123456789abcdef0123456789")).unwrap(),
            "wrong-kind" => std::fs::write(&rec, record().replace("decoration-follow-theme", "file")).unwrap(),
            "pin" => std::fs::write(&rec, record().replace("decoration-follow-theme", "decoration-pinned")).unwrap(),
            "oversize" => std::fs::write(&rec, vec![b' '; 4097]).unwrap(),
            "record-link" => { std::fs::rename(&rec, f.0.join("outside")).unwrap(); std::os::unix::fs::symlink(f.0.join("outside"), &rec).unwrap(); },
            "slot-link" => { std::fs::rename(&slot, f.0.join("outside")).unwrap(); std::os::unix::fs::symlink(f.0.join("outside"), &slot).unwrap(); },
            "root-link" => { std::fs::rename(&root, f.0.join("outside")).unwrap(); std::os::unix::fs::symlink(f.0.join("outside"), &root).unwrap(); },
            "foreign-mode" => std::fs::set_permissions(&rec, std::fs::Permissions::from_mode(0o644)).unwrap(),
            "extra-child" => std::fs::write(slot.join("foreign"), b"untouched").unwrap(),
            "hardlink" => std::fs::hard_link(&rec, f.0.join("outside")).unwrap(),
            "root-file" => { std::fs::rename(&root, f.0.join("outside")).unwrap(); std::fs::write(&root, b"foreign root").unwrap(); },
            "foreign-root" => std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap(),
            _ => unreachable!(),
        }
        let before = snapshot(&f.0);
        let mut b = a.clone(); b.placement_id = "abcdef0123456789abcdef0123456789".into();
        assert!(matches!(place(&e, &b), placement_store::Outcome::NotCommitted(_)), "{kind}");
        assert_eq!(snapshot(&f.0), before, "{kind}: no data repair/overwrite");
    }
}
#[test]
fn slice4b_record_schema_limits_refuse_before_creating_layout() {
    for (old,new) in [("yaw=0.5", "yaw=nan"), ("size=[0.8,0.8,0.6]", "size=[0.0,0.8,0.6]"), ("at=[2.0,-1.0]", "at=[10001.0,0.0]"), ("tag='hair.seating.chair'", "tag='../chair'"), ("version=1", "version=2")] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
        let bad: placement_store::Decoration = toml::from_str(&record().replace(old,new)).unwrap();
        assert!(matches!(place(&e, &bad), placement_store::Outcome::NotCommitted(_)));
        assert!(!f.0.join(".hair-layout").exists());
    }
}
#[test]
fn slice4b_shared_record_budget_counts_file_history_and_decoration_slots() {
    let f = Fixture::new(); f.dir(".hair-layout"); f.file(".hair-layout/lock", b"");
    // 255 independent committed decoration slots plus one dormant file history slot.
    for i in 0..255 {
        let id = format!("{i:032x}"); let slot = format!(".hair-layout/dc-{id}"); f.dir(&slot);
        f.file(&format!("{slot}/decoration.toml"), record().replace(ID, &id).as_bytes());
    }
    let tx = ".hair-layout/tx-ffffffffffffffffffffffffffffffff"; f.dir(tx);
    f.file(&format!("{tx}/record.toml"), b"version=2\nname=[100,111,114,109,97,110,116]\ntoken=[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]\nidentity=[1,0,0]\n[position]\nat=[2.0,-1.0]\nyaw=0.5\nsize=[0.8,0.8,0.6]\n");
    let e = Explorer::new(&f.0).unwrap(); assert_eq!(placement_store::read_layout(&e).unwrap().decorations.len(), 255);
    let before = snapshot(&f.0);
    assert!(matches!(place(&e, &decoration()), placement_store::Outcome::NotCommitted(s) if s.contains("256 shared slots")));
    assert_eq!(snapshot(&f.0), before);
}
