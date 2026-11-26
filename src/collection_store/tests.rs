use super::*;
mod faults;
mod metadata;
use crate::local_explorer::Explorer;
use std::{fs, io::Write, os::unix::{ffi::OsStringExt, fs::PermissionsExt}, path::PathBuf, process::Command, sync::atomic::{AtomicU64, Ordering}};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("collection-c-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn root(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        p
    }
}
impl Drop for Fixture { fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); } }
fn policy() -> Policy { Policy::new(16 * 1024 * 1024, 64 * 1024 * 1024).unwrap() }
fn name() -> OsString { OsString::from_vec("large-é\n.bin".as_bytes().to_vec()) }
const LENGTH: u64 = 5 * 1024 * 1024 + 17;
fn check_reopened(root: &Path) {
    let store = Store::open(root, policy()).unwrap();
    assert_eq!(store.items().len(), 1);
    let item = &store.items()[0];
    assert_eq!(item.id, 1);
    assert_eq!(item.name, name());
    assert_eq!(item.category, Category::Document);
    assert_eq!(item.bytes, LENGTH);
    assert_eq!(store.usage(), LENGTH);
    struct Exact(u64);
    impl Write for Exact {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            assert!(bytes.len() <= 65536, "bounded streaming reads");
            assert!(bytes.iter().all(|b| *b == 0xa7));
            self.0 += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let mut exact = Exact(0);
    store.copy_to(item.id, &mut exact, &AtomicBool::new(false)).unwrap();
    assert_eq!(exact.0, LENGTH);
}
#[test]
fn import_reopen_exact_large_lossless_document() {
    let f = Fixture::new();
    let root = f.root("cache");
    let source = f.root("originals");
    let path = source.join(name());
    let mut file = fs::File::create(&path).unwrap();
    let block = [0xa7; 4096];
    for _ in 0..(LENGTH / 4096) { file.write_all(&block).unwrap(); }
    file.write_all(&block[..(LENGTH % 4096) as usize]).unwrap();
    drop(file);
    let explorer = Explorer::new(&source).unwrap();
    let reader = explorer.open_reader(0).unwrap();
    let mut store = Store::open(&root, policy()).unwrap();
    let imported = store.import_document(&name(), &reader, &AtomicBool::new(false)).unwrap();
    assert_eq!(imported.value.id, 1);
    assert!(imported.sync_warning.is_none());
    drop(store);
    check_reopened(&root);
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "collection_store::tests::cold_reopen_child", "--nocapture", "--test-threads=1"])
        .env("HAIR_COLLECTION_C_CHILD_ROOT", &root).output().unwrap();
    assert!(output.status.success(), "child: {}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    assert_eq!(fs::metadata(path).unwrap().len(), LENGTH);
    println!("COLLECTION_C_COLD_REOPEN: exited child checked exact identity/name/{} bytes; no decoder", LENGTH);
}
#[test]
fn cold_reopen_child() {
    let Some(root) = std::env::var_os("HAIR_COLLECTION_C_CHILD_ROOT") else { return; };
    check_reopened(Path::new(&root));
}

fn small_source(f: &Fixture, bytes: &[u8]) -> (PathBuf, ReaderFile) {
    let dir = f.root("source");
    let path = dir.join("original.txt");
    fs::write(&path, bytes).unwrap();
    let reader = Explorer::new(&dir).unwrap().open_reader(0).unwrap();
    (path, reader)
}
#[test]
fn removal_preserves_source_room_copy_and_other_cache_entry_refuses_stale_view() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (original, reader) = small_source(&f, b"independent ownership");
    let room = f.root("room-retained").join("exact-copy");
    fs::copy(&original, &room).unwrap();
    let mut a = Store::open(&root, policy()).unwrap();
    let first = a.import_document(OsStr::new("one"), &reader, &AtomicBool::new(false)).unwrap().value;
    let mut b = Store::open(&root, policy()).unwrap();
    let second = b.import_document(OsStr::new("two"), &reader, &AtomicBool::new(false)).unwrap().value;
    let before = fs::read(root.join(MANIFEST)).unwrap();
    assert!(a.remove(first.id).is_err(), "stale removal must refuse before writing");
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
    a.refresh().unwrap();
    let committed = a.remove(first.id).unwrap();
    assert!(committed.is_complete());
    assert!(!root.join(payload_name(first.id)).exists());
    let reopened = Store::open(&root, policy()).unwrap();
    assert_eq!(reopened.items(), &[second]);
    assert_eq!(fs::read(original).unwrap(), b"independent ownership");
    assert_eq!(fs::read(room).unwrap(), b"independent ownership");
    assert!(a.remove(first.id).is_err());
}

