//! External, UNREGISTERED candidate for a child module of main.rs under cfg(test).
//! One synthetic App-input tracer; no native window, GPU, or OS-input claim.
//! Parent must wait for the current test-source writer to stop before registering.
use super::*;
use std::{ffi::OsStr, fs, io::Write, os::unix::fs::MetadataExt};

const ORIGINAL: &str = concat!(
    "{\"raw_reader_probe\":\"ANGEL_RAW_READER_JSON_7C29E4\",",
    "\"note\":\"ordinary stored plain UTF-8, not a metadata description\"}\n",
);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        // The parent runner owns TMPDIR; keep this registered test self-contained.
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("owned test scratch");
        let path = base.join(format!("raw-reader-first-fixture-{}", std::process::id()));
        fs::create_dir(&path).expect("exclusive fixture; reconcile leftovers before rerun");
        let fixture = Self(path);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(fixture.0.join("notes.json"))
            .expect("create owned ordinary regular file");
        file.write_all(ORIGINAL.as_bytes()).unwrap();
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only this exclusively created fixture");
    }
}

type FileState = (PathBuf, u64, u64, u32, u64, i64, i64, Vec<u8>);
type FixtureInventory = (u32, Vec<FileState>);

// Immediate fixture inventory only; no recursive enumeration or followed links.
// atime is intentionally excluded because reading may update it.
fn inventory(folder: &Path) -> FixtureInventory {
    let directory = fs::symlink_metadata(folder).unwrap();
    assert!(directory.is_dir() && !directory.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        files.push((
            path.clone(),
            meta.dev(),
            meta.ino(),
            meta.mode(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            fs::read(path).unwrap(),
        ));
    }
    files.sort();
    (directory.mode(), files)
}

fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(
            !app.keyboard_input(Some(code), down, false, None).unwrap(),
            "unexpected exit from synthetic keyboard input: {code:?}"
        );
    }
}

#[test]
fn regular_json_file_e_reader_contains_original_text() {
    let fixture = Fixture::new();
    let before = inventory(&fixture.0);
    let file_path = fixture.0.join("notes.json");
    assert_eq!(fs::read(&file_path).unwrap(), ORIGINAL.as_bytes());
    assert!(
        ORIGINAL.len() < 16_384,
        "fixture fits the existing text-read budget"
    );

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = hair::theme::Theme::load(
        &root.join("packs/office.toml"),
        &mut hair::theme::AssetCache::default(),
    )
    .unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(fixture.0.clone()))).unwrap();
    assert_eq!(
        app.local.as_ref().unwrap().explorer.current().folder,
        fixture.0
    );

    // Resolve the actual raw-name binding, not an assumed slot or hard-coded target.
    let view = app.world.folder_view.as_ref().unwrap();
    assert_eq!(view.bindings.len(), 1);
    let (anchor_id, bound) = view
        .bindings
        .iter()
        .find(|(_, bound)| bound.entry.name == OsStr::new("notes.json"))
        .expect("the fixture's actual file binding");
    assert_eq!(bound.entry.kind, hair::local_explorer::Kind::File);
    assert_eq!(bound.entry.path, file_path);
    let anchor_id = anchor_id.clone();
    let host = &app.themes[app.theme_index].host;
    let anchor = host.anchors.iter().find(|a| a.id == anchor_id).unwrap();
    let inward = host.portal_inward(&anchor_id);
    app.pose = Pose {
        position: [
            anchor.at[0] + 1.05 * inward[0],
            anchor.at[2] + 1.05 * inward[1],
        ],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    assert!(
        !host.blocked(app.pose.position),
        "fixture approach is reachable"
    );
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    let target = app
        .world
        .interaction
        .as_ref()
        .expect("eligible physical file target");
    assert_eq!(target.anchor, anchor_id);
    assert_eq!(
        target.destination.as_deref(),
        Some(hair::folder_scene::display(file_path.as_os_str()).as_str())
    );

    tap(&mut app, KeyCode::KeyE);
    assert!(
        app.world.reading,
        "E must open the real local reader before checking content"
    );
    let document = app.world.document().unwrap().to_owned();
    assert!(document.contains(&hair::folder_scene::display(file_path.as_os_str())));
    let has_original = document
        .as_bytes()
        .windows(ORIGINAL.len())
        .any(|bytes| bytes == ORIGINAL.as_bytes());

    // Run preservation and cleanup even on the intended missing-body failure.
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    assert_eq!(
        inventory(&fixture.0),
        before,
        "reader must not write the fixture"
    );
    drop(app);
    drop(fixture);

    assert!(
        has_original,
        "RAW_READER_EXTENSION_GATE: E opened notes.json, but world.document() did not contain its original plain UTF-8 bytes; metadata-only unsupported text is not content. Actual document: {document:?}"
    );
}
