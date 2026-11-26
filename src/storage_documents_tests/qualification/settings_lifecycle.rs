//! L2/L3 actual chooser and numeric-settings input; disposable roots only.
use super::*;
use std::os::fd::AsRawFd;
fn names(path: &Path) -> Vec<std::ffi::OsString> {
    let mut names = std::fs::read_dir(path).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>();
    names.sort(); names
}
fn limit(a: &mut App, value: &str) {
    activate(a, Item::StorageItemLimit); text(a, value); tap(a, KeyCode::Enter);
}
#[test]
fn storage_preferences_lifecycle_known_blocked_chooser_never_initializes_candidate() {
    let f = Fixture::new("prefs-blocked");
    let prefs = f.directory("prefs"); let old = f.directory("old"); let empty = f.directory("empty-room");
    let mut a = app(); a.restore_preferences_at(&prefs).unwrap(); settings(&mut a);
    choose(&mut a, Ok(Some(old.clone())));
    let saved = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    let manifest = std::fs::read(old.join("manifest")).unwrap();
    a.storage.preferences_blocked = true;
    choose(&mut a, Ok(Some(empty.clone())));
    assert!(names(&empty).is_empty(), "known blocked preferences must precede ALL candidate writes");
    assert_eq!(a.storage.preferences.path().unwrap(), old);
    assert_eq!(std::fs::read(old.join("manifest")).unwrap(), manifest);
    limit(&mut a, "19");
    assert_eq!(a.storage.preferences.item_bytes, storage_native::DEFAULT_ITEM_BYTES);
    assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), saved);
}
#[test]
fn storage_preferences_lifecycle_unsupported_chooser_preserves_file_and_empty_room() {
    let f = Fixture::new("prefs-unsupported"); let prefs = f.directory("prefs"); let empty = f.directory("empty-room");
    let store = hair::acquisition::PreferenceFile::open(&prefs).unwrap();
    let bytes = b"version = 2\nui_scale = 100\ntree_visible = true\n[fog]\nenabled = false\ndistance = 20.0\ntint = [0.1, 0.2, 0.3]\n";
    store.write(bytes).unwrap();
    let mut a = app(); assert!(a.restore_preferences_at(&prefs).is_err()); settings(&mut a);
    choose(&mut a, Ok(Some(empty.clone())));
    assert!(names(&empty).is_empty(), "unsupported schema must not initialize the selected room");
    assert!(!a.inventory.is_persistent()); assert_eq!(store.read().unwrap().unwrap(), bytes);
}
#[test]
fn storage_preferences_lifecycle_late_lock_preserves_old_and_discloses_actual_candidate() {
    let f = Fixture::new("prefs-late-lock"); let prefs = f.directory("prefs");
    let old = f.directory("old"); let candidate = f.directory("candidate");
    let mut a = app(); a.restore_preferences_at(&prefs).unwrap(); settings(&mut a); choose(&mut a, Ok(Some(old.clone())));
    let saved = std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap();
    let lock = std::fs::File::open(prefs.join("HAIR")).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }, 0);
    choose(&mut a, Ok(Some(candidate.clone())));
    assert!(candidate.join("manifest").is_file()); assert!(candidate.join("lock").is_file());
    assert_eq!(a.storage.preferences.path().unwrap(), old);
    assert_eq!(std::fs::read(prefs.join("HAIR/player-settings.toml")).unwrap(), saved);
    assert!(a.storage_lines().iter().any(|s| s.contains("candidate may remain initialized") && s.contains(candidate.to_str().unwrap())), "leftover cache disclosure missing: {:?}", a.storage_lines());
    drop(lock);
    choose(&mut a, Ok(Some(candidate.clone())));
    assert_eq!(a.storage.preferences.path().unwrap(), candidate);
}