#[test]
fn independent_bounds_lowering_and_root_change_never_delete_or_move() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, b"12345678");
    for (item, total) in [(0, 1), (1, 0), (u64::MAX, 1), (1, u64::MAX)] {
        assert!(Policy::new(item, total).is_err());
    }
    let mut store = Store::open(&root, Policy::new(7, 100).unwrap()).unwrap();
    let before = fs::read(root.join(MANIFEST)).unwrap();
    assert!(format!("{:?}", store.import_document(OsStr::new("x"), &reader, &AtomicBool::new(false))).contains("Per-item"));
    assert!(!root.join(PAYLOAD_PENDING).exists());
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
    store.set_policy(Policy::new(8, 15).unwrap());
    let first = store.import_document(OsStr::new("x"), &reader, &AtomicBool::new(false)).unwrap().value;
    let before = fs::read(root.join(MANIFEST)).unwrap();
    assert!(format!("{:?}", store.import_document(OsStr::new("y"), &reader, &AtomicBool::new(false))).contains("Aggregate"));
    assert!(!root.join(PAYLOAD_PENDING).exists());
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
    store.set_policy(Policy::new(1, 1).unwrap());
    store.refresh().unwrap();
    assert_eq!(store.items(), std::slice::from_ref(&first));
    assert_eq!(Store::open(&root, Policy::new(1, 1).unwrap()).unwrap().items(), &[first]);
    let other = f.root("other-cache");
    assert!(Store::open(&other, policy()).unwrap().items().is_empty());
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
    assert_eq!(store.usage(), 8);
}

#[test]
fn storage_exceeds_unchanged_layout_and_preview_caps_without_body_allocation() {
    let f = Fixture::new();
    let root = f.root("cache");
    let source = f.root("source");
    let path = source.join("large-zeros");
    let len = crate::inventory::MAX_TOTAL as u64 + 1;
    assert_eq!(crate::inventory::MAX_TOTAL, 32 * 1024 * 1024);
    fs::File::create(&path).unwrap().set_len(len).unwrap();
    let reader = Explorer::new(&source).unwrap().open_reader(0).unwrap();
    assert!(reader.read_image_bytes(&AtomicBool::new(false)).is_err());
    let mut store = Store::open(&root, Policy::new(len, len).unwrap()).unwrap();
    let item = store.import_document(OsStr::new("large-zeros"), &reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(item.bytes, len);
    assert_eq!(store.usage(), len);
    drop(store);
    let reopened = Store::open(&root, Policy::new(1, 1).unwrap()).unwrap();
    reopened.copy_to(item.id, &mut std::io::sink(), &AtomicBool::new(false)).unwrap();
    assert_eq!(fs::metadata(path).unwrap().len(), len);
}

#[test]
fn schema_corruption_payload_corruption_and_bounded_metadata_refuse_without_mutation() {
    for case in ["schema", "checksum", "truncated", "oversized", "payload", "short-payload", "missing-payload", "count", "trailing", "duplicate-id", "unsafe-name"] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (_, reader) = small_source(&f, b"original bytes");
        let mut store = Store::open(&root, policy()).unwrap();
        let item = store.import_document(OsStr::new("x"), &reader, &AtomicBool::new(false)).unwrap().value;
        let manifest = root.join(MANIFEST);
        let mut bytes = fs::read(&manifest).unwrap();
        match case {
            "schema" => bytes[8..12].copy_from_slice(&99_u32.to_le_bytes()),
            "checksum" => bytes[20] ^= 1,
            "truncated" => bytes.truncate(20),
            "oversized" => bytes.resize(MANIFEST_BYTES as usize + 1, 0),
            "payload" => fs::write(root.join(payload_name(item.id)), b"corrupt! bytes").unwrap(),
            "short-payload" => fs::write(root.join(payload_name(item.id)), b"short").unwrap(),
            "missing-payload" => fs::remove_file(root.join(payload_name(item.id))).unwrap(),
            "count" => { bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes()); rehash(&mut bytes); },
            "trailing" => { bytes.insert(bytes.len() - 32, 42); rehash(&mut bytes); },
            "duplicate-id" => { bytes[32..40].copy_from_slice(&0_u64.to_le_bytes()); rehash(&mut bytes); },
            "unsafe-name" => { bytes[83] = b'/'; rehash(&mut bytes); },
            _ => unreachable!(),
        }
        fs::write(&manifest, &bytes).unwrap();
        if matches!(case, "payload" | "short-payload" | "missing-payload") {
            // Only the old GLOBAL body-failure expectation is obsolete. Selected
            // use, selected removal and recovery must still refuse. Import may
            // preserve unrelated damaged expected records (tested independently).
            let mut reopened = Store::open(&root, policy()).unwrap();
            assert!(!matches!(reopened.verify_all(u64::MAX, &AtomicBool::new(false)).unwrap()[0].status, PayloadStatus::Verified));
            assert!(reopened.copy_to(item.id, &mut Vec::new(), &AtomicBool::new(false)).is_err());
            assert!(reopened.remove(item.id).is_err());
            assert!(reopened.inspect_interrupted_import(64 * 1024 * 1024, &AtomicBool::new(false)).is_err());
        } else {
            let error = match Store::open(&root, policy()) { Ok(_) => panic!("accepted {case}"), Err(e) => e };
            if case == "schema" { assert!(error.contains("schema 99") && error.contains("compatible") && error.contains("unchanged")); }
        }
        assert_eq!(fs::read(&manifest).unwrap(), bytes, "mutated {case}");
        assert!(!root.join(MANIFEST_PENDING).exists());
    }
}
fn rehash(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}

