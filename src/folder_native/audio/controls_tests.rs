//! Additional CPU-only regressions. Default Output remains denied.
use super::*;

fn wav() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44_u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(8000_u32.to_le_bytes());
    bytes.extend(32000_u32.to_le_bytes());
    bytes.extend(4_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8_u32.to_le_bytes());
    for n in [123_i16, -456, 789, -321] {
        bytes.extend(n.to_le_bytes());
    }
    bytes
}
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("hair-controls-{}-{n}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("owned.wav"), wav()).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn rapid_physical_play_pause_before_first_app_poll_does_not_decode() {
    use crate::*;
    let f = Fixture::new();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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
    let tap = |app: &mut App, code| {
        for down in [true, false] {
            assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
        }
    };
    tap(&mut app, KeyCode::KeyO);
    assert!(app.claim_folder_request());
    app.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "owned.wav")
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
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    tap(&mut app, KeyCode::Space);
    assert!(
        app.world
            .document()
            .unwrap()
            .contains("AUDIO STATUS: PLAY REQUESTED")
    );
    tap(&mut app, KeyCode::Space);
    assert!(
        app.world
            .document()
            .unwrap()
            .contains("AUDIO STATUS: PAUSE REQUESTED")
    );
    assert!(!app.audio.busy());
    app.poll_local_audio().unwrap();
    assert!(
        !app.audio.busy(),
        "rapid pause must not acquire/decode solely to pause"
    );
    assert_eq!(app.audio.observed, Observed::Ready);
    let state = app
        .local
        .as_ref()
        .unwrap()
        .reader
        .as_ref()
        .unwrap()
        .audio
        .as_ref()
        .unwrap();
    assert_eq!(state.desired, Desired::Stopped);
    assert!(!state.requested);
    assert!(state.info.is_none());
    assert_eq!(std::fs::read(f.0.join("owned.wav")).unwrap(), wav());
}

#[test]
fn validated_stereo_source_frame_offsets_and_gain_are_finite_and_bounded() {
    let p = pcm::decode(&wav(), &AtomicBool::new(false)).unwrap();
    assert_eq!(frame_for_seconds(&p, 0.0).unwrap(), 0);
    assert_eq!(frame_for_seconds(&p, 1.0 / 8000.0).unwrap(), 1);
    assert_eq!(pcm_tail(&p, 1).unwrap(), &[789, -321]);
    assert_eq!(frame_for_seconds(&p, 300.0).unwrap(), 2);
    assert!(pcm_tail(&p, 2).unwrap().is_empty());
    assert!(pcm_tail(&p, 3).is_err());
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert!(frame_for_seconds(&p, n).is_err());
    }
    for n in [0, 5, 10, 100] {
        assert_eq!(gain(n).unwrap(), f32::from(n) / 100.0);
    }
    assert!(gain(101).is_err());
    let mut bad = p;
    bad.frames = usize::MAX;
    assert!(pcm_tail(&bad, 0).is_err());
}

#[test]
fn real_worker_prepared_seek_tail_replay_and_cancel_do_not_autostart() {
    let f = Fixture::new();
    let explorer = hair::local_explorer::Explorer::new(&f.0).unwrap();
    let token = Arc::new(AtomicBool::new(false));
    let mut c = Controller::default();
    c.acquire_after_explicit_play(token.clone(), explorer.open_reader(0).unwrap())
        .unwrap();
    while !c.job.as_ref().unwrap().handle.is_finished() {
        std::thread::yield_now();
    }
    let mut controls = Controls {
        seconds: 1.0 / 8000.0,
        volume: 0,
    };
    c.poll_controls(Some(&token), Desired::Pause, controls);
    assert_eq!(c.observed, Observed::Prepared);
    assert_eq!(c.track.as_ref().unwrap().start, 1);
    assert!(c.track.as_ref().unwrap().output.is_none());
    assert_eq!(
        c.track.as_ref().unwrap().pcm.samples,
        [123, -456, 789, -321]
    );
    controls.seconds = 300.0;
    c.poll_controls(Some(&token), Desired::Pause, controls);
    assert_eq!(c.track.as_ref().unwrap().start, 2);
    assert!(c.track.as_ref().unwrap().output.is_none());
    assert_eq!(c.observed, Observed::Prepared);
    // Explicit Play at an end target ends without opening an empty queue.
    c.poll_controls(Some(&token), Desired::Play, controls);
    assert_eq!(c.observed, Observed::Ended);
    assert!(c.track.as_ref().unwrap().output.is_none());
    controls.seconds = 0.0;
    c.poll_controls(Some(&token), Desired::Stopped, controls);
    assert_eq!(c.observed, Observed::Prepared);
    assert!(c.track.as_ref().unwrap().output.is_none());
    // A seek/volume poll cannot revive a cancelled generation, even with Play.
    token.store(true, Ordering::Release);
    c.poll_controls(Some(&token), Desired::Play, controls);
    assert!(c.track.is_none());
    assert_eq!(c.observed, Observed::Ready);
    assert!(!c.busy());
}

