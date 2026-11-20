//! Non-GPU qualification through the real App input/poll path and native held files.
//! Registered by the existing cfg(test) declaration in folder_native.rs.
use super::*;
use std::{
    fs,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR"))
            .canonicalize()
            .unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()) && !base.starts_with(&source));
        let path = base.join(format!(
            "hair-png-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("exclusive owned fixture");
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.0.join(name))
            .unwrap();
        file.write_all(bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn png_bytes(width: u32, color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, 1);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    out
}
fn good() -> Vec<u8> {
    png_bytes(1, png::ColorType::Rgba, &[19, 37, 83, 109])
}
fn second() -> Vec<u8> {
    png_bytes(2, png::ColorType::Rgb, &[201, 7, 129, 13, 203, 47])
}
fn app_for(fixture: &Fixture) -> App {
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
    app.complete_folder(Ok(Some(fixture.0.clone()))).unwrap();
    assert!(app.renderer.is_none() && app.web.is_none());
    app
}
fn tap(app: &mut App, code: KeyCode) {
    for down in [true, false] {
        assert!(!app.keyboard_input(Some(code), down, false, None).unwrap());
    }
}
fn aim(app: &mut App, name: &str) -> String {
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, bound)| bound.entry.name == name)
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
    assert!(!host.blocked(app.pose.position));
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
    id
}
fn drain(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.poll_local_image().unwrap();
        if !app.local_image_pending() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "finite real App PNG completion deadline"
        );
        std::thread::yield_now();
    }
}
#[test]
fn image_pan_release_in_metadata_does_not_latch_the_next_press() {
    let fixture = Fixture::new();
    fixture.write("view.png", &good());
    let mut app = app_for(&fixture);
    aim(&mut app, "view.png");
    tap(&mut app, KeyCode::KeyE);
    drain(&mut app);
    tap(&mut app, KeyCode::Equal);
    app.keyboard_input(Some(KeyCode::ArrowDown), true, false, None)
        .unwrap();
    let first = session(&app).image_view();
    tap(&mut app, KeyCode::KeyI);
    app.keyboard_input(Some(KeyCode::ArrowDown), false, false, None)
        .unwrap();
    tap(&mut app, KeyCode::KeyI);
    tap(&mut app, KeyCode::ArrowDown);
    assert!(
        session(&app).image_view()[2] > first[2],
        "released pan key must not remain latched across metadata"
    );
}

#[test]
fn png_object_returns_after_reader_escape_without_manual_refresh() {
    let fixture = Fixture::new();
    fixture.write("preview.png", &good());
    let mut app = app_for(&fixture);
    let id = aim(&mut app, "preview.png");
    for cycle in 0..2 {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !app.object_previews.cache.contains_key(&id) {
            app.poll_local_image().unwrap();
            assert!(
                Instant::now() < deadline,
                "object preview missing after reader close, cycle={cycle}"
            );
            std::thread::yield_now();
        }
        assert!(!app.world.reading);
        assert_eq!(app.object_previews.cache[&id].rgba, [19, 37, 83, 109]);
        if cycle == 0 {
            tap(&mut app, KeyCode::KeyE);
            drain(&mut app);
            assert!(app.local_image_visible());
            tap(&mut app, KeyCode::Escape);
            assert!(!app.world.reading);
        }
    }
}

