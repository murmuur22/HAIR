//! File-only prepared regressions: actual Explorer -> scheduler -> held text worker.
//! Observations are local call-site accounting, not a filesystem/OS-wide audit.
use super::*;
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, symlink},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR"))
            .canonicalize()
            .unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(base.starts_with(source.parent().unwrap()));
        assert!(!base.starts_with(&source) && !source.starts_with(&base));
        let path = base.join(format!(
            "s{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("exclusive short fixture directory");
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(self.0.join(name))
            .unwrap();
        file.write_all(bytes).unwrap();
        file.flush().unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn view(explorer: &Explorer, page: usize) -> folder_scene::View {
    folder_scene::build(explorer.current(), page, false)
        .unwrap()
        .0
        .folder_view
        .unwrap()
}
fn anchor(view: &folder_scene::View, name: &str) -> String {
    view.bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .unwrap()
        .0
        .clone()
}
fn drain(runtime: &mut Runtime, explorer: &Explorer, view: &folder_scene::View) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.poll(Some(explorer), Some(view), false);
        if runtime.queue.is_empty() && runtime.active.is_none() && !runtime.worker.busy() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "scheduler did not drain: {}",
            runtime.status
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn finished(worker: &image::Runtime) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.test_finished() {
        assert!(
            Instant::now() < deadline,
            "owned text worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn same_texture(actual: &Texture, escaped: &str, truncated: bool) {
    // Oracle for acquisition/escaping/dispatch, NOT an independent font or GPU oracle.
    let expected = hair::text::literal_excerpt_texture(escaped, truncated);
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height)
    );
    assert!(
        actual.rgba == expected.rgba,
        "actual worker texture differs from the explicit literal projection"
    );
}

#[test]
fn sparse_large_source_reads_one_4096_byte_non_eof_prefix() {
    let fixture = Fixture::new();
    let mut prefix = vec![b' '; 4096];
    prefix[..7].copy_from_slice(b"PREFIX\n");
    // A split scalar at the boundary must not trigger lookahead.
    prefix[4094..].copy_from_slice(&[0xe2, 0x82]);
    fixture.file("p.txt", &prefix);
    let large_len = 1024 * 1024 * 1024 + 17;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(fixture.0.join("p.txt"))
        .unwrap();
    file.set_len(large_len).unwrap(); // Owned sparse regular file, not a length-sized allocation.
    file.seek(SeekFrom::Start(4096)).unwrap();
    file.write_all(b"AC\nAFTER_PREFIX_MUST_NOT_BE_READ")
        .unwrap();
    file.flush().unwrap();
    drop(file);
    let explorer = Explorer::new(&fixture.0).unwrap();
    let view = view(&explorer, 0);
    let mut runtime = Runtime::default();
    runtime.room(&view);
    assert_eq!(runtime.acquisition_attempts, 0, "room only queues intents");
    drain(&mut runtime, &explorer, &view);
    let audit = runtime.worker.text_reads.lock().unwrap();
    assert_eq!(audit.calls, 1);
    assert_eq!(audit.chunks, vec![(0, 4096, large_len, false)]);
    drop(audit);
    assert_eq!(runtime.acquisition_attempts, 1);
    let mut escaped = String::from("PREFIX\n");
    escaped.push_str(&" ".repeat(4087));
    escaped.push_str("\\xE2\\x82");
    same_texture(&runtime.cache[&anchor(&view, "p.txt")], &escaped, true);
    // Independently test the non-overflow footer choice in the real rasterizer.
    let short = hair::text::literal_excerpt_texture("PREFIX", false);
    let more = hair::text::literal_excerpt_texture("PREFIX", true);
    let footer = 218 * short.width as usize * 4;
    assert_eq!(&short.rgba[..footer], &more.rgba[..footer]);
    assert_ne!(&short.rgba[footer..], &more.rgba[footer..]);
    let actual = &runtime.cache[&anchor(&view, "p.txt")];
    assert_eq!(&actual.rgba[footer..], &more.rgba[footer..]);
    assert_eq!(
        fs::metadata(fixture.0.join("p.txt")).unwrap().len(),
        large_len
    );
}

#[test]
fn actual_text_worker_preserves_escaped_bytes_and_inactive_markup() {
    let fixture = Fixture::new();
    let bytes = b"\0\x1b\t\r\x7f\\\n\xff\xe2\x82\n\xe2\x80\xae\n<b>x</b>\n[x](file:target)\n![x](target)\n<script>x</script>\n";
    let escaped = "\\x00\\x1B\\x09\\x0D\\x7F\\\\\n\\xFF\\xE2\\x82\n\\u{202E}\n<b>x</b>\n[x](file:target)\n![x](target)\n<script>x</script>\n";
    fixture.file("m.md", bytes);
    let explorer = Explorer::new(&fixture.0).unwrap();
    let view = view(&explorer, 0);
    let mut runtime = Runtime::default();
    runtime.room(&view);
    drain(&mut runtime, &explorer, &view);
    let mut literal = String::new();
    super::super::raw_reader::escaped_bytes(&mut literal, bytes);
    assert_eq!(
        literal, escaped,
        "exact escapes; no replacement, controls or bidi authority"
    );
    same_texture(&runtime.cache[&anchor(&view, "m.md")], escaped, false);
    assert_eq!(runtime.acquisition_attempts, 1);
    assert_eq!(runtime.worker.text_reads.lock().unwrap().calls, 1);
    assert_eq!(fs::read(fixture.0.join("m.md")).unwrap(), bytes);
}

#[test]
fn empty_blank_and_raced_symlink_refusals_do_not_retry_on_ordinary_polls() {
    let fixture = Fixture::new();
    fixture.file("e.txt", b"");
    fixture.file("b.md", b" \n");
    fixture.file("r.markdown", b"original regular source");
    fixture.file("d.bin", b"DECOY MUST NOT BECOME A TEXTURE");
    let explorer = Explorer::new(&fixture.0).unwrap();
    let view = view(&explorer, 0);
    // Snapshot says regular file; actual held-directory open must still refuse the link.
    fs::rename(fixture.0.join("r.markdown"), fixture.0.join("held.bin")).unwrap();
    symlink("d.bin", fixture.0.join("r.markdown")).unwrap();
    let mut runtime = Runtime::default();
    runtime.room(&view);
    drain(&mut runtime, &explorer, &view);
    assert!(
        runtime.cache.is_empty(),
        "all three refused sources retain pack fallback"
    );
    assert_eq!(runtime.acquisition_attempts, 3);
    {
        let audit = runtime.worker.text_reads.lock().unwrap();
        assert_eq!(
            audit.calls, 2,
            "raced symlink must fail before the text worker"
        );
        assert_eq!(audit.chunks.len(), 2);
        assert!(
            audit
                .chunks
                .iter()
                .all(|&(at, used, len, eof)| at == 0 && used as u64 == len && eof)
        );
    }
    // Restore usable bytes without a refresh: ordinary polling must not requeue refusal.
    fs::remove_file(fixture.0.join("r.markdown")).unwrap();
    fs::rename(fixture.0.join("held.bin"), fixture.0.join("r.markdown")).unwrap();
    fs::write(fixture.0.join("e.txt"), b"now nonempty").unwrap();
    fs::write(fixture.0.join("b.md"), b"now nonblank").unwrap();
    for _ in 0..64 {
        runtime.poll(Some(&explorer), Some(&view), false);
    }
    assert!(runtime.cache.is_empty());
    assert_eq!(runtime.acquisition_attempts, 3);
    assert_eq!(runtime.worker.text_reads.lock().unwrap().calls, 2);
    runtime.room(&view); // Explicit room requeue is allowed, unlike an ordinary poll.
    drain(&mut runtime, &explorer, &view);
    assert_eq!(runtime.cache.len(), 3);
    assert_eq!(runtime.acquisition_attempts, 6);
    assert_eq!(runtime.worker.text_reads.lock().unwrap().calls, 5);
    assert_eq!(
        fs::read(fixture.0.join("d.bin")).unwrap(),
        b"DECOY MUST NOT BECOME A TEXTURE"
    );
}

#[test]
fn room_bounds_exclude_nested_offpage_reads_and_completed_stale_generation() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("sub")).unwrap();
    fixture.file("sub/hidden.txt", b"NESTED");
    for i in 0..15 {
        fixture.file(&format!("f{i:02}.txt"), format!("SOURCE {i}\n").as_bytes());
    }
    let explorer = Explorer::new(&fixture.0).unwrap();
    let first = view(&explorer, 0);
    let next = view(&explorer, 1);
    assert_eq!(first.bindings.len(), folder_scene::PAGE_SIZE);
    assert!(!next.bindings.is_empty());
    let mut runtime = Runtime::default();
    runtime.room(&first);
    assert!(runtime.queue.len() <= folder_scene::PAGE_SIZE);
    for _ in 0..8 {
        runtime.poll(Some(&explorer), Some(&first), true);
    }
    assert_eq!(
        runtime.acquisition_attempts, 0,
        "paused scheduler cannot acquire"
    );
    runtime.poll(Some(&explorer), Some(&first), false);
    finished(&runtime.worker); // Real successful result held in its JoinHandle, not injected.
    let old_attempt = runtime.active.as_ref().unwrap().attempt.clone();
    let old_generation = runtime.generation.as_ref().unwrap().clone();
    assert!(runtime.cache.is_empty());
    runtime.room(&next);
    assert!(old_attempt.load(Ordering::Relaxed) && old_generation.load(Ordering::Relaxed));
    assert!(
        runtime.worker.busy(),
        "cancel does not release the owned worker before join"
    );
    runtime.poll(Some(&explorer), Some(&next), true); // Drain stale success without new work.
    assert!(!runtime.worker.busy() && runtime.active.is_none());
    assert!(
        runtime.cache.is_empty(),
        "completed old generation must not publish"
    );
    assert_eq!(runtime.acquisition_attempts, 1);
    drain(&mut runtime, &explorer, &next);
    assert_eq!(
        runtime.cache.len(),
        next.bindings
            .values()
            .filter(|b| text_eligible(&b.entry))
            .count()
    );
    assert!(
        runtime
            .acquisition_sources
            .iter()
            .skip(1)
            .all(|e| next.bindings.values().any(|b| b.entry == *e))
    );
    // A fresh page-zero scheduler must stop at the shared item/cache admission bound.
    let mut bounded = Runtime::default();
    bounded.room(&first);
    drain(&mut bounded, &explorer, &first);
    assert_eq!(bounded.cache.len(), ITEM_CAP);
    assert_eq!(bounded.acquisition_attempts, ITEM_CAP);
    assert_eq!(bounded.worker.text_reads.lock().unwrap().calls, ITEM_CAP);
    assert_eq!(bounded.acquisition_sources.len(), ITEM_CAP);
    assert!(
        bounded
            .acquisition_sources
            .iter()
            .all(|e| first.bindings.values().any(|b| b.entry == *e))
    );
    assert!(
        bounded
            .acquisition_sources
            .iter()
            .all(|e| e.path.parent() == Some(fixture.0.as_path()))
    );
    assert!(
        bounded
            .acquisition_sources
            .iter()
            .all(|e| e.name != "hidden.txt")
    );
    for _ in 0..64 {
        bounded.poll(Some(&explorer), Some(&first), false);
    }
    assert_eq!(bounded.acquisition_attempts, ITEM_CAP);
    assert!(bounded.cache.values().map(|t| t.rgba.len()).sum::<usize>() <= CACHE_BYTES);
}

