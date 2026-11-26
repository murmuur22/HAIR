//! Bounded safety regressions and authentic native qualification.
//! Real Explorer/ReaderFile -> sole worker -> ordinary scheduler polling.
//! On macOS the nonignored authentic_bounded_loader test REQUIRES the genuine
//! SHA-pinned HAIR_VIDEO_STILL_FIXTURE; missing/wrong input is a setup failure,
//! never a skip or qualification pass. Two additional authentic native adverses
//! remain explicitly ignored and require separate opt-in qualification runs.
//! Source assertions are supplemental, NOT native runtime proof.
use super::*;
use crate::local_model_fallback_tests::fixture;
use fixture::owned::Fixture;
use hair::assets::LocalImageFormat;
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, symlink},
    time::{Duration, Instant},
};

fn view(explorer: &Explorer) -> folder_scene::View {
    folder_scene::build(explorer.current(), 0, false)
        .expect("SETUP: real folder projection")
        .0
        .folder_view
        .expect("SETUP: folder view")
}
fn anchor(view: &folder_scene::View, name: &str) -> String {
    view.bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .expect("SETUP: named binding")
        .0
        .clone()
}
fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("SETUP: PNG header");
        writer
            .write_image_data(&[19, 37, 83, 255].repeat((width * height) as usize))
            .expect("SETUP: PNG pixels");
        writer.finish().expect("SETUP: PNG finish");
    }
    bytes
}
fn finished(worker: &image::Runtime) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !worker.test_finished() {
        assert!(
            Instant::now() < deadline,
            "PROGRESS: real worker not finished; not a semantic refusal assertion"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn drain(runtime: &mut Runtime, explorer: &Explorer, view: &folder_scene::View) {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        runtime.poll(Some(explorer), Some(view), false);
        assert!(runtime.cached_items() <= ITEM_CAP);
        assert!(runtime.cached_bytes() <= CACHE_BYTES);
        if runtime.worker.busy() {
            // Native occupation can outlive active/Rust join. Do NOT require an
            // active job here or invent a refund when its JoinHandle disappears.
            assert!(runtime.cached_bytes() + WORK_RESERVATION <= TOTAL_BYTES);
        }
        if runtime.test_model_fallback_progress().1 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "PROGRESS: scheduler/native reclamation not drained; status={}",
            runtime.status
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn stable_no_retry(runtime: &mut Runtime, explorer: &Explorer, view: &folder_scene::View) {
    assert!(
        runtime.test_model_fallback_progress().1,
        "PROGRESS: must observe drained state before testing no-retry"
    );
    let attempts = runtime.acquisition_attempts;
    let status = runtime.status.clone();
    let items = runtime.cached_items();
    for _ in 0..64 {
        runtime.poll(Some(explorer), Some(view), false);
        assert_eq!(runtime.test_model_fallback_progress(), (attempts, true));
        assert_eq!(runtime.status, status);
        assert_eq!(runtime.cached_items(), items);
    }
}
fn raw_and_source(explorer: &Explorer, view: &folder_scene::View, name: &str, bytes: &[u8]) {
    let bound = &view.bindings[&anchor(view, name)];
    let chunk = explorer
        .open_reader(bound.index)
        .expect("raw acquisition remains available")
        .read_chunk(0)
        .expect("raw read remains available");
    assert_eq!(
        chunk.body(),
        &bytes[..bytes.len().min(hair::local_explorer::READER_CHUNK_BYTES)]
    );
    assert_eq!(chunk.observed_len, bytes.len() as u64);
    let metadata = folder_scene::metadata(&bound.entry);
    assert!(metadata.contains(&folder_scene::display(&bound.entry.name)));
    assert!(metadata.contains(&folder_scene::display(bound.entry.path.as_os_str())));
    assert!(metadata.contains("no playback"));
    assert!(metadata.contains("no arbitrary-file/external-reference containment"));
    assert!(metadata.contains("callback-drain guarantee"));
}
fn actual_charge(runtime: &Runtime) -> usize {
    runtime.cache.values().map(|t| t.rgba.len()).sum::<usize>()
        + runtime
            .models
            .values()
            .map(|m| {
                m.vertices.capacity() * std::mem::size_of::<hair::assets::Vertex>()
                    + std::mem::size_of::<hair::assets::Model>()
            })
            .sum::<usize>()
}

#[test]
fn empty_and_over_one_mib_video_refuse_without_retry_and_keep_raw_source() {
    for oversized in [false, true] {
        let f = Fixture::new();
        let name = "Held Case\né.MP4";
        f.file(name, b"");
        if oversized {
            let file = fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(f.0.join(name))
                .expect("SETUP: owned regular file");
            file.set_len((MIB + 1) as u64)
                .expect("SETUP: sparse length");
        }
        let explorer = Explorer::new(&f.0).expect("SETUP: Explorer");
        let projected = view(&explorer);
        let index = projected.bindings[&anchor(&projected, name)].index;
        let held = explorer
            .open_reader(index)
            .expect("SETUP: regular held reader");
        let error = held.read_video_still_bytes(&image::token()).unwrap_err();
        assert!(error.contains("empty or exceeds 1 MiB"), "{error}");
        let mut runtime = Runtime::default();
        runtime.room(&projected);
        assert_eq!(
            runtime.queue.len(),
            1,
            "uppercase literal suffix is an intent"
        );
        assert_eq!(runtime.acquisition_attempts, 0);
        drain(&mut runtime, &explorer, &projected);
        assert!(runtime.cache.is_empty() && runtime.models.is_empty());
        assert_eq!(runtime.acquisition_attempts, 1);
        assert!(runtime.status.contains("empty or exceeds 1 MiB"));
        let bytes = fs::read(f.0.join(name)).unwrap();
        raw_and_source(&explorer, &projected, name, &bytes);
        // A repair to ordinary bytes is not permission for an automatic retry.
        fs::write(
            f.0.join(name),
            b"changed owned source, still not an encoded video",
        )
        .unwrap();
        stable_no_retry(&mut runtime, &explorer, &projected);
        assert_eq!(runtime.acquisition_attempts, 1);
        drop(runtime);
    }
}

#[test]
fn raced_nonregular_video_sources_and_changed_held_stamp_are_refused() {
    for replacement in ["symlink", "directory"] {
        let f = Fixture::new();
        f.file("a.MP4", b"original held bytes");
        f.file("decoy.bin", b"DECOY MUST NOT BE READ AS VIDEO");
        let explorer = Explorer::new(&f.0).expect("SETUP: Explorer");
        let projected = view(&explorer);
        let index = projected.bindings[&anchor(&projected, "a.MP4")].index;
        fs::rename(f.0.join("a.MP4"), f.0.join("original.bin")).unwrap();
        if replacement == "symlink" {
            symlink("decoy.bin", f.0.join("a.MP4")).expect("SETUP: raced symlink");
        } else {
            fs::create_dir(f.0.join("a.MP4")).expect("SETUP: raced directory");
        }
        assert!(
            explorer.open_reader(index).is_err(),
            "{replacement} must not authorize ReaderFile"
        );
        let mut runtime = Runtime::default();
        runtime.room(&projected);
        drain(&mut runtime, &explorer, &projected);
        assert_eq!(runtime.acquisition_attempts, 1);
        assert!(runtime.cache.is_empty() && runtime.models.is_empty());
        assert!(runtime.status.contains("object source refused"));
        if replacement == "symlink" {
            fs::remove_file(f.0.join("a.MP4")).unwrap();
        } else {
            fs::remove_dir(f.0.join("a.MP4")).unwrap();
        }
        fs::rename(f.0.join("original.bin"), f.0.join("a.MP4")).unwrap();
        stable_no_retry(&mut runtime, &explorer, &projected);
        raw_and_source(&explorer, &projected, "a.MP4", b"original held bytes");
        assert_eq!(
            fs::read(f.0.join("decoy.bin")).unwrap(),
            b"DECOY MUST NOT BE READ AS VIDEO"
        );
        let held = explorer.open_reader(index).unwrap();
        fs::write(
            f.0.join("a.MP4"),
            b"different length invalidates baseline deterministically",
        )
        .unwrap();
        let error = held.read_video_still_bytes(&image::token()).unwrap_err();
        assert!(error.contains("source changed"), "{error}");
        assert!(
            held.read_chunk(0)
                .unwrap_err()
                .contains("changed since baseline")
        );
        // Fresh regular-file acquisition works; this is stamp refusal, not a
        // claim about racing native parsing or immutable filesystem snapshots.
        assert_eq!(
            explorer
                .open_reader(index)
                .unwrap()
                .read_video_still_bytes(&image::token())
                .unwrap(),
            b"different length invalidates baseline deterministically"
        );
        drop(runtime);
    }
}

#[test]
fn completed_video_error_cannot_replace_reused_slot_status_or_pixels() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("old")).unwrap();
    fs::create_dir(f.0.join("new")).unwrap();
    f.file("old/a.mp4", b""); // Real source-bound error, before native allocation.
    f.file("new/b.png", &png_bytes(1, 1));
    let old_explorer = Explorer::new(&f.0.join("old")).unwrap();
    let next_explorer = Explorer::new(&f.0.join("new")).unwrap();
    let old = view(&old_explorer);
    let next = view(&next_explorer);
    assert_eq!(anchor(&old, "a.mp4"), anchor(&next, "b.png"));
    // Positive control: unchanged source identity really publishes its error.
    let mut control = Runtime::default();
    control.room(&old);
    drain(&mut control, &old_explorer, &old);
    assert!(control.status.contains("empty or exceeds 1 MiB"));
    drop(control);
    for new_generation in [false, true] {
        let mut runtime = Runtime::default();
        runtime.room(&old);
        runtime.poll(Some(&old_explorer), Some(&old), false);
        finished(&runtime.worker); // Real completed error owned by JoinHandle.
        if new_generation {
            runtime.room(&next); // Normal room invalidation route, no result injection.
        }
        let before = runtime.status.clone();
        runtime.poll(Some(&next_explorer), Some(&next), true);
        assert_eq!(
            runtime.status, before,
            "stale error must not change rebound status"
        );
        assert!(runtime.cache.is_empty() && runtime.models.is_empty());
        assert!(!runtime.worker.busy() && runtime.active.is_none());
        assert_eq!(runtime.acquisition_attempts, 1);
        if new_generation {
            drain(&mut runtime, &next_explorer, &next);
            assert_eq!(
                runtime.cache[&anchor(&next, "b.png")].rgba,
                [19, 37, 83, 255]
            );
            assert_eq!(runtime.acquisition_attempts, 2);
        }
        drop(runtime);
    }
}

#[test]
fn completed_video_job_blocks_all_media_start_routes_until_real_join() {
    let f = Fixture::new();
    f.file("a.mp4", b"");
    f.file("b.png", &png_bytes(1, 1));
    f.file("c.txt", b"LITERAL TEXT");
    f.file("d.glb", &fixture::control());
    let explorer = Explorer::new(&f.0).unwrap();
    let projected = view(&explorer);
    let held = |name| {
        explorer
            .open_reader(projected.bindings[&anchor(&projected, name)].index)
            .unwrap()
    };
    let mut worker = image::Runtime::default();
    worker.start_video(image::token(), held("a.mp4")).unwrap();
    finished(&worker);
    assert!(
        worker.busy(),
        "finished Rust job must remain reserved until joined"
    );
    for result in [
        worker.start_video(image::token(), held("a.mp4")),
        worker.start_object(image::token(), held("b.png"), LocalImageFormat::Png),
        worker.start_text(image::token(), held("c.txt")),
        worker.start_model(image::token(), held("d.glb")),
    ] {
        assert!(result.unwrap_err().contains("occupied"));
    }
    let (_, result) = worker.poll_object().expect("completed real job is joined");
    match result {
        Ok(_) => panic!("empty video source must refuse"),
        Err(error) => assert!(error.contains("empty or exceeds 1 MiB")),
    }
    assert!(!worker.busy());
    worker.start_text(image::token(), held("c.txt")).unwrap();
    finished(&worker);
    match worker.poll_object().unwrap().1.unwrap() {
        image::Output::Image(texture) => assert!(!texture.rgba.is_empty()),
        image::Output::Model(_) => panic!("text returned model"),
    }
    assert!(!worker.busy());
    // This proves Rust-job reservation only; it does NOT inject or prove an
    // occupied native post-join slot. That deterministic case remains missing.
}

#[test]
fn mixed_real_cache_charge_refuses_video_before_acquisition_without_new_allowance() {
    let f = Fixture::new();
    f.file("a.glb", &fixture::control());
    f.file("b.txt", b"literal shared ledger");
    f.file("c.png", &png_bytes(1024, 1024));
    f.file("d.MP4", b""); // Intent only; never used as a successful encoded fixture.
    let explorer = Explorer::new(&f.0).unwrap();
    let projected = view(&explorer);
    let names: Vec<_> = projected
        .bindings
        .values()
        .map(|b| b.entry.name.clone())
        .collect();
    assert_eq!(
        names,
        ["a.glb", "b.txt", "c.png", "d.MP4"].map(std::ffi::OsString::from),
        "SETUP: declared deterministic intent order"
    );
    let mut runtime = Runtime::default();
    runtime.room(&projected);
    assert_eq!(runtime.queue.len(), 4);
    assert_eq!(runtime.acquisition_attempts, 0);
    drain(&mut runtime, &explorer, &projected);
    assert_eq!((runtime.models.len(), runtime.cache.len()), (1, 2));
    assert_eq!(runtime.cached_bytes(), actual_charge(&runtime));
    assert!(runtime.cached_bytes() > 4 * MIB && runtime.cached_bytes() <= CACHE_BYTES);
    assert_eq!(runtime.acquisition_attempts, 3);
    assert!(
        runtime
            .acquisition_sources
            .iter()
            .all(|e| e.name != "d.MP4"),
        "video may not bypass existing mixed cache/work admission"
    );
    assert!(runtime.status.contains("budget exhausted"));
    assert!(!runtime.cache.contains_key(&anchor(&projected, "d.MP4")));
    assert_eq!(runtime.worker.text_reads.lock().unwrap().calls, 1);
    raw_and_source(&explorer, &projected, "d.MP4", b"");
    stable_no_retry(&mut runtime, &explorer, &projected);
    drop(runtime);
}

#[test]
fn native_post_join_fail_closed_wiring_is_source_only_not_deterministic_runtime_proof() {
    let worker = include_str!("image.rs");
    let scheduler = include_str!("object_preview.rs");
    let slot = include_str!("video_still.rs");
    let native = include_str!("../../native/video_still.m");
    assert!(worker.contains("self.job.is_none()") && worker.contains("slot.occupied()"));
    assert!(worker.contains("self.job.is_some() || self.native_retirement_pending()"));
    for (start, end) in [
        ("pub(super) fn start_video(", "pub(super) fn start("),
        ("fn start_with(", "pub(super) fn start_model("),
        ("pub(super) fn start_model(", "pub(super) fn start_text("),
        ("pub(super) fn start_text(", "pub(super) fn poll("),
    ] {
        let body = worker
            .split_once(start)
            .expect("source start")
            .1
            .split_once(end)
            .expect("source end")
            .0;
        let gate = body.find("if self.busy()").expect("shared admission gate");
        let spawn = body
            .find("std::thread::Builder::new()")
            .expect("real worker spawn");
        assert!(
            gate < spawn,
            "all media must consult native occupation before spawn"
        );
    }
    let observed = scheduler
        .split_once("self.native_pending = self.worker.native_retirement_pending();")
        .expect("actual native occupation observation")
        .1;
    let (gate, after_gate) = observed.split_once('{').expect("retirement guard block");
    assert_eq!(
        gate.trim(),
        "if self.native_pending && !self.queue.is_empty()",
        "no intervening reassignment between observation and guard"
    );
    let refusal = after_gate
        .split_once("// One acquisition per poll")
        .expect("retirement refusal precedes new acquisition")
        .0;
    assert!(refusal.contains("self.queue.clear()") && refusal.contains("no retry"));
    assert!(
        refusal.contains("self.refuse_queued("),
        "queued diagnostics refused without changing occupation"
    );
    assert!(scheduler.contains("&& !self.worker.busy()"));
    assert!(slot.contains("ABANDONED_RESERVATION.store(true, Ordering::Release)"));
    let available = native
        .split_once("static BOOL Available(")
        .unwrap()
        .1
        .split_once("static BOOL LoadKeys(")
        .unwrap()
        .0;
    for predicate in [
        "!slot.context",
        "!slot.loader",
        "!slot.asset",
        "!slot.generator",
        "!l.contexts",
        "!l.loaders",
        "!l.sources",
        "!l.sourceBytes",
        "!l.chunks",
        "!l.chunkBytes",
    ] {
        assert!(
            available.contains(predicate),
            "native availability must include {predicate}"
        );
    }
    assert!(native.contains("generateCGImageAsynchronouslyForTime:kCMTimeZero"));
    for forbidden in [
        "AVPlayer",
        "AVAudioPlayer",
        "AVAudioEngine",
        "fileURLWithPath:",
    ] {
        assert!(
            !native.contains(forbidden),
            "still adapter must not construct {forbidden}"
        );
    }
    for (name, expected_kind) in [
        (
            "a.png",
            "PNG suffix (lazy bounded preview or fallback; E checks source)",
        ),
        (
            "a.jpg",
            "JPEG suffix (lazy bounded preview or fallback; E checks source)",
        ),
        (
            "a.txt",
            "text suffix (literal prefix or fallback; E reads bytes)",
        ),
        (
            "a.glb",
            "GLB suffix (bounded static mesh or fallback; E reads bytes)",
        ),
        ("a.mov", "media (playback unsupported)"),
        ("a.mkv", "MKV video (playback unavailable)"),
        ("a.pdf", "PDF (preview unsupported)"),
    ] {
        let entry = Entry {
            name: name.into(),
            path: Path::new("literal").join(name),
            kind: Kind::File,
        };
        assert_eq!(folder_scene::kind(&entry), expected_kind);
        assert_eq!(
            folder_scene::metadata(&entry),
            format!(
                "NAME (escaped OS bytes):\n{}\nSOURCE PATH (display only; may become stale):\n{}\nTYPE: {}\nNo writes, execution, default-open or link following.\n",
                folder_scene::display(&entry.name),
                folder_scene::display(entry.path.as_os_str()),
                expected_kind
            )
        );
    }
    // The requested held-native-state post-join admission case has no clean
    // Rust injection boundary. These exact source assertions supplement, never
    // replace, the separate native-proof harness and integrated review.
}

#[cfg(target_os = "macos")]
fn authentic_input() -> Vec<u8> {
    use sha2::{Digest, Sha256};
    use std::{io::Read, os::unix::fs::MetadataExt, path::PathBuf};
    let path = PathBuf::from(std::env::var_os("HAIR_VIDEO_STILL_FIXTURE").expect(
        "SETUP: genuine MP4 required for native qualification (including nonignored loader test)",
    ));
    assert!(path.is_absolute(), "SETUP: absolute fixture path");
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(&path)
        .expect("SETUP: provision original authentic fixture");
    let before = file.metadata().unwrap();
    assert!(
        before.is_file() && before.len() > 8 && before.len() < 4096,
        "SETUP: original small regular fixture"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(before.len() + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    let after = file.metadata().unwrap();
    assert_eq!(
        (
            before.dev(),
            before.ino(),
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec()
        ),
        (
            after.dev(),
            after.ino(),
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec()
        ),
        "SETUP: fixture changed during acquisition"
    );
    assert_eq!(
        bytes.len() as u64,
        before.len(),
        "SETUP: fixture incomplete"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "9638f62dd10ef140610a04dd00b132c943e444b407b10307e823824ad78b43a6",
        "SETUP: authentic fixture identity mismatch"
    );
    bytes
}

#[cfg(target_os = "macos")]
#[test]
fn authentic_bounded_loader_reaches_first_frame_without_request_cap_refusal() {
    let bytes = authentic_input();
    let mut slot = crate::folder_native::video_still::Slot::default();
    let texture = slot.decode(&bytes).expect(
        "genuine tiny MP4 must decode within unchanged held-loader request/delivery limits",
    );
    assert_eq!([texture.width, texture.height], [160, 96]);
    let (pixels, rest) = texture.rgba.as_chunks::<4>();
    assert!(rest.is_empty() && pixels.iter().all(|p| p[3] == 255));
    for (x, y, rgb) in [
        (24, 18, [220_u8, 40, 40]),
        (100, 18, [40, 200, 60]),
        (24, 70, [40, 70, 220]),
        (100, 70, [230, 200, 40]),
    ] {
        assert!(
            pixels[y * 160 + x][..3]
                .iter()
                .zip(rgb)
                .all(|(a, b)| a.abs_diff(b) <= 28)
        );
    }
    assert!(
        !slot.occupied(),
        "normal decode must observe tracked reclamation"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit authentic MP4 fixture; native no-device qualification"]
fn authentic_truncated_and_corrupt_container_refuse_without_retry_keep_raw_source() {
    let original = authentic_input();
    let truncated = original[..8].to_vec(); // Real truncation, not an encoded fake.
    let mut corrupt = original.clone();
    corrupt[4] ^= 0xff; // Deliberately break ftyp; NOT alternate-codec semantics.
    for bytes in [truncated, corrupt] {
        let f = Fixture::new();
        let name = "Corrupt Case\né.MP4";
        f.file(name, &bytes);
        let explorer = Explorer::new(&f.0).unwrap();
        let projected = view(&explorer);
        let mut runtime = Runtime::default();
        runtime.room(&projected);
        drain(&mut runtime, &explorer, &projected);
        assert_eq!(runtime.acquisition_attempts, 1);
        assert!(runtime.cache.is_empty() && runtime.models.is_empty());
        assert!(
            runtime.status.contains("not a bounded MP4 ftyp source"),
            "{}",
            runtime.status
        );
        raw_and_source(&explorer, &projected, name, &bytes);
        assert_eq!(fs::read(f.0.join(name)).unwrap(), bytes);
        // Restore exact genuine bytes. No implicit retry until an explicit room requeue.
        fs::write(f.0.join(name), &original).unwrap();
        stable_no_retry(&mut runtime, &explorer, &projected);
        drop(runtime);
    }
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit authentic MP4 fixture; native no-device qualification"]
fn authentic_mixed_media_share_item_cap_and_stale_video_success_is_not_published() {
    let bytes = authentic_input();
    let f = Fixture::new();
    fs::create_dir(f.0.join("old")).unwrap();
    fs::create_dir(f.0.join("replacement")).unwrap();
    fs::create_dir(f.0.join("mixed")).unwrap();
    f.file("old/Original\né.MP4", &bytes);
    f.file("replacement/Replacement.png", &png_bytes(1, 1));
    for i in 0..10 {
        match i % 4 {
            0 => f.file(&format!("mixed/f{i:02}.MP4"), &bytes),
            1 => f.file(&format!("mixed/f{i:02}.png"), &png_bytes(1, 1)),
            2 => f.file(&format!("mixed/f{i:02}.txt"), b"LITERAL MIXED SOURCE"),
            _ => f.file(&format!("mixed/f{i:02}.glb"), &fixture::control()),
        }
    }
    let old_explorer = Explorer::new(&f.0.join("old")).unwrap();
    let old = view(&old_explorer);
    let replacement_explorer = Explorer::new(&f.0.join("replacement")).unwrap();
    let replacement = view(&replacement_explorer);
    assert_eq!(
        anchor(&old, "Original\né.MP4"),
        anchor(&replacement, "Replacement.png")
    );
    let mut runtime = Runtime::default();
    runtime.room(&old);
    runtime.poll(Some(&old_explorer), Some(&old), false);
    finished(&runtime.worker); // Native call finished, still unjoined and unpublished.
    assert_eq!(
        runtime
            .worker
            .video_successes
            .load(std::sync::atomic::Ordering::Acquire),
        1,
        "PROGRESS/SEMANTIC: stale result must be an actual successful native video result"
    );
    assert!(runtime.worker.busy() && runtime.cache.is_empty());
    runtime.room(&replacement);
    let status = runtime.status.clone();
    runtime.poll(Some(&replacement_explorer), Some(&replacement), true);
    assert!(runtime.cache.is_empty() && runtime.models.is_empty());
    assert!(
        !runtime.worker.native_retirement_pending(),
        "PROGRESS: normal authentic job joined but tracked native reclamation remains unobserved; not a stale-publication failure"
    );
    assert_eq!(
        runtime.status, status,
        "stale native result must not change new status"
    );
    drain(&mut runtime, &replacement_explorer, &replacement);
    assert_eq!(
        runtime.cache[&anchor(&replacement, "Replacement.png")].rgba,
        [19, 37, 83, 255]
    );
    assert_eq!(runtime.acquisition_attempts, 2);
    let mixed_explorer = Explorer::new(&f.0.join("mixed")).unwrap();
    let mixed = view(&mixed_explorer);
    assert_eq!(
        mixed.bindings.len(),
        10,
        "SETUP: ten mixed intents in one room"
    );
    let ordered: Vec<_> = mixed
        .bindings
        .values()
        .map(|b| b.entry.name.clone())
        .collect();
    assert_eq!(
        ordered,
        (0..10)
            .map(|i| {
                let suffix = ["MP4", "png", "txt", "glb"][i % 4];
                std::ffi::OsString::from(format!("f{i:02}.{suffix}"))
            })
            .collect::<Vec<_>>(),
        "SETUP: actual binding order must match the item-cap oracle"
    );
    runtime.room(&mixed);
    let before = runtime.acquisition_attempts;
    drain(&mut runtime, &mixed_explorer, &mixed);
    assert_eq!(runtime.acquisition_attempts - before, 8);
    assert_eq!(
        (
            runtime.cached_items(),
            runtime.models.len(),
            runtime.cache.len()
        ),
        (8, 2, 6)
    );
    assert_eq!(runtime.cached_bytes(), actual_charge(&runtime));
    assert!(runtime.status.contains("budget exhausted"));
    for i in [0, 4] {
        let texture = &runtime.cache[&anchor(&mixed, &format!("f{i:02}.MP4"))];
        assert_eq!([texture.width, texture.height], [160, 96]);
        let (pixels, rest) = texture.rgba.as_chunks::<4>();
        assert!(rest.is_empty() && pixels.iter().all(|p| p[3] == 255));
        // Source landmark excludes a blank or substituted result. Full four-color
        // fit/orientation oracle stays in the unchanged original App tracer.
        assert!(
            pixels[18 * 160 + 24][..3]
                .iter()
                .zip([220_u8, 40, 40])
                .all(|(a, b)| a.abs_diff(b) <= 28)
        );
    }
    assert!(
        !runtime.cache.contains_key(&anchor(&mixed, "f08.MP4")),
        "ninth intent must not receive a second video allowance"
    );
    assert_eq!(runtime.worker.text_reads.lock().unwrap().calls, 2);
    stable_no_retry(&mut runtime, &mixed_explorer, &mixed);
    assert!(
        !runtime.worker.native_retirement_pending(),
        "observed normal-job reclamation only, not universal native drain"
    );
    assert_eq!(fs::read(f.0.join("old/Original\né.MP4")).unwrap(), bytes);
    drop(runtime);
}