fn worker_result(
    runtime: &mut image::Runtime,
) -> (image::Token, hair::Result<hair::assets::Texture>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = runtime.poll() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "finite actual held-file worker deadline"
        );
        std::thread::yield_now();
    }
}
fn session(app: &App) -> &raw_reader::Session {
    app.local.as_ref().unwrap().reader.as_ref().unwrap()
}
fn pixels(app: &App, width: u32, rgba: &[u8]) {
    let texture = session(app)
        .image_texture()
        .expect("real acquired/decoded CPU texture");
    assert_eq!((texture.width, texture.height), (width, 1));
    assert_eq!(texture.rgba, rgba);
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

#[test]
fn png_qual_app_completion_modes_modal_keys_focus_and_source_are_non_destructive() {
    let fixture = Fixture::new();
    let name = "source\né.not-png";
    let encoded = good();
    fixture.write(name, &encoded);
    let mut app = app_for(&fixture);
    let id = aim(&mut app, name);
    let host = app.themes[0].host.clone();
    let bound = app.world.folder_view.as_ref().unwrap().bindings[&id].clone();
    // I without E is metadata-only: it does not even create a reader session.
    tap(&mut app, KeyCode::KeyI);
    app.poll_local_image().unwrap();
    assert!(app.world.reading && !app.local_image_pending() && !app.local_image_visible());
    assert!(app.local.as_ref().unwrap().reader.is_none());
    tap(&mut app, KeyCode::Escape);
    aim(&mut app, name);
    tap(&mut app, KeyCode::KeyE);
    drain(&mut app);
    assert!(app.world.reading && app.local_image_visible());
    pixels(&app, 1, &[19, 37, 83, 109]);
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    let current = app.world.current.clone();
    let before = app.world.document().unwrap().to_string();
    for code in [
        KeyCode::KeyW,
        KeyCode::KeyE,
        KeyCode::KeyU,
        KeyCode::KeyR,
        KeyCode::F5,
        KeyCode::KeyO,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::KeyL,
        KeyCode::Tab,
    ] {
        tap(&mut app, code);
    }
    assert!(app.world.reading && app.local_image_visible());
    assert_eq!(app.world.current, current);
    assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
    assert!(
        !app.input.forward && !app.input.captured && !app.chooser_pending && !app.chooser_active
    );
    assert!(app.address.is_none() && app.web.is_none() && app.world.reader_link.is_none());
    assert_eq!(app.themes[0].host, host);
    assert_eq!(app.world.folder_view.as_ref().unwrap().bindings[&id], bound);
    assert_eq!(app.world.document().unwrap(), before);
    app.focus_changed(false);
    for code in [KeyCode::KeyV, KeyCode::KeyC, KeyCode::Escape, KeyCode::KeyI] {
        tap(&mut app, code);
    }
    assert_eq!(app.world.document().unwrap(), before);
    assert!(app.world.reading && app.local_image_visible());
    app.focus_changed(true);
    tap(&mut app, KeyCode::KeyI);
    assert!(!app.local_image_visible());
    let metadata = app.world.document().unwrap();
    assert!(metadata.contains("METADATA VIEW"));
    assert!(metadata.contains(&folder_scene::display(bound.entry.path.as_os_str())));
    assert!(raw_bytes(metadata).is_none());
    tap(&mut app, KeyCode::KeyI);
    assert!(app.local_image_visible());
    tap(&mut app, KeyCode::KeyV);
    assert!(!app.local_image_visible());
    assert_eq!(raw_bytes(app.world.document().unwrap()).unwrap(), encoded);
    let raw = app.world.document().unwrap().to_string();
    tap(&mut app, KeyCode::ArrowRight);
    assert_eq!(
        app.world.document().unwrap(),
        raw,
        "small source EOF remains inert"
    );
    tap(&mut app, KeyCode::KeyV);
    assert!(!app.local_image_visible());
    assert!(app.world.document().unwrap().starts_with("TEXT:"));
    tap(&mut app, KeyCode::KeyV);
    assert!(app.local_image_visible());
    pixels(&app, 1, &[19, 37, 83, 109]);
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading && !app.local_image_visible());
    assert!(app.local.as_ref().unwrap().reader.is_none());
    drain(&mut app);
    assert_eq!(fs::read(fixture.0.join(name)).unwrap(), encoded);
    assert_eq!(app.themes[0].host, host);
}

#[test]
fn png_qual_app_corrupt_and_unsupported_sources_retain_exact_raw_fallback() {
    let fixture = Fixture::new();
    let good = good();
    let mut corrupt = good.clone();
    corrupt[29] ^= 1;
    let unsupported = png_bytes(1025, png::ColorType::Grayscale, &[37; 1025]);
    for (name, bytes) in [
        ("good.bin", &good),
        ("bad.png", &corrupt),
        ("wide.png", &unsupported),
    ] {
        fixture.write(name, bytes);
    }
    let mut app = app_for(&fixture);
    for (name, bytes, supported) in [
        ("good.bin", &good, true),
        ("bad.png", &corrupt, false),
        ("wide.png", &unsupported, false),
    ] {
        aim(&mut app, name);
        tap(&mut app, KeyCode::KeyE);
        drain(&mut app);
        assert_eq!(app.local_image_visible(), supported);
        if supported {
            pixels(&app, 1, &[19, 37, 83, 109]);
            tap(&mut app, KeyCode::KeyV);
        } else {
            assert!(session(&app).image_texture().is_none());
            assert!(app.world.document().unwrap().contains("PNG refused:"));
        }
        assert_eq!(raw_bytes(app.world.document().unwrap()).unwrap(), *bytes);
        assert!(app.world.reader_link.is_none() && app.web.is_none() && app.address.is_none());
        tap(&mut app, KeyCode::Escape);
        assert_eq!(fs::read(fixture.0.join(name)).unwrap(), *bytes);
    }
}