#[test]
fn raw_names_are_metadata_only_and_invalid_components_never_stage() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, b"bytes");
    let mut store = Store::open(&root, policy()).unwrap();
    for bad in ["", ".", "..", "../escape", "/absolute", "a/b", "a\0b", &"x".repeat(256)] {
        assert!(store.import_document(OsStr::new(bad), &reader, &AtomicBool::new(false)).is_err());
    }
    // APFS itself refuses a non-UTF-8 directory entry. The raw metadata format
    // still roundtrips these bytes without ever using them as a payload path.
    let raw = OsString::from_vec(b"label-\xff\n\\:data".to_vec());
    let item = store.import_document(&raw, &reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(Store::open(&root, policy()).unwrap().items(), &[item]);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 3);
}

#[test]
fn pending_orphan_unknown_evidence_is_sorted_preserved_and_blocks_writes() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, b"bytes");
    let mut store = Store::open(&root, policy()).unwrap();
    for name in [PAYLOAD_PENDING, MANIFEST_PENDING, "item-00000000000000ff", "unknown-owner-evidence"] {
        fs::write(root.join(name), name.as_bytes()).unwrap();
    }
    store.refresh().unwrap();
    let expected: Vec<_> = ["item-00000000000000ff", MANIFEST_PENDING, PAYLOAD_PENDING, "unknown-owner-evidence"].into_iter().map(OsString::from).collect();
    assert_eq!(store.evidence().unwrap(), expected);
    let before = fs::read(root.join(MANIFEST)).unwrap();
    assert!(store.import_document(OsStr::new("x"), &reader, &AtomicBool::new(false)).is_err());
    assert!(store.remove(1).is_err());
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), before);
    for name in &expected { assert_eq!(fs::read(root.join(name)).unwrap(), name.as_bytes()); }
    assert_eq!(Store::open(&root, policy()).unwrap().evidence().unwrap(), expected);
}

