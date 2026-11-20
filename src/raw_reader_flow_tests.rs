use super::*;
use std::fs;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn tap(app: &mut App, code: KeyCode) {
    app.keyboard_input(Some(code), true, false, None).unwrap();
    app.keyboard_input(Some(code), false, false, None).unwrap();
}

#[test]
fn binary_reader_shows_exact_hex_and_browses_bounded_chunks_without_page_keys() {
    let path = std::env::temp_dir().join(format!("hair-raw-chunks-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    let fixture = Fixture(path);
    let mut bytes = vec![0_u8; 8193];
    bytes[..8].copy_from_slice(&[0, 1, 2, 3, 0x41, 0x7f, 0x80, 0xff]);
    bytes[4096..4104].copy_from_slice(b"SECOND!!");
    bytes[8192] = 0xee;
    fs::write(fixture.0.join("Stored Data.bin"), &bytes).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme =
        hair::theme::Theme::load(&root.join("packs/office.toml"), &mut Default::default()).unwrap();
    let mut app = App::new(
        root.clone(),
        vec![theme],
        0,
        World::load(&root).unwrap(),
        false,
    );
    app.complete_folder(Ok(Some(fixture.0.canonicalize().unwrap())))
        .unwrap();
    let anchor = app.themes[0]
        .host
        .anchors
        .iter()
        .find(|a| a.id == "entry-0")
        .unwrap()
        .clone();
    app.pose = Pose {
        position: [anchor.at[0], anchor.at[2] + 1.05],
        yaw: 0.0,
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    app.world.explicit_target = None;
    hair::interaction::refresh(&app.themes[0].host, &app.pose, &mut app.world);
    assert_eq!(
        app.world.interaction.as_ref().map(|t| t.anchor.as_str()),
        Some("entry-0")
    );
    tap(&mut app, KeyCode::KeyE);
    assert!(app.world.reading);
    let first = app.world.document().unwrap().to_string();
    assert!(
        first.contains("RAW STORED BYTES"),
        "binary E must expose stored bytes, not unsupported/refused UTF-8 metadata: {first:?}"
    );
    assert!(first.contains("0000000000000000"));
    assert!(first.contains("00 01 02 03 41 7F 80 FF"));
    assert!(
        !first.contains("53 45 43 4F 4E 44 21 21"),
        "first chunk must not preload next chunk"
    );
    tap(&mut app, KeyCode::ArrowRight);
    let second = app.world.document().unwrap();
    assert!(second.contains("0000000000001000"));
    assert!(second.contains("53 45 43 4F 4E 44 21 21"));
    tap(&mut app, KeyCode::ArrowRight);
    let last = app.world.document().unwrap().to_string();
    assert!(last.contains("0000000000002000") && last.contains("EE"));
    assert!(last.contains("EOF"));
    tap(&mut app, KeyCode::ArrowRight);
    assert_eq!(app.world.document().unwrap(), last);
    tap(&mut app, KeyCode::ArrowLeft);
    assert!(app.world.document().unwrap().contains("0000000000001000"));
    tap(&mut app, KeyCode::ArrowLeft);
    assert_eq!(app.world.document().unwrap(), first);
    tap(&mut app, KeyCode::KeyV);
    assert!(app.world.document().unwrap().contains("TEXT"));
    assert!(app.world.document().unwrap().contains("\\x00"));
    assert!(!app.world.document().unwrap().contains('\0'));
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    assert_eq!(fs::read(fixture.0.join("Stored Data.bin")).unwrap(), bytes);
}
