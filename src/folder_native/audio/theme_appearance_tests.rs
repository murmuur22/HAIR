//! Silent software-intent regression, NOT active device-playback acceptance.
//! Canonical macOS cfg(test) child of folder_native::audio; parent verification pending.
//! Real App keys, held-file acquisition and PCM; no fake Output, clocks or state writes.
use super::*;
use crate::{App, KeyCode, Path, PathBuf, Pose, World};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

const NAME: &str = "Theme Audio.WaV";

fn wav(sample: i16) -> Vec<u8> {
    // Twenty seconds allows a real nonzero 5-second prepared seek before T.
    let data_bytes = 160_000_u32 * 2;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data_bytes).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(8000_u32.to_le_bytes());
    bytes.extend(16000_u32.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_bytes.to_le_bytes());
    for _ in 0..160_000 {
        bytes.extend(sample.to_le_bytes());
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

type FileState = (u64, u64, u32, u64, i64, i64, i64, i64, Vec<u8>);
fn file_state(path: &Path) -> FileState {
    let m = fs::symlink_metadata(path).unwrap();
    assert!(m.is_file() && !m.file_type().is_symlink());
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
        fs::read(path).unwrap(),
    )
}

fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!app.key(code, down, false).unwrap());
    }
}

fn state(app: &App) -> &State {
    app.local
        .as_ref()
        .unwrap()
        .reader
        .as_ref()
        .unwrap()
        .audio
        .as_ref()
        .unwrap()
}

fn prefix(app: &App) -> Vec<u8> {
    app.local
        .as_ref()
        .unwrap()
        .reader
        .as_ref()
        .unwrap()
        .audio_source()
        .unwrap()
        .read_chunk(0)
        .unwrap()
        .body()
        .to_vec()
}

fn silent_boundary() {
    // Same no-device tripwire convention as audio/controls_tests.rs.
    // Linkage to frameworks is not claimed absent; parent additionally audits symbols.
    let source = include_str!("../audio.rs");
    let gate = "#[cfg(all(target_os = \"macos\", not(test)))]";
    assert_eq!(source.matches(gate).count(), 4);
    for declaration in [
        "struct Output {",
        "unsafe extern \"C\" {",
        "impl Output {",
        "impl Drop for Output {",
    ] {
        assert!(source.contains(&format!("{gate}\n{declaration}")));
    }
    let denied = source
        .split_once("#[cfg(any(test, not(target_os = \"macos\")))]\nimpl Output {")
        .unwrap()
        .1
        .split_once("\n#[cfg(test)]\n#[path =")
        .unwrap()
        .0;
    assert!(!denied.contains("unsafe") && !denied.contains("hair_pcm_"));
    assert!(denied.contains("\"DENIED_TEST_AUDIO_OUTPUT\""));
    for method in ["fn gain(", "fn set(", "fn state("] {
        let body = denied
            .split_once(method)
            .unwrap()
            .1
            .split_once("\n    }")
            .unwrap()
            .0;
        assert!(body.contains("Err(\"audio output denied\".into())"));
    }
    let p = pcm::decode(&wav(1234), &AtomicBool::new(false)).unwrap();
    assert!(matches!(Output::new_at(&p, 0), Err((ref e, None)) if e == "DENIED_TEST_AUDIO_OUTPUT"));
}

fn no_output(app: &App) {
    assert!(app.renderer.is_none() && app.window.is_none() && app.surface.is_none());
    assert!(!app.audio.disabled() && app.audio.error.is_none());
    if let Some(track) = &app.audio.track {
        assert!(track.output.is_none());
        assert_eq!(track.queue_gain, None);
    }
}

