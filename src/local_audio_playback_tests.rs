//! First audio CONTROL tracer only: real owned WAV -> folder selection -> E -> Space.
//! No decoder/output mocks, injected sessions, clocks, devices or listening claim.
use super::*;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const PCM: [i16; 16] = [
    0, 8192, -8192, 16384, -16384, 24576, -24576, 4096, -4096, 1024, -1024, 30000, -30000, 2048,
    -2048, 0,
];

fn wav(samples: &[i16; 16]) -> Vec<u8> {
    // Genuine canonical RIFF/WAVE: PCM16 LE, mono, 8000 Hz, 16 owned frames.
    // Encoded bytes are NOT a native decoder result or evidence of output.
    let data_len = u32::try_from(samples.len() * 2).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8000_u32.to_le_bytes());
    bytes.extend_from_slice(&16000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    assert_eq!(bytes.len(), 76);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    let stored_pcm: Vec<_> = bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(stored_pcm.as_slice(), samples);
    bytes
}

fn exclusive(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
}

type FileState = (u64, u64, u32, u64, i64, i64, Vec<u8>);
fn file_state(path: &Path) -> FileState {
    let meta = fs::symlink_metadata(path).unwrap();
    assert!(meta.is_file() && !meta.file_type().is_symlink());
    (
        meta.dev(),
        meta.ino(),
        meta.mode(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        fs::read(path).unwrap(),
    )
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn key(app: &mut App, down: bool, repeat: bool) {
    assert!(
        !app.keyboard_input(Some(KeyCode::Space), down, repeat, None)
            .unwrap()
    );
}
fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
    }
}
fn raw_bytes(document: &str) -> Option<Vec<u8>> {
    let body = document
        .split_once("\nBYTE GROUPS (8 bytes max)\n")?
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")?
        .0;
    let mut bytes = Vec::new();
    for line in body.lines().filter_map(|line| line.strip_prefix("HEX ")) {
        for token in line.split_whitespace() {
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
    }
    Some(bytes)
}
fn require_raw(app: &mut App, encoded: &[u8]) {
    for _ in 0..3 {
        if let Some(actual) = raw_bytes(app.world.document().unwrap()) {
            assert_eq!(
                actual, encoded,
                "original held WAV, not same-name display decoy"
            );
            return;
        }
        tap(app, KeyCode::KeyV);
    }
    panic!("V must retain exact original raw byte access");
}
fn expect_control(app: &App, status: &str, stage: &str, missing: &mut Vec<String>) {
    // Proposed UI request contract, not an invented player API. Deliberately says
    // REQUESTED, never PLAYING: a text/control GREEN will not certify output.
    let document = app.world.document().unwrap();
    if !document.lines().any(|line| line == status) {
        missing.push(format!("{stage}: missing {status:?}"));
    }
}

#[test]
fn owned_pcm_wav_e_then_physical_space_requests_play_pause_in_audio_body() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(temp.starts_with(root.parent().unwrap()));
    assert!(!temp.starts_with(&root) && !root.starts_with(&temp));
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = temp.join(format!(
        "hair-audio-control-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    let selected = fixture.0.join("selected");
    let held = fixture.0.join("held");
    fs::create_dir(&selected).unwrap();
    let name = OsString::from("Owned Audio\né.WaV");
    let encoded = wav(&PCM);
    let decoy = wav(&[0; 16]);
    assert_ne!(encoded, decoy);
    assert_eq!(hair::local_explorer::READER_CHUNK_BYTES, 4096);
    assert_eq!(hair::local_explorer::MAX_BODY_BYTES, 4 * 1024 * 1024);
    exclusive(&selected.join(&name), &encoded);

    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    // The actual chooser claim/completion seam, not a constructed Local/session.
    app.frames = 1;
    tap(&mut app, KeyCode::KeyO);
    assert!(app.claim_folder_request());
    app.complete_folder(Ok(Some(selected.clone()))).unwrap();
    assert!(!app.world.reading);
    let view = app.world.folder_view.as_ref().unwrap();
    assert_eq!(view.bindings.len(), 1);
    let (id, bound) = view
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .unwrap();
    let (id, bound) = (id.clone(), bound.clone());
    assert_eq!(bound.entry.kind, hair::local_explorer::Kind::File);
    assert_eq!(bound.entry.path, selected.join(&name));
    // Before E, replace ONLY the owned display path. Acquisition must use the
    // retained directory and literal component, not a reconstructed display name.
    fs::rename(&selected, &held).unwrap();
    fs::create_dir(&selected).unwrap();
    exclusive(&selected.join(&name), &decoy);
    let before = file_state(&held.join(&name));
    let decoy_before = file_state(&selected.join(&name));
    let host = &app.themes[0].host;
    let anchor = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(&id);
    app.pose = Pose {
        position: [
            anchor.at[0] + 1.05 * inward[0],
            anchor.at[2] + 1.05 * inward[1],
        ],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
    assert!(!app.world.reading, "aim is not E or a playback request");
    key(&mut app, true, false);
    key(&mut app, false, false);
    assert!(
        !app.world.reading,
        "world Space must not open the audio reader"
    );
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    let opened = app.world.document().unwrap().to_string();
    assert!(opened.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
    let mut missing = Vec::new();
    expect_control(
        &app,
        "AUDIO STATUS: READY",
        "E must not request play",
        &mut missing,
    );
    if !opened
        .lines()
        .any(|line| line.contains("Space") && line.contains("play/pause"))
    {
        missing.push("opened audio body must teach Space play/pause".into());
    }

    key(&mut app, true, false);
    expect_control(
        &app,
        "AUDIO STATUS: PLAY REQUESTED",
        "first explicit press",
        &mut missing,
    );
    let play = app.world.document().unwrap().to_string();
    key(&mut app, true, true);
    key(&mut app, true, false);
    assert_eq!(
        app.world.document().unwrap(),
        play,
        "repeat/duplicate press must not toggle"
    );
    key(&mut app, false, false);
    assert_eq!(
        app.world.document().unwrap(),
        play,
        "physical release must not toggle"
    );
    key(&mut app, true, false);
    expect_control(
        &app,
        "AUDIO STATUS: PAUSE REQUESTED",
        "second physical press",
        &mut missing,
    );
    key(&mut app, false, false);

    tap(&mut app, KeyCode::KeyI);
    let info = app.world.document().unwrap().to_string();
    assert!(info.contains("METADATA VIEW"));
    assert!(info.contains(&hair::folder_scene::display(&name)));
    assert!(info.contains(&hair::folder_scene::display(bound.entry.path.as_os_str())));
    key(&mut app, true, false);
    key(&mut app, false, false);
    assert_eq!(
        app.world.document().unwrap(),
        info,
        "Space is audio-body only, not info"
    );
    tap(&mut app, KeyCode::KeyI);
    require_raw(&mut app, &encoded);
    let raw = app.world.document().unwrap().to_string();
    assert!(raw.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
    key(&mut app, true, false);
    key(&mut app, false, false);
    assert_eq!(
        app.world.document().unwrap(),
        raw,
        "Space is inert in raw mode"
    );
    tap(&mut app, KeyCode::KeyC);
    require_raw(&mut app, &encoded);
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    assert!(!app.input.captured && !app.input.forward);
    drop(app);
    assert_eq!(
        file_state(&held.join(&name)),
        before,
        "held source nonmutation"
    );
    assert_eq!(
        file_state(&selected.join(&name)),
        decoy_before,
        "decoy nonmutation"
    );
    println!(
        "AUDIO_CONTROL_ONLY missing={missing:?}; no native decode, device, emitted sound or listening proof"
    );
    assert!(
        missing.is_empty(),
        "LOCAL_AUDIO_EXPLICIT_CONTROL_MISSING: {missing:?}"
    );
}