#[test]
fn symlink_hardlink_private_root_and_held_source_identity_protections() {
    use std::os::unix::fs::symlink;
    for target in ["root", "ancestor", "lock", "manifest", "payload", "hardlink", "mode"] {
        let f = Fixture::new();
        let root = f.root("cache");
        let (source, reader) = small_source(&f, b"authentic");
        let mut store = Store::open(&root, policy()).unwrap();
        let item = store.import_document(OsStr::new("x"), &reader, &AtomicBool::new(false)).unwrap().value;
        let path = match target { "lock" => root.join("lock"), "manifest" => root.join(MANIFEST), _ => root.join(payload_name(item.id)) };
        match target {
            "root" => { fs::rename(&root, f.0.join("held-root")).unwrap(); symlink(f.0.join("held-root"), &root).unwrap(); },
            "ancestor" => {
                symlink(&f.0, f.0.join("alias")).unwrap();
                assert!(Store::open(&f.0.join("alias/cache"), policy()).is_err());
                continue;
            },
            "hardlink" => { fs::hard_link(&path, f.0.join("external-link")).unwrap(); },
            "mode" => fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap(),
            _ => { fs::rename(&path, f.0.join("held-file")).unwrap(); symlink(f.0.join("held-file"), &path).unwrap(); },
        }
        if matches!(target, "payload" | "hardlink") {
            let reopened = Store::open(&root, policy()).unwrap();
            assert!(matches!(reopened.inspect().unwrap()[0].status, PayloadStatus::Unavailable(_)));
            assert!(reopened.copy_to(item.id, &mut Vec::new(), &AtomicBool::new(false)).is_err());
        } else {
            assert!(Store::open(&root, policy()).is_err(), "accepted {target}");
        }
        assert!(store.remove(item.id).is_err(), "remove accepted {target}");
        assert_eq!(fs::read(&source).unwrap(), b"authentic");
    }
    let f = Fixture::new();
    let root = f.root("cache");
    let (path, reader) = small_source(&f, b"held original");
    let old = path.with_file_name("held-original");
    fs::rename(&path, &old).unwrap();
    fs::write(&path, b"same-name decoy").unwrap();
    let mut store = Store::open(&root, policy()).unwrap();
    // A namespace rename changes ctime on some filesystems; acquire after the
    // rename to retain the checked original, then replace only its display path.
    drop(reader);
    let explorer = Explorer::new(old.parent().unwrap()).unwrap();
    let index = explorer.current().entries.iter().position(|e| e.name == "held-original").unwrap();
    let held = explorer.open_reader(index).unwrap();
    let item = store.import_document(OsStr::new("original.txt"), &held, &AtomicBool::new(false)).unwrap().value;
    assert_eq!(fs::read(root.join(payload_name(item.id))).unwrap(), b"held original");
    assert_eq!(fs::read(&path).unwrap(), b"same-name decoy");
    let link = old.with_file_name("link");
    symlink(&path, &link).unwrap();
    let explorer = Explorer::new(old.parent().unwrap()).unwrap();
    let index = explorer.current().entries.iter().position(|e| e.name == "link").unwrap();
    assert!(explorer.open_reader(index).is_err());
}

#[test]
fn scoped_lock_blocks_other_instances_and_child_then_releases() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (_, reader) = small_source(&f, b"bytes");
    let mut first = Store::open(&root, policy()).unwrap();
    let mut second = Store::open(&root, policy()).unwrap();
    let lock = first.lock().unwrap();
    assert!(second.import_document(OsStr::new("blocked"), &reader, &AtomicBool::new(false)).is_err());
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "collection_store::tests::busy_child", "--nocapture", "--test-threads=1"])
        .env("HAIR_COLLECTION_C_BUSY_ROOT", &root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    drop(lock);
    let one = first.import_document(OsStr::new("one"), &reader, &AtomicBool::new(false)).unwrap().value;
    let two = second.import_document(OsStr::new("two"), &reader, &AtomicBool::new(false)).unwrap().value;
    assert_eq!((one.id, two.id), (1, 2));
    assert_eq!(Store::open(&root, policy()).unwrap().items(), &[one, two]);
}
#[test]
fn busy_child() {
    let Some(root) = std::env::var_os("HAIR_COLLECTION_C_BUSY_ROOT") else { return; };
    let error = match Store::open(Path::new(&root), policy()) { Ok(_) => panic!("child acquired busy collection"), Err(e) => e };
    assert!(error.contains("busy"));
}

#[test]
fn late_source_growth_before_manifest_activation_never_commits() {
    let f = Fixture::new();
    let root = f.root("cache");
    let (path, reader) = small_source(&f, b"held bytes");
    let mut store = Store::open(&root, policy()).unwrap();
    let manifest = fs::read(root.join(MANIFEST)).unwrap();
    let result = store.import_inner(OsStr::new("x"), &reader, &AtomicBool::new(false), |stage, _| {
        if stage == ImportStage::ManifestReady {
            fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"growth").unwrap();
        }
        Ok(())
    });
    assert!(matches!(result, Err(Failure::Refused(_))), "late growth accepted: {result:?}");
    assert_eq!(fs::read(root.join(MANIFEST)).unwrap(), manifest);
    let reopened = Store::open(&root, policy()).unwrap();
    assert!(reopened.items().is_empty());
    assert_eq!(reopened.evidence().unwrap(), &[OsString::from(payload_name(1)), OsString::from(MANIFEST_PENDING)]);
    assert_eq!(fs::read(path).unwrap(), b"held bytesgrowth");
}
