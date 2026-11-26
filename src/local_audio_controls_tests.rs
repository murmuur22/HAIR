//! Request-only actual-App tracer. No Play, decoded PCM injection or Output mock.
//! UI lines below are a proposed deferred-request contract, NOT backend evidence.
use super::*;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

fn wav(silent: bool) -> Vec<u8> {
    let samples: [i16; 16] = if silent {
        [0; 16]
    } else {
        [
            0, 8192, -8192, 16384, -16384, 24576, -24576, 4096, -4096, 1024, -1024, 30000, -30000,
            2048, -2048, 0,
        ]
    };
    let mut bytes = Vec::from(&b"RIFF"[..]);
    bytes.extend_from_slice(&68_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8000_u32.to_le_bytes());
    bytes.extend_from_slice(&16000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&32_u32.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    assert_eq!(bytes.len(), 76);
    let stored: Vec<_> = bytes[44..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(stored.as_slice(), &samples);
    bytes
}
fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[12, 200, 40, 230, 30, 90])
            .unwrap();
        writer.finish().unwrap();
    }
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
type FileState = (OsString, u64, u64, u32, u64, i64, i64, Vec<u8>);
fn inventory(dir: &Path) -> (u32, Vec<FileState>) {
    let metadata = fs::symlink_metadata(dir).unwrap();
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        files.push((
            entry.file_name(),
            meta.dev(),
            meta.ino(),
            meta.mode(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            fs::read(entry.path()).unwrap(),
        ));
    }
    files.sort();
    (metadata.mode(), files)
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn key(app: &mut App, code: KeyCode, down: bool, repeat: bool) {
    assert!(!app.keyboard_input(Some(code), down, repeat, None).unwrap());
}
fn tap(app: &mut App, code: KeyCode) {
    key(app, code, true, false);
    key(app, code, false, false);
}
fn open_named(app: &mut App, name: &OsString) {
    // Geometric aiming is setup, never a fabricated reader/session/control state.
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| &b.entry.name == name)
        .unwrap()
        .0
        .clone();
    let host = &app.themes[app.theme_index].host;
    let anchor = host.anchors.iter().find(|a| a.id == id).unwrap();
    let inward = host.portal_inward(&id);
    app.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
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
    tap(app, KeyCode::KeyE);
    assert!(app.world.reading);
}
fn raw_bytes(doc: &str) -> Option<Vec<u8>> {
    let body = doc
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
fn require_raw(app: &mut App, bytes: &[u8]) {
    for _ in 0..3 {
        if let Some(actual) = raw_bytes(app.world.document().unwrap()) {
            assert_eq!(actual, bytes, "retained original, never display-path decoy");
            return;
        }
        tap(app, KeyCode::KeyV);
    }
    panic!("V must expose original raw source");
}
fn missing_line(doc: &str, line: &str, stage: &str, missing: &mut Vec<String>) {
    if !doc.lines().any(|actual| actual == line) {
        missing.push(format!("{stage}: missing {line:?}"));
    }
}
fn deferred(app: &mut App) {
    // Existing baseline APIs only. Observation, never private-state mutation.
    assert!(
        !app.audio.busy(),
        "no acquisition/decoded track/output before Play"
    );
    app.poll_local_audio().unwrap();
    assert!(
        !app.audio.busy(),
        "ordinary audio poll must not acquire on adjustment"
    );
    assert_eq!(app.audio.observed, audio::Observed::Ready);
    assert!(app.audio.error.is_none());
    if let Some(state) = app
        .local
        .as_ref()
        .unwrap()
        .reader
        .as_ref()
        .unwrap()
        .audio
        .as_ref()
    {
        assert_eq!(state.desired, audio::Desired::Stopped);
        assert!(
            !state.requested,
            "seek/volume is not a Play acquisition request"
        );
        assert_eq!(state.observed, audio::Observed::Ready);
        assert!(state.error.is_none());
    }
    let doc = app.world.document().unwrap();
    for status in ["PLAYING", "PAUSED", "ACQUIRING", "PLAY REQUESTED"] {
        assert!(
            !doc.lines()
                .any(|line| line == format!("AUDIO STATUS: {status}"))
        );
    }
}
fn requests(app: &mut App, seek: u32, volume: u32, stage: &str, missing: &mut Vec<String>) {
    deferred(app);
    // Logical pending seconds may exceed this tiny fixture's duration: neither
    // decode nor a known total is allowed before Play. Never call this position.
    let doc = app.world.document().unwrap();
    missing_line(doc, "AUDIO STATUS: READY", stage, missing);
    missing_line(
        doc,
        &format!("AUDIO SEEK REQUEST: {seek} s (pending; duration unknown)"),
        stage,
        missing,
    );
    missing_line(
        doc,
        &format!("AUDIO VOLUME REQUEST: {volume}% (pending)"),
        stage,
        missing,
    );
}
fn edge(app: &mut App, code: KeyCode, seek: u32, volume: u32, missing: &mut Vec<String>) {
    key(app, code, true, false);
    requests(app, seek, volume, "fresh physical press", missing);
    let after = app.world.document().unwrap().to_string();
    key(app, code, true, true);
    key(app, code, true, false);
    assert_eq!(
        app.world.document().unwrap(),
        after,
        "repeat/duplicate must be inert"
    );
    key(app, code, false, false);
    assert_eq!(
        app.world.document().unwrap(),
        after,
        "release must not adjust"
    );
}

#[test]
fn owned_pcm_wav_physical_seek_volume_requests_are_deferred_and_source_preserved() {
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
    let path = temp.join(format!("hair-seek-volume-{}-{unique}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    let selected = fixture.0.join("selected");
    let held = fixture.0.join("held");
    fs::create_dir(&selected).unwrap();
    let name = OsString::from("Owned Seek Volume\né.WaV");
    let image_name = OsString::from("Owned image.png");
    let text_name = OsString::from("Owned text.txt");
    let encoded = wav(false);
    let decoy = wav(true);
    assert_ne!(encoded, decoy);
    exclusive(&selected.join(&name), &encoded);
    exclusive(&selected.join(&image_name), &png());
    exclusive(
        &selected.join(&text_name),
        b"Original text control context\n",
    );
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.frames = 1;
    tap(&mut app, KeyCode::KeyO);
    assert!(app.claim_folder_request());
    app.complete_folder(Ok(Some(selected.clone()))).unwrap();
    assert_eq!(app.world.folder_view.as_ref().unwrap().bindings.len(), 3);
    fs::rename(&selected, &held).unwrap();
    fs::create_dir(&selected).unwrap();
    exclusive(&selected.join(&name), &decoy);
    let before = inventory(&held);
    let decoy_before = inventory(&selected);
    open_named(&mut app, &name);
    let opened = app.world.document().unwrap().to_string();
    assert!(opened.starts_with("WAV AUDIO\nAUDIO STATUS: READY\n"));
    assert!(opened.contains(&hair::folder_scene::display(
        selected.join(&name).as_os_str()
    )));
    let mut missing = Vec::new();
    let chrome = opened.split_once("\nFULL SOURCE / METADATA:\n").unwrap().0;
    for help in [
        "Space play/pause | V audio/raw/text | I source/body",
        "C retry SAME handle | Escape close",
        "Left/Right seek request -/+5 s (0..300 s pending)",
        "+/- volume request 5% (0..100%; system unchanged)",
    ] {
        if !chrome.contains(help) {
            missing.push(format!("visible audio help: missing {help:?}"));
        }
    }
    if opened.contains("No seek, volume control") {
        missing.push("audio help still denies seek/volume controls".into());
    }
    requests(&mut app, 0, 100, "initial defaults", &mut missing);
    // Unity starts at the ceiling; retain saturation AND effective +/- edges.
    edge(&mut app, KeyCode::Equal, 0, 100, &mut missing);
    edge(&mut app, KeyCode::Minus, 0, 95, &mut missing);
    edge(&mut app, KeyCode::Equal, 0, 100, &mut missing);
    edge(&mut app, KeyCode::ArrowRight, 5, 100, &mut missing);
    edge(&mut app, KeyCode::ArrowRight, 10, 100, &mut missing);
    edge(&mut app, KeyCode::ArrowLeft, 5, 100, &mut missing);
    edge(&mut app, KeyCode::ArrowLeft, 0, 100, &mut missing);
    edge(&mut app, KeyCode::ArrowLeft, 0, 100, &mut missing);
    for n in 1..=62_u32 {
        tap(&mut app, KeyCode::ArrowRight);
        requests(
            &mut app,
            (n * 5).min(300),
            100,
            "seek upper clamp",
            &mut missing,
        );
    }
    for n in 1..=62_u32 {
        tap(&mut app, KeyCode::ArrowLeft);
        requests(
            &mut app,
            300_u32.saturating_sub(n * 5),
            100,
            "seek lower clamp",
            &mut missing,
        );
    }
    // Establish zero by physical input so the upper sweep still exercises
    // effective increases, not twenty-two inert presses at the unity ceiling.
    for n in 1..=20_u32 {
        tap(&mut app, KeyCode::Minus);
        requests(
            &mut app,
            0,
            100 - n * 5,
            "prepare upper sweep",
            &mut missing,
        );
    }
    for n in 1..=22_u32 {
        tap(&mut app, KeyCode::Equal);
        requests(
            &mut app,
            0,
            (n * 5).min(100),
            "volume upper clamp",
            &mut missing,
        );
    }
    for n in 1..=22_u32 {
        tap(&mut app, KeyCode::Minus);
        requests(
            &mut app,
            0,
            100_u32.saturating_sub(n * 5),
            "volume lower clamp",
            &mut missing,
        );
    }
    // Held keys survive switches. Releases in I/raw/text must re-arm each code.
    // Seek/volume preferences are retained across I/V; output intent is stopped.
    for code in [
        KeyCode::ArrowRight,
        KeyCode::ArrowLeft,
        KeyCode::Equal,
        KeyCode::Minus,
    ] {
        tap(&mut app, KeyCode::KeyC); // real same-handle retry resets request defaults
        requests(&mut app, 0, 100, "C defaults", &mut missing);
        if code == KeyCode::Equal {
            // Three effective +5 presses must remain observable across I/V;
            // starting at 100 would hide a release/re-arm regression.
            for _ in 0..3 {
                tap(&mut app, KeyCode::Minus);
            }
            requests(&mut app, 0, 85, "headroom for re-arm", &mut missing);
        }
        let (seek, volume) = match code {
            KeyCode::ArrowRight => (5, 100),
            KeyCode::Equal => (0, 90),
            KeyCode::Minus => (0, 95),
            _ => (0, 100),
        };
        key(&mut app, code, true, false);
        tap(&mut app, KeyCode::KeyI);
        let info = app.world.document().unwrap().to_string();
        assert!(info.contains("METADATA VIEW"));
        key(&mut app, code, true, true);
        key(&mut app, code, true, false);
        key(&mut app, code, false, false);
        assert_eq!(app.world.document().unwrap(), info);
        deferred(&mut app);
        tap(&mut app, KeyCode::KeyI);
        requests(&mut app, seek, volume, "release in metadata", &mut missing);
        key(&mut app, code, true, false);
        let (seek2, volume2) = match code {
            KeyCode::ArrowRight => (10, 100),
            KeyCode::Equal => (0, 95),
            KeyCode::Minus => (0, 90),
            _ => (0, 100),
        };
        requests(
            &mut app,
            seek2,
            volume2,
            "rearmed after metadata",
            &mut missing,
        );
        require_raw(&mut app, &encoded); // V while the adjustment key remains held
        key(&mut app, code, true, true);
        key(&mut app, code, true, false);
        key(&mut app, code, false, false);
        assert_eq!(raw_bytes(app.world.document().unwrap()).unwrap(), encoded);
        deferred(&mut app);
        // Raw -> text -> audio. Both non-audio contexts must ignore +/-.
        for mode in 0..2 {
            let doc = app.world.document().unwrap().to_string();
            for inert in [KeyCode::Equal, KeyCode::Minus, KeyCode::Space] {
                tap(&mut app, inert);
            }
            assert_eq!(
                app.world.document().unwrap(),
                doc,
                "raw/text controls isolated"
            );
            if mode == 0 {
                tap(&mut app, KeyCode::KeyV);
            }
        }
        key(&mut app, code, true, false); // held in text when V returns to audio
        tap(&mut app, KeyCode::KeyV);
        requests(
            &mut app,
            seek2,
            volume2,
            "return with key held",
            &mut missing,
        );
        key(&mut app, code, true, true);
        key(&mut app, code, true, false);
        requests(
            &mut app,
            seek2,
            volume2,
            "no synthetic press on body switch",
            &mut missing,
        );
        key(&mut app, code, false, false);
        edge(
            &mut app,
            code,
            if code == KeyCode::ArrowRight { 15 } else { 0 },
            if code == KeyCode::Minus { 85 } else { 100 },
            &mut missing,
        );
    }
    // C restores original source after baseline arrows leave interpretation.
    tap(&mut app, KeyCode::KeyC);
    require_raw(&mut app, &encoded);
    assert!(
        app.world
            .document()
            .unwrap()
            .contains("LIMIT: 4096 bytes/chunk; no whole-file load")
    );
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading && !app.input.captured && !app.input.forward);
    assert!(!app.audio.busy());

    // Actual PNG acquisition, pure file/decoder path; no renderer/device constructed.
    open_named(&mut app, &image_name);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        app.poll_local_image().unwrap();
        if !app.local_image_pending() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned PNG setup deadline, not audio RED"
        );
        std::thread::yield_now();
    }
    assert!(app.local_image_visible());
    let view = |app: &App| {
        app.local
            .as_ref()
            .unwrap()
            .reader
            .as_ref()
            .unwrap()
            .image_view()
    };
    tap(&mut app, KeyCode::Equal);
    assert_eq!(view(&app), [1.25, 0.0, 0.0], "image + remains zoom");
    tap(&mut app, KeyCode::ArrowRight);
    assert_eq!(view(&app), [1.25, 0.125, 0.0], "image Right remains pan");
    tap(&mut app, KeyCode::ArrowLeft);
    tap(&mut app, KeyCode::Minus);
    assert_eq!(view(&app), [1.0, 0.0, 0.0]);
    assert!(
        !app.world
            .document()
            .unwrap()
            .contains("AUDIO SEEK REQUEST:")
    );
    assert!(
        !app.world
            .document()
            .unwrap()
            .contains("AUDIO VOLUME REQUEST:")
    );
    assert!(!app.audio.busy());
    tap(&mut app, KeyCode::Escape);
    open_named(&mut app, &text_name);
    let text = app.world.document().unwrap().to_string();
    assert!(text.starts_with("TEXT: UTF-8 interpretation"));
    for code in [
        KeyCode::Equal,
        KeyCode::Minus,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Space,
    ] {
        tap(&mut app, code);
    }
    assert_eq!(
        app.world.document().unwrap(),
        text,
        "one-chunk text remains unchanged"
    );
    assert!(!app.audio.busy());
    tap(&mut app, KeyCode::Escape);
    drop(app);
    assert_eq!(
        inventory(&held),
        before,
        "held sources nonmutation including names/inodes/modes/bytes"
    );
    assert_eq!(
        inventory(&selected),
        decoy_before,
        "display-path decoy nonmutation"
    );
    assert!(
        missing.is_empty(),
        "LOCAL_AUDIO_SEEK_VOLUME_REQUEST_CONTROLS_MISSING: {missing:?}"
    );
}