#[test]
fn ordinary_t_preserves_explicit_play_intent_and_held_audio_generation() {
    use sha2::{Digest, Sha256};
    silent_boundary();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let tmp = std::env::temp_dir().canonicalize().unwrap();
    assert_eq!(tmp.parent(), root.parent());
    assert_ne!(tmp, root);
    // Retained even on semantic RED: parent inventories and hashes these exact files.
    let fixture = tmp.join(format!("theme-audio-{}", std::process::id()));
    fs::create_dir(&fixture).unwrap();
    let selected = fixture.join("selected");
    let held = fixture.join("held");
    fs::create_dir(&selected).unwrap();
    let encoded = wav(1234);
    let decoy = wav(-2345);
    exclusive(&selected.join(NAME), &encoded);
    let mut cache = hair::theme::AssetCache::default();
    let themes = ["packs/office.toml", "packs/archive.toml"]
        .into_iter()
        .map(|p| hair::theme::Theme::load(&root.join(p), &mut cache).unwrap())
        .collect();
    let mut app = App::new(root.clone(), themes, 0, World::load(&root).unwrap(), false);
    app.frames = 1; // Existing chooser completion seam, no native panel/window.
    tap(&mut app, KeyCode::KeyO);
    assert!(app.claim_folder_request());
    app.complete_folder(Ok(Some(selected.clone()))).unwrap();
    assert_ne!(app.themes[0].name, app.themes[1].name);
    assert_eq!(app.themes[0].host, app.themes[1].host);
    for theme in &app.themes {
        assert!(!theme.load_failed);
        assert!(
            !theme
                .diagnostics
                .iter()
                .any(|d| d.starts_with("ROLE_FALLBACK")),
            "genuine complete legacy pack required for setup: {:?}",
            theme.diagnostics
        );
    }
    // Real directory capability survives path replacement BEFORE E acquisition.
    fs::rename(&selected, &held).unwrap();
    fs::create_dir(&selected).unwrap();
    exclusive(&selected.join(NAME), &decoy);
    let original = file_state(&held.join(NAME));
    let replacement = file_state(&selected.join(NAME));
    assert_ne!(original.1, replacement.1);
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == NAME)
        .unwrap()
        .0
        .clone();
    let host = &app.themes[app.theme_index].host;
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
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    assert!(
        app.local
            .as_ref()
            .unwrap()
            .reader
            .as_ref()
            .unwrap()
            .audio_visible()
    );
    app.poll_local_audio().unwrap();
    assert_eq!(state(&app).desired, Desired::Stopped);
    assert!(!state(&app).requested && !app.audio.busy());
    tap(&mut app, KeyCode::KeyT); // Appearance action itself is not Play.
    app.poll_local_audio().unwrap();
    assert_eq!(state(&app).desired, Desired::Stopped);
    assert!(!state(&app).requested && !app.audio.busy());
    assert_eq!(prefix(&app), encoded[..4096]);

    tap(&mut app, KeyCode::Space);
    assert_eq!(state(&app).desired, Desired::Play);
    assert_eq!(state(&app).observed, Observed::PlayRequested);
    let token = state(&app).token.clone();
    // Established SETUP EXCEPTION: start the real acquisition after actual Play,
    // then actual Pause BEFORE completion-consumer polling. Never fabricate PCM.
    let source = app
        .local
        .as_ref()
        .unwrap()
        .reader
        .as_ref()
        .unwrap()
        .audio_source()
        .unwrap();
    app.audio
        .acquire_after_explicit_play(token.clone(), source)
        .unwrap();
    tap(&mut app, KeyCode::Space);
    assert_eq!(state(&app).desired, Desired::Pause);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    for _ in 0..600 {
        if app.audio.job.as_ref().unwrap().handle.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker setup deadline, NOT theme RED"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        app.audio.job.as_ref().unwrap().handle.is_finished(),
        "finite worker setup budget"
    );
    app.poll_local_audio().unwrap();
    assert_eq!(state(&app).observed, Observed::Prepared);
    tap(&mut app, KeyCode::ArrowRight);
    tap(&mut app, KeyCode::Minus);
    app.poll_local_audio().unwrap();
    assert_eq!(
        state(&app).controls,
        Controls {
            seconds: 5.0,
            volume: 95
        }
    );
    assert_eq!(
        state(&app).applied,
        Some(Applied {
            frame: 40_000,
            queue_gain: None
        })
    );
    let track = app.audio.track.as_ref().unwrap();
    assert_eq!(track.pcm.frames, 160_000);
    assert_eq!(track.pcm.rate, 8000);
    assert_eq!(track.pcm.duration, 20.0);
    assert!(track.pcm.samples.iter().all(|s| *s == 1234));
    assert_eq!(
        track._source.read_chunk(0).unwrap().body(),
        &encoded[..4096]
    );
    let pcm_address = track.pcm.samples.as_ptr();
    tap(&mut app, KeyCode::Space); // Real active software Play intent on retained PCM.
    assert_eq!(state(&app).desired, Desired::Play);
    assert_eq!(state(&app).observed, Observed::PlayRequested);
    assert!(state(&app).requested && !cancelled(&token) && app.audio.owns(&token));
    no_output(&app);
    let before = (
        state(&app).desired,
        state(&app).observed,
        state(&app).requested,
        state(&app).controls,
        state(&app).info,
        state(&app).applied,
        state(&app).body,
        state(&app).error.clone(),
    );
    let controller_before = app.audio.observed;
    let document = app.world.document().unwrap().to_string();
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    println!(
        "THEME_AUDIO_SETUP_OK fixture={} wav_sha256={:x} decoy_sha256={:x}",
        fixture.display(),
        Sha256::digest(&encoded),
        Sha256::digest(&decoy)
    );

    // The ordinary production App::key branch is the subject. No renderer is
    // constructed: current T cancels BEFORE its renderer-conditional reskin.
    // This detects that cancellation, not a rendered colour/Metal/theme-index change.
    tap(&mut app, KeyCode::KeyT);
    let mut missing = Vec::new();
    let after = (
        state(&app).desired,
        state(&app).observed,
        state(&app).requested,
        state(&app).controls,
        state(&app).info,
        state(&app).applied,
        state(&app).body,
        state(&app).error.clone(),
    );
    if after != before {
        missing.push(format!("audio state before={before:?} after={after:?}"));
    }
    if cancelled(&token) || !Arc::ptr_eq(&token, &state(&app).token) {
        missing.push("existing token cancelled or replaced".into());
    }
    if !app.audio.owns(&token) || app.audio.job.is_some() {
        missing.push("retained owner/job changed".into());
    }
    if app.audio.observed != controller_before {
        missing.push("controller observation changed".into());
    }
    if app.world.document().unwrap() != document {
        missing.push("reader audio document changed".into());
    }
    if !app.world.reading || (app.pose.position, app.pose.yaw, app.pose.pitch) != pose {
        missing.push("reader/pose changed".into());
    }
    if prefix(&app) != encoded[..4096] {
        missing.push("held reader source changed".into());
    }
    match &app.audio.track {
        Some(track)
            if track.pcm.samples.as_ptr() == pcm_address
                && Arc::ptr_eq(&track.token, &token)
                && track.start == 40_000
                && track.pcm.samples.iter().all(|s| *s == 1234)
                && track._source.read_chunk(0).unwrap().body() == &encoded[..4096] => {}
        _ => missing.push("decoded allocation/source/token/prepared seek changed".into()),
    }
    no_output(&app);
    // NEVER poll retained Play at a nonterminal target: denied Output would be
    // an unrelated error. Intent immediately after T is the narrow silent oracle.

    // Safety controls run even on intended RED; preserving T must not weaken stop.
    // On today's RED, T already stopped it. Re-arm by a real Space, never a bool,
    // so explicit stop is independently checked against uncancelled Play intent.
    if state(&app).desired != Desired::Play {
        tap(&mut app, KeyCode::Space);
    }
    let stop_token = state(&app).token.clone();
    assert_eq!(state(&app).desired, Desired::Play);
    assert!(state(&app).requested && !cancelled(&stop_token));
    app.stop_local_audio();
    assert!(cancelled(&token) && cancelled(&stop_token));
    assert_eq!(state(&app).desired, Desired::Stopped);
    assert!(!state(&app).requested);
    app.poll_local_audio().unwrap();
    assert!(!app.audio.busy());
    tap(&mut app, KeyCode::Space);
    let focus_token = state(&app).token.clone();
    assert!(!Arc::ptr_eq(&stop_token, &focus_token));
    assert_eq!(state(&app).desired, Desired::Play);
    assert!(state(&app).requested && !cancelled(&focus_token));
    app.focus_changed(false);
    assert!(cancelled(&focus_token));
    assert_eq!(state(&app).desired, Desired::Stopped);
    assert!(!state(&app).requested);
    app.focus_changed(true);
    app.poll_local_audio().unwrap();
    assert_eq!(state(&app).desired, Desired::Stopped);
    assert!(!state(&app).requested && !app.audio.busy());
    tap(&mut app, KeyCode::Space); // Focus regain alone did not restart.
    let escape_token = state(&app).token.clone();
    assert_eq!(state(&app).desired, Desired::Play);
    assert!(state(&app).requested && !cancelled(&escape_token));
    tap(&mut app, KeyCode::Escape);
    assert!(cancelled(&escape_token) && !app.world.reading);
    assert!(app.local.as_ref().unwrap().reader.is_none());
    app.poll_local_audio().unwrap();
    assert!(!app.audio.busy());
    no_output(&app);
    drop(app);
    assert_eq!(
        file_state(&held.join(NAME)),
        original,
        "held source nonmutation"
    );
    assert_eq!(
        file_state(&selected.join(NAME)),
        replacement,
        "decoy nonmutation"
    );
    println!(
        "THEME_AUDIO_CONTROLS_OK explicit_stop=true focus_loss=true escape=true no_autoplay=true source_nonmutation=true"
    );
    assert!(missing.is_empty(), "THEME_AUDIO_INTENT_RED: {missing:?}");
    println!(
        "THEME_AUDIO_INTENT_OK: software request/generation preservation only; no native playing, continuity, sound, GPU or package proof"
    );
}