// AC-R1 uses the ordinary cfg(test) denied Output, NOT the reviewer's isolated
// no-framework probe. These source checks are tripwires in addition to the
// parent's required undefined-API audit; AudioToolbox linkage is NOT excluded.
fn ended_order_assert_cpu_boundary() {
    let source = include_str!("../audio.rs");
    let native_gate = "#[cfg(all(target_os = \"macos\", not(test)))]";
    assert_eq!(source.matches(native_gate).count(), 4);
    for declaration in [
        "struct Output {",
        "unsafe extern \"C\" {",
        "impl Output {",
        "impl Drop for Output {",
    ] {
        assert!(source.contains(&format!("{native_gate}\n{declaration}")));
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
    assert!(denied.contains("fn new_at("));
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
    assert!(source.contains("fn default_test_output_is_denied_not_simulated()"));
}

fn ended_order_wav(sample: i16) -> Vec<u8> {
    // Owned mono PCM16LE, 160000 frames / 8000 Hz = exactly twenty seconds.
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

fn ended_order_tap(app: &mut crate::App, code: winit::keyboard::KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
    }
}

fn ended_order_state(app: &crate::App) -> &State {
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

fn ended_order_retained(app: &crate::App, token: &Token, pcm_address: *const i16) {
    assert!(app.audio.job.is_none(), "no second acquisition job");
    assert!(!app.audio.disabled() && app.audio.error.is_none());
    assert!(app.audio.owns(token));
    let track = app.audio.track.as_ref().unwrap();
    assert!(Arc::ptr_eq(&track.token, token));
    assert!(!cancelled(token));
    assert!(
        track.output.is_none(),
        "endpoint/stopped polls must never open output"
    );
    assert_eq!(track.queue_gain, None);
    assert_eq!(
        track.pcm.samples.as_ptr(),
        pcm_address,
        "retain the same decoded allocation"
    );
    assert_eq!(track.pcm.frames, 160_000);
    assert_eq!(track.pcm.rate, 8000);
    assert_eq!(track.pcm.duration, 20.0);
    assert_eq!(track.pcm.samples.len(), 160_000);
    assert!(
        track.pcm.samples.iter().all(|sample| *sample == 1234),
        "held original, not display-path decoy"
    );
    assert_eq!(track._source.read_chunk(0).unwrap().observed_len, 320_044);
    let state = ended_order_state(app);
    assert!(Arc::ptr_eq(&state.token, token));
    assert!(state.error.is_none());
    assert_eq!(state.info.unwrap().duration, 20.0);
    assert_eq!(state.applied.unwrap().queue_gain, None);
    assert!(
        app.renderer.is_none(),
        "CPU App only; no device/renderer setup"
    );
}

#[test]
fn ended_seek_then_space_is_independent_of_app_poll() {
    use crate::*;
    use std::os::unix::fs::MetadataExt;

    ended_order_assert_cpu_boundary();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let workspace = root.parent().unwrap();
    assert_eq!(
        workspace,
        std::path::Path::new(
            &std::env::var("HAIR_TEST_WORKSPACE")
                .expect("explicit owned qualification workspace required")
        )
    );
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture_path = workspace
        .join("audio-repair")
        .join(format!("ended-order-{}-{unique}", std::process::id()));
    std::fs::create_dir(&fixture_path).unwrap();
    let fixture = Fixture(fixture_path);
    let selected = fixture.0.join("selected");
    let held = fixture.0.join("held");
    std::fs::create_dir(&selected).unwrap();
    let encoded = ended_order_wav(1234);
    let decoy = ended_order_wav(-2345);
    std::fs::write(selected.join("owned.wav"), &encoded).unwrap();
    let identity = |path: &std::path::Path| {
        let m = std::fs::symlink_metadata(path).unwrap();
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
        )
    };

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
    ended_order_tap(&mut app, KeyCode::KeyO);
    assert!(app.claim_folder_request());
    app.complete_folder(Ok(Some(selected.clone()))).unwrap();
    // Keep the selected directory capability; replace its DISPLAY path before E.
    std::fs::rename(&selected, &held).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("owned.wav"), &decoy).unwrap();
    let original_identity = identity(&held.join("owned.wav"));
    let decoy_identity = identity(&selected.join("owned.wav"));
    assert_ne!(original_identity.1, decoy_identity.1);

    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == "owned.wav")
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
    ended_order_tap(&mut app, KeyCode::KeyE);
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
    assert!(!app.audio.busy());

    // SETUP EXCEPTION: launch exactly one REAL held-file worker through the
    // existing acquisition API after physical Play, then physically Pause the
    // pending job BEFORE ANY completion-consumer App poll. This controls intent,
    // not scheduling: the OS worker may already finish, but is not joined yet.
    // No fake decoded result, injected observation, worker gate or success Output.
    ended_order_tap(&mut app, KeyCode::Space);
    let token = ended_order_state(&app).token.clone();
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
    assert!(app.audio.job.is_some() && app.audio.track.is_none());
    ended_order_tap(&mut app, KeyCode::Space);
    assert_eq!(ended_order_state(&app).desired, Desired::Pause);
    assert_eq!(ended_order_state(&app).observed, Observed::PauseRequested);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    for _ in 0..600 {
        if app.audio.job.as_ref().unwrap().handle.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "real worker setup deadline, not semantic RED"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        app.audio.job.as_ref().unwrap().handle.is_finished(),
        "finite real worker setup budget"
    );
    app.poll_local_audio().unwrap();
    assert_eq!(ended_order_state(&app).observed, Observed::Prepared);
    assert_eq!(ended_order_state(&app).desired, Desired::Pause);
    let pcm_address = app.audio.track.as_ref().unwrap().pcm.samples.as_ptr();
    ended_order_retained(&app, &token, pcm_address);
    for _ in 0..4 {
        ended_order_tap(&mut app, KeyCode::ArrowRight);
    }
    assert_eq!(ended_order_state(&app).controls.seconds, 20.0);
    app.poll_local_audio().unwrap(); // Pause prepares the known-duration end offset.
    assert_eq!(app.audio.track.as_ref().unwrap().start, 160_000);
    ended_order_retained(&app, &token, pcm_address);

    let cases = [
        ("effective Left", Some(KeyCode::ArrowLeft), 15.0),
        ("plain replay", None, 0.0),
        ("volume increase only", Some(KeyCode::Equal), 0.0),
        ("volume decrease only", Some(KeyCode::Minus), 0.0),
        ("duration-clamped Right", Some(KeyCode::ArrowRight), 0.0),
        ("effective Left then Right", Some(KeyCode::ArrowLeft), 20.0),
    ];
    let mut results = Vec::new();
    for (label, adjustment, expected) in cases {
        for stopped_poll in [0, 1, 3] {
            // First entry is paused at end. Later entries were left with explicit
            // Play pending at end by the reset below. Every actual Play POLL is
            // at the exact decoded endpoint; none can attempt Output::new_at.
            if ended_order_state(&app).desired == Desired::Pause {
                ended_order_tap(&mut app, KeyCode::Space);
            }
            assert_eq!(ended_order_state(&app).desired, Desired::Play);
            assert_eq!(ended_order_state(&app).controls.seconds, 20.0);
            app.poll_local_audio().unwrap();
            assert_eq!(app.audio.observed, Observed::Ended);
            assert_eq!(ended_order_state(&app).observed, Observed::Ended);
            assert_eq!(ended_order_state(&app).desired, Desired::Stopped);
            assert!(!ended_order_state(&app).requested);
            assert!(app.audio.track.as_ref().unwrap().ended);
            assert_eq!(app.audio.track.as_ref().unwrap().start, 160_000);
            ended_order_retained(&app, &token, pcm_address);
            app.poll_local_audio().unwrap(); // ordinary later EOF poll cannot autoplay
            assert_eq!(ended_order_state(&app).desired, Desired::Stopped);
            assert_eq!(ended_order_state(&app).observed, Observed::Ended);
            ended_order_retained(&app, &token, pcm_address);

            // Unity is now the default ceiling. Make the increase-only case an
            // effective +5 press rather than accidentally testing saturation.
            if adjustment == Some(KeyCode::Equal) {
                ended_order_tap(&mut app, KeyCode::Minus);
                assert!(ended_order_state(&app).controls.volume <= 95);
                assert_eq!(ended_order_state(&app).desired, Desired::Stopped);
                assert!(!ended_order_state(&app).requested);
                ended_order_retained(&app, &token, pcm_address);
            }
            let prior_volume = ended_order_state(&app).controls.volume;
            if let Some(code) = adjustment {
                ended_order_tap(&mut app, code); // physical press AND release
            }
            if label == "effective Left then Right" {
                ended_order_tap(&mut app, KeyCode::ArrowRight);
            }
            let adjusted_seconds = if label == "effective Left" {
                15.0
            } else {
                20.0
            };
            assert_eq!(ended_order_state(&app).controls.seconds, adjusted_seconds);
            let adjusted_volume = match adjustment {
                Some(KeyCode::Equal) => prior_volume + 5,
                Some(KeyCode::Minus) => prior_volume - 5,
                _ => prior_volume,
            };
            assert_eq!(ended_order_state(&app).controls.volume, adjusted_volume);
            assert_eq!(ended_order_state(&app).desired, Desired::Stopped);
            assert!(!ended_order_state(&app).requested);
            assert_eq!(app.audio.track.as_ref().unwrap().start, 160_000);
            ended_order_retained(&app, &token, pcm_address);
            for _ in 0..stopped_poll {
                app.poll_local_audio().unwrap(); // REAL App publication, no mirrored mapping
                assert_eq!(ended_order_state(&app).desired, Desired::Stopped);
                assert!(!ended_order_state(&app).requested);
                let prepared = if label == "effective Left" {
                    120_000
                } else {
                    160_000
                };
                assert_eq!(app.audio.track.as_ref().unwrap().start, prepared);
                assert_eq!(ended_order_state(&app).applied.unwrap().frame, prepared);
                ended_order_retained(&app, &token, pcm_address);
            }
            ended_order_tap(&mut app, KeyCode::Space);
            let state = ended_order_state(&app);
            assert_eq!(state.desired, Desired::Play);
            assert_eq!(state.observed, Observed::PlayRequested);
            assert!(state.requested);
            assert_eq!(state.controls.volume, adjusted_volume);
            results.push((label, stopped_poll, state.controls.seconds, expected));
            ended_order_retained(&app, &token, pcm_address);
            // NO output poll at this final explicit Space target. Reset only by
            // physical Right presses while Play is pending, before the next poll.
            for _ in 0..4 {
                ended_order_tap(&mut app, KeyCode::ArrowRight);
            }
            assert_eq!(ended_order_state(&app).controls.seconds, 20.0);
        }
    }
    drop(app);
    assert_eq!(identity(&held.join("owned.wav")), original_identity);
    assert_eq!(identity(&selected.join("owned.wav")), decoy_identity);
    assert_eq!(std::fs::read(held.join("owned.wav")).unwrap(), encoded);
    assert_eq!(std::fs::read(selected.join("owned.wav")).unwrap(), decoy);
    assert_eq!(results.len(), 18);
    assert!(
        results
            .iter()
            .all(|(_, _, actual, expected)| actual == expected),
        "AUDIO_ENDED_SEEK_REPLAY_POLL_ORDER: (case, stopped poll, actual seconds, required seconds) = {results:?}"
    );
}