#[test]
fn held_source_mutation_is_refused_by_actual_text_adapter_before_raster() {
    let fixture = Fixture::new();
    fixture.file("h.txt", b"original");
    let explorer = Explorer::new(&fixture.0).unwrap();
    let index = explorer
        .current()
        .entries
        .iter()
        .position(|e| e.name == "h.txt")
        .unwrap();
    let held = explorer.open_reader(index).unwrap();
    // A different length deterministically invalidates the real baseline, no timing assumption.
    fs::write(fixture.0.join("h.txt"), b"mutated length and content").unwrap();
    let mut worker = image::Runtime::default();
    worker.start_text(image::token(), held).unwrap();
    finished(&worker);
    let (_, result) = worker.poll().unwrap();
    let error = match result {
        Ok(_) => panic!("changed held baseline must not rasterize"),
        Err(error) => error,
    };
    assert!(error.contains("changed since baseline"), "{error}");
    assert!(!worker.busy());
    {
        let audit = worker.text_reads.lock().unwrap();
        assert_eq!(audit.calls, 1);
        assert!(
            audit.chunks.is_empty(),
            "changed source has no successful acquired chunk"
        );
    }
    // Real fresh acquisition is a positive control; no replacement result injection.
    worker
        .start_text(image::token(), explorer.open_reader(index).unwrap())
        .unwrap();
    finished(&worker);
    let (_, result) = worker.poll().unwrap();
    same_texture(&result.unwrap(), "mutated length and content", false);
    assert_eq!(worker.text_reads.lock().unwrap().calls, 2);
}
