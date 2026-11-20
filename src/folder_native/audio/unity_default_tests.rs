//! CPU-only unity contract. The preserved external App/CPU-HUD gate is documented
//! in audio-unity-followup/HANDOFF.md; this module does not replace that oracle.
use super::super::raw_reader::Session;
use super::*;
use std::{fs, path::PathBuf};
use winit::keyboard::KeyCode;

fn wav(samples: [i16; 4]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44_u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(8000_u32.to_le_bytes());
    bytes.extend(16000_u32.to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8_u32.to_le_bytes());
    for sample in samples {
        bytes.extend(sample.to_le_bytes());
    }
    bytes
}

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn require_unity(session: &Session) {
    let state = session.audio.as_ref().unwrap();
    assert!(session.audio_visible());
    assert_eq!(
        state.controls,
        Controls {
            seconds: 0.0,
            volume: 100
        }
    );
    assert_eq!(gain(state.controls.volume).unwrap(), 1.0);
    assert_eq!(state.desired, Desired::Stopped);
    assert_eq!(state.observed, Observed::Ready);
    assert!(!state.requested && !cancelled(&state.token));
    assert!(state.info.is_none() && state.applied.is_none() && state.error.is_none());
    let document = session.document().unwrap();
    assert!(
        document
            .lines()
            .any(|line| line == "AUDIO VOLUME REQUEST: 100% (pending)")
    );
    assert!(document.contains("C resets to 0 s / 100%."));
    assert!(document.contains("default 100% (normal source level); system volume unchanged."));
    assert!(!document.contains("default 10%") && !document.contains("C resets to 0 s / 10%"));
}

#[test]
fn controls_default_is_source_unity_not_attenuation() {
    let controls = Controls::default();
    assert_eq!(
        controls,
        Controls {
            seconds: 0.0,
            volume: 100
        }
    );
    assert_eq!(gain(controls.volume).unwrap(), 1.0);
    let state = State::from_bytes(&wav([1234, -2345, 3456, -4567])).unwrap();
    assert_eq!(state.controls, controls);
    assert_eq!(state.desired, Desired::Stopped);
    assert_eq!(state.observed, Observed::Ready);
    assert!(!state.requested);
    assert!(state.info.is_none() && state.applied.is_none());
}

#[test]
fn held_reader_retry_resets_seek_and_volume_to_unity_without_play() {
    use std::os::unix::fs::MetadataExt;

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(
        std::env::temp_dir().join(format!("hair-unity-retry-{}-{unique}", std::process::id())),
    );
    fs::create_dir(&fixture.0).unwrap();
    let selected = fixture.0.join("selected");
    let held = fixture.0.join("held");
    fs::create_dir(&selected).unwrap();
    let encoded = wav([1234, -2345, 3456, -4567]);
    let decoy = wav([0; 4]);
    fs::write(selected.join("Owned Unity.WaV"), &encoded).unwrap();
    let explorer = hair::local_explorer::Explorer::new(&selected).unwrap();
    let mut session = Session::open(explorer.open_reader(0), "held unity fixture".into());
    require_unity(&session);
    fs::rename(&selected, &held).unwrap();
    fs::create_dir(&selected).unwrap();
    fs::write(selected.join("Owned Unity.WaV"), &decoy).unwrap();
    let stamp = |path: &std::path::Path| {
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
        )
    };
    let original_stamp = stamp(&held.join("Owned Unity.WaV"));
    let decoy_stamp = stamp(&selected.join("Owned Unity.WaV"));
    assert_ne!(original_stamp.1, decoy_stamp.1);
    let mut controller = Controller::default();
    for _ in 0..2 {
        let old_token = session.audio.as_ref().unwrap().token.clone();
        let _ = session.action(KeyCode::Minus, 0);
        let _ = session.action(KeyCode::ArrowRight, 0);
        assert_eq!(
            session.audio.as_ref().unwrap().controls,
            Controls {
                seconds: 5.0,
                volume: 95
            }
        );
        // Exercise the real same-held-handle retry, not a fabricated State reset.
        let _ = session.action(KeyCode::KeyC, 0);
        assert!(cancelled(&old_token));
        assert!(!Arc::ptr_eq(
            &old_token,
            &session.audio.as_ref().unwrap().token
        ));
        require_unity(&session);
        let state = session.audio.as_ref().unwrap();
        controller.poll_controls(Some(&state.token), state.desired, state.controls);
        assert!(!controller.busy() && !controller.owns(&state.token));
        assert!(controller.snapshot(&state.token).is_none());
        assert!(controller.job.is_none() && controller.track.is_none());
        assert_eq!(controller.observed, Observed::Ready);
        let chunk = session.audio_source().unwrap().read_chunk(0).unwrap();
        assert_eq!(chunk.body(), encoded.as_slice());
        let decoded = pcm::decode(chunk.body(), &AtomicBool::new(false)).unwrap();
        assert_eq!(decoded.samples, [1234, -2345, 3456, -4567]);
    }
    drop(session);
    drop(explorer);
    assert_eq!(stamp(&held.join("Owned Unity.WaV")), original_stamp);
    assert_eq!(stamp(&selected.join("Owned Unity.WaV")), decoy_stamp);
    assert_eq!(fs::read(held.join("Owned Unity.WaV")).unwrap(), encoded);
    assert_eq!(fs::read(selected.join("Owned Unity.WaV")).unwrap(), decoy);
}