#[test]
fn png_qual_app_c_retries_held_inode_not_replacement_and_retires_prior_pixels() {
    let fixture = Fixture::new();
    let encoded = good();
    let changed = second();
    fixture.write("source", &encoded);
    let mut app = app_for(&fixture);
    aim(&mut app, "source");
    tap(&mut app, KeyCode::KeyE);
    drain(&mut app);
    pixels(&app, 1, &[19, 37, 83, 109]);
    fs::rename(fixture.0.join("source"), fixture.0.join("held")).unwrap();
    fixture.write("source", b"IMPOSTOR, NOT A PNG");
    fs::write(fixture.0.join("held"), &changed).unwrap();
    tap(&mut app, KeyCode::KeyV);
    let last_good = raw_bytes(app.world.document().unwrap()).unwrap();
    assert_eq!(last_good, encoded);
    // C explicitly accepts current stamps on the original ReaderFile, not its name.
    tap(&mut app, KeyCode::KeyC);
    assert!(!app.local_image_visible());
    assert!(session(&app).image_texture().is_none());
    assert_eq!(raw_bytes(app.world.document().unwrap()).unwrap(), changed);
    drain(&mut app);
    pixels(&app, 2, &[201, 7, 129, 255, 13, 203, 47, 255]);
    assert!(app.local_image_visible());
    tap(&mut app, KeyCode::KeyV);
    assert_eq!(raw_bytes(app.world.document().unwrap()).unwrap(), changed);
    tap(&mut app, KeyCode::Escape);
    assert_eq!(
        fs::read(fixture.0.join("source")).unwrap(),
        b"IMPOSTOR, NOT A PNG"
    );
    assert_eq!(fs::read(fixture.0.join("held")).unwrap(), changed);
}

#[test]
fn png_qual_session_retry_and_same_slot_reopen_reject_successful_prior_generation() {
    let fixture = Fixture::new();
    fixture.write("source", &good());
    let explorer = Explorer::new(&fixture.0).unwrap();
    let mut runtime = image::Runtime::default();
    for reopen in [false, true] {
        let mut reader =
            raw_reader::Session::open(explorer.open_reader(0), "actual held source".into());
        let (old_token, file) = reader.image_request().unwrap().unwrap();
        runtime.start(old_token.clone(), file).unwrap();
        // Deterministically retain a SUCCESSFUL real worker completion before
        // retirement. No fabricated Texture or timing-based stale-success claim.
        let (completed_token, completed) = worker_result(&mut runtime);
        assert!(Arc::ptr_eq(&old_token, &completed_token));
        assert_eq!(completed.as_ref().unwrap().rgba, [19, 37, 83, 109]);
        if reopen {
            drop(reader);
            reader =
                raw_reader::Session::open(explorer.open_reader(0), "same slot/name/inode".into());
        } else {
            let _ = reader.action(KeyCode::KeyC, 0);
        }
        assert!(old_token.load(Ordering::Relaxed));
        let (new_token, file) = reader.image_request().unwrap().unwrap();
        assert!(!Arc::ptr_eq(&old_token, &new_token));
        assert!(!new_token.load(Ordering::Relaxed));
        reader.image_complete(&completed_token, completed);
        assert!(
            reader.image_texture().is_none(),
            "late successful pixels must not cross generation"
        );
        runtime.start(new_token.clone(), file).unwrap();
        let (token, result) = worker_result(&mut runtime);
        assert!(Arc::ptr_eq(&token, &new_token));
        reader.image_complete(&token, result);
        assert!(reader.image_visible());
        assert_eq!(reader.image_texture().unwrap().rgba, [19, 37, 83, 109]);
    }
    assert!(!runtime.busy()); // only slot/drain assertion; not descriptor accounting
}

#[test]
fn png_qual_app_bounded_close_reopen_navigation_and_failed_staging_lifetimes() {
    let fixture = Fixture::new();
    fixture.write("source", &good());
    fs::create_dir(fixture.0.join("child")).unwrap();
    let mut app = app_for(&fixture);
    for _ in 0..3 {
        aim(&mut app, "source");
        tap(&mut app, KeyCode::KeyE);
        app.poll_local_image().unwrap(); // actually reserve/start the production job
        assert!(app.local_image_pending());
        tap(&mut app, KeyCode::Escape);
        assert!(app.local.as_ref().unwrap().reader.is_none());
        assert!(!app.local_image_visible());
        aim(&mut app, "source");
        tap(&mut app, KeyCode::KeyE);
        drain(&mut app);
        pixels(&app, 1, &[19, 37, 83, 109]);
        let before = app.world.document().unwrap().to_string();
        assert!(app.local_change(Change::Enter(usize::MAX)).is_err());
        assert!(app.world.reading && app.local_image_visible());
        assert_eq!(app.world.document().unwrap(), before);
        // Exercise successful staging while reader is live, not an invented key
        // route (reader keys intentionally block navigation).
        app.local_change(Change::Refresh).unwrap();
        assert!(!app.world.reading && !app.local_image_visible());
        assert!(app.local.as_ref().unwrap().reader.is_none());
        drain(&mut app);
        aim(&mut app, "child");
        tap(&mut app, KeyCode::KeyE);
        assert_eq!(
            app.local.as_ref().unwrap().explorer.current().folder,
            fixture.0.join("child")
        );
        assert!(app.local.as_ref().unwrap().reader.is_none());
        tap(&mut app, KeyCode::KeyR);
        assert_eq!(
            app.local.as_ref().unwrap().explorer.current().folder,
            fixture.0
        );
        assert!(app.local.as_ref().unwrap().reader.is_none());
        drain(&mut app);
    }
    assert_eq!(fs::read(fixture.0.join("source")).unwrap(), good());
    // Finite loops do not force a stale App completion ordering, measure FDs, or
    // prove hard cancellation. The Session generation test controls delivery.
}
