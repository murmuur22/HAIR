//! Post-implementation qualification, NOT the protected feature RED tests.
//! Synthetic input uses the actual App adapter. No GPU/native-input claim.
//! Registered below folder_native only to inspect ownership on Escape; all body
//! acquisition/paging/formatting uses existing Explorer/ReaderFile/App APIs.
use super::super::*;
use hair::local_explorer::{Explorer, MAX_BODY_BYTES, READER_CHUNK_BYTES, ReaderChunk};
use std::{
    ffi::{CString, OsStr},
    fs,
    io::{Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    process::Command,
    time::{Duration, Instant, SystemTime},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(tag: &str) -> Self {
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let owned = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        assert!(temp.starts_with(owned), "only the qualified owned TMPDIR");
        let path = temp.join(format!("rawq-{tag}-{}", std::process::id()));
        fs::create_dir(&path).expect("exclusive fixture; do not overwrite leftovers");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn index(explorer: &Explorer, name: &str) -> usize {
    explorer
        .current()
        .entries
        .iter()
        .position(|e| e.name == OsStr::new(name))
        .unwrap()
}
fn key(app: &mut App, code: KeyCode, down: bool, repeat: bool) {
    assert!(!app.keyboard_input(Some(code), down, repeat, None).unwrap());
}
fn tap(app: &mut App, code: KeyCode) {
    key(app, code, true, false);
    key(app, code, false, false);
}
fn app(folder: &Path) -> App {
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
    app.complete_folder(Ok(Some(folder.canonicalize().unwrap())))
        .unwrap();
    app
}
fn aim(app: &mut App, name: &str) {
    let id = app
        .world
        .folder_view
        .as_ref()
        .unwrap()
        .bindings
        .iter()
        .find(|(_, b)| b.entry.name == OsStr::new(name))
        .unwrap()
        .0
        .clone();
    let host = &app.themes[0].host;
    let anchor = host.anchors.iter().find(|a| a.id == id).unwrap();
    let normal = host.portal_inward(&id);
    app.pose = Pose {
        position: [
            anchor.at[0] + 1.05 * normal[0],
            anchor.at[2] + 1.05 * normal[1],
        ],
        yaw: (-normal[0]).atan2(normal[1]),
        pitch: ((anchor.at[1] + anchor.size[1] * 0.5 - 1.65) / 1.05).atan(),
    };
    app.world.explicit_target = None;
    hair::interaction::refresh(host, &app.pose, &mut app.world);
    assert_eq!(app.world.interaction.as_ref().unwrap().anchor, id);
}
fn open(app: &mut App, name: &str) {
    aim(app, name);
    tap(app, KeyCode::KeyE);
    assert!(app.world.reading);
    assert!(app.local.as_ref().unwrap().reader.is_some());
}
fn document(app: &App) -> &str {
    app.world.document().unwrap()
}
fn assert_range(doc: &str, start: usize, end: usize, len: usize) {
    assert!(
        doc.contains(&format!("RANGE [0x{start:016X}, 0x{end:016X})")),
        "{doc}"
    );
    assert!(doc.contains(&format!(
        "Observed length: {len} bytes | {}",
        if end >= len {
            "EOF (next inert)"
        } else {
            "MORE"
        }
    )));
    assert!(doc.contains("LIMIT: 4096 bytes/chunk; no whole-file load"));
    assert!(doc.len() <= 262144);
}
fn raw_section(doc: &str) -> &str {
    doc.split_once("\nBYTE GROUPS (8 bytes max)\n")
        .unwrap()
        .1
        .split_once("\nFULL SOURCE / METADATA:\n")
        .unwrap()
        .0
}
fn text_section(doc: &str) -> &str {
    doc.split_once("\nTEXT (links inactive; escapes shown; range only):\n")
        .unwrap()
        .1
        .split_once("\n\nFULL SOURCE / METADATA:\n")
        .unwrap()
        .0
}
fn assert_raw(doc: &str, start: usize, expected: &[u8]) {
    assert!(doc.starts_with("RAW STORED BYTES\n"));
    // Parse independently of the production formatter. Exact separators and
    // counts reject missing/extra groups, octets, ASCII, and incorrect offsets.
    let section = raw_section(doc);
    let groups: Vec<_> = if section.is_empty() {
        Vec::new()
    } else {
        section
            .strip_suffix("\n\n")
            .unwrap()
            .split("\n\n")
            .collect()
    };
    assert_eq!(groups.len(), expected.len().div_ceil(8));
    let mut byte_count = 0;
    for (row, (group, bytes)) in groups.iter().zip(expected.chunks(8)).enumerate() {
        let lines: Vec<_> = group.lines().collect();
        assert_eq!(lines.len(), 3, "offset, hex, and ASCII stay associated");
        assert!(lines.iter().all(|line| line.chars().count() <= 31));
        assert_eq!(lines[0], format!("OFFSET {:016X}", start + row * 8));
        let hex = lines[1].strip_prefix("HEX ").unwrap();
        assert_eq!(hex.len(), bytes.len() * 3 - 1);
        let parsed: Vec<_> = hex
            .split(' ')
            .map(|v| {
                assert_eq!(v.len(), 2);
                assert!(
                    v.bytes()
                        .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
                );
                u8::from_str_radix(v, 16).unwrap()
            })
            .collect();
        assert_eq!(parsed.len(), bytes.len());
        assert_eq!(parsed, bytes);
        byte_count += parsed.len();
        let visible: String = bytes
            .iter()
            .map(|b| {
                if b.is_ascii_graphic() || *b == b' ' {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect();
        assert_eq!(lines[2].strip_prefix("ASCII ").unwrap(), visible);
    }
    assert_eq!(byte_count, expected.len());
    assert!(
        doc.bytes()
            .all(|b| b == b'\n' || (b' '..=b'~').contains(&b)),
        "no unescaped controls or font-unsupported bytes"
    );
}
#[test]
fn grouped_raw_parser_rejects_wrong_counts_offsets_octets_and_ascii() {
    // Hand-authored control, never generated by raw_body or Session::document.
    let valid = "RAW STORED BYTES\n\nBYTE GROUPS (8 bytes max)\nOFFSET 0000000000001000\nHEX 00 01 02 03 41 7F 80 FF\nASCII ....A...\n\n\nFULL SOURCE / METADATA:\n";
    let bytes = [0, 1, 2, 3, 0x41, 0x7f, 0x80, 0xff];
    assert_raw(valid, 4096, &bytes);
    for invalid in [
        valid.replace("OFFSET 0000000000001000", "OFFSET 0000000000001008"),
        valid.replace("HEX 00 01 02 03 41 7F 80 FF", "HEX 00 01 02 03 41 7F 80"),
        valid.replace(
            "HEX 00 01 02 03 41 7F 80 FF",
            "HEX 00 01 02 03 41 7F 80 FF 00",
        ),
        valid.replace("HEX 00", "HEX 01"),
        valid.replace("7F", "7f"),
        valid.replace("ASCII ....A...", "ASCII ....B..."),
        valid.replace("ASCII ....A...", "ASCII ....A.."),
        valid.replace(
            "\n\n\nFULL SOURCE",
            "\n\nOFFSET 0000000000001008\nHEX 00\nASCII .\n\n\nFULL SOURCE",
        ),
    ] {
        assert!(
            std::panic::catch_unwind(|| assert_raw(&invalid, 4096, &bytes)).is_err(),
            "independent parser accepted an invalid group: {invalid:?}"
        );
    }
}

fn assert_backend_chunk(c: &ReaderChunk, start: u64, body: &[u8], len: u64) {
    assert_eq!(c.bytes.len(), 4096);
    assert_eq!(c.used, body.len());
    assert_eq!(c.start, start);
    assert_eq!(c.end, start + body.len() as u64);
    assert_eq!(c.observed_len, len);
    assert_eq!(c.eof, c.end >= len);
    assert_eq!(c.body(), body);
}
fn mutation_stamp(file: &fs::File) {
    // Deterministic distinguishable mtime, no sleeps or chmod of external data.
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(42))
        .unwrap();
    file.sync_all().unwrap();
}

#[test]
fn sparse_large_and_empty_ranges_keep_separate_chunk_and_whole_body_limits() {
    let f = Fixture::new("bounds");
    let path = f.0.join("data.bin");
    let len = MAX_BODY_BYTES + 8193;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_len(len).unwrap(); // Sparse: no fixture-sized Vec or whole-file read.
    file.write_all(b"ORIGINAL").unwrap();
    file.seek(SeekFrom::Start(4096)).unwrap();
    file.write_all(b"SECOND!!").unwrap();
    file.seek(SeekFrom::Start(len - 1)).unwrap();
    file.write_all(&[0xee]).unwrap();
    drop(file);
    let mode = fs::metadata(&path).unwrap().mode();
    let explorer = Explorer::new(&f.0).unwrap();
    let i = index(&explorer, "data.bin");
    for limit in [0, 1024, MAX_BODY_BYTES, MAX_BODY_BYTES + 1, u64::MAX] {
        assert!(
            explorer.read_entry(i, limit).is_err(),
            "old caller/4MiB ceiling must remain: {limit}"
        );
    }
    assert!(explorer.open_reader(usize::MAX).is_err());
    let reader = explorer.open_reader(i).unwrap();
    assert_eq!(READER_CHUNK_BYTES, 4096);
    assert!(
        std::mem::size_of::<ReaderChunk>() < 8192,
        "fixed-size result, not allocation by file length"
    );
    let mut first = [0; 4096];
    first[..8].copy_from_slice(b"ORIGINAL");
    assert_backend_chunk(&reader.read_chunk(0).unwrap(), 0, &first, len);
    let mut second = [0; 4096];
    second[..8].copy_from_slice(b"SECOND!!");
    assert_backend_chunk(&reader.read_chunk(4096).unwrap(), 4096, &second, len);
    assert_backend_chunk(&reader.read_chunk(len - 1).unwrap(), len - 1, &[0xee], len);
    for offset in [len, len + 1, libc::off_t::MAX as u64 - 4096] {
        assert_backend_chunk(&reader.read_chunk(offset).unwrap(), offset, &[], len);
    }
    assert!(
        reader
            .read_chunk(u64::MAX)
            .unwrap_err()
            .contains("offset overflow")
    );
    assert!(
        reader
            .read_chunk(u64::MAX - 4095)
            .unwrap_err()
            .contains("offset overflow")
    );
    assert!(
        reader
            .read_chunk(libc::off_t::MAX as u64 - 4095)
            .unwrap_err()
            .contains("OS offset limit")
    );
    assert!(
        reader
            .read_chunk(libc::off_t::MAX as u64)
            .unwrap_err()
            .contains("OS offset limit")
    );
    assert_eq!(fs::metadata(&path).unwrap().len(), len);
    assert_eq!(fs::metadata(&path).unwrap().mode(), mode);
    fs::write(f.0.join("empty"), []).unwrap();
    let empty = Explorer::new(&f.0).unwrap();
    let empty_reader = empty.open_reader(index(&empty, "empty")).unwrap();
    assert_backend_chunk(&empty_reader.read_chunk(0).unwrap(), 0, &[], 0);
    assert_backend_chunk(&empty_reader.read_chunk(1).unwrap(), 1, &[], 0);
    // Also exercise actual App acquisition on a >4MiB ordinary file.
    let mut app = app(&f.0);
    open(&mut app, "data.bin");
    assert_range(document(&app), 0, 4096, len as usize);
    assert_raw(document(&app), 0, &first);
    tap(&mut app, KeyCode::ArrowRight);
    assert_range(document(&app), 4096, 8192, len as usize);
    assert_raw(document(&app), 4096, &second);
    tap(&mut app, KeyCode::Escape);
    open(&mut app, "empty");
    assert_range(document(&app), 0, 0, 0);
    assert!(document(&app).contains("Empty byte range / EOF"));
    let empty_doc = document(&app).to_owned();
    for code in [KeyCode::ArrowLeft, KeyCode::ArrowRight] {
        tap(&mut app, code);
        assert_eq!(document(&app), empty_doc);
    }
}

#[test]
fn renamed_file_and_parent_keep_original_handle_and_retained_directory() {
    let f = Fixture::new("renames");
    let chosen = f.0.join("chosen");
    fs::create_dir(&chosen).unwrap();
    let bytes = [b"ORIGINAL".as_slice(), &[0; 4088], b"SECOND!!", &[0]].concat();
    fs::write(chosen.join("data.bin"), &bytes).unwrap();
    let explorer = Explorer::new(&chosen).unwrap();
    let i = index(&explorer, "data.bin");
    let mut reader = explorer.open_reader(i).unwrap();
    let mut app = app(&chosen);
    open(&mut app, "data.bin");
    let initial = document(&app).to_owned();
    fs::rename(chosen.join("data.bin"), chosen.join("held.bin")).unwrap();
    fs::write(chosen.join("data.bin"), b"WRONG REPLACEMENT").unwrap();
    let moved = f.0.join("moved");
    fs::rename(&chosen, &moved).unwrap();
    fs::create_dir(&chosen).unwrap();
    fs::write(chosen.join("data.bin"), b"WRONG DIRECTORY").unwrap();
    match reader.read_chunk(4096) {
        Ok(c) => assert_backend_chunk(&c, 4096, &bytes[4096..], bytes.len() as u64),
        Err(e) => assert!(
            e.contains("changed since baseline"),
            "rename ctime must be explicit: {e}"
        ),
    }
    assert_backend_chunk(
        &reader.retry_from_zero().unwrap(),
        0,
        &bytes[..4096],
        bytes.len() as u64,
    );
    let retained = explorer.open_reader(i).unwrap();
    assert_eq!(
        retained.read_chunk(0).unwrap().body(),
        b"WRONG REPLACEMENT",
        "new acquisition uses held directory, not rebound display path"
    );
    // V/I operate on cached bytes, even after both displayed paths rebound.
    tap(&mut app, KeyCode::KeyV);
    assert!(document(&app).starts_with("TEXT:"));
    assert!(!document(&app).contains("STALE / READ ERROR:"));
    assert!(text_section(document(&app)).starts_with("ORIGINAL"));
    tap(&mut app, KeyCode::KeyI);
    assert!(document(&app).contains("METADATA VIEW"));
    tap(&mut app, KeyCode::KeyI);
    tap(&mut app, KeyCode::KeyV);
    assert_eq!(document(&app), initial);
    tap(&mut app, KeyCode::ArrowRight);
    if document(&app).contains("STALE / READ ERROR:") {
        assert_range(document(&app), 0, 4096, bytes.len());
        assert_raw(document(&app), 0, &bytes[..4096]);
        let stale = document(&app).to_owned();
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(document(&app), stale);
    } else {
        assert_raw(document(&app), 4096, &bytes[4096..]);
    }
    tap(&mut app, KeyCode::KeyC);
    assert_range(document(&app), 0, 4096, bytes.len());
    assert_raw(document(&app), 0, &bytes[..4096]);
    assert!(!document(&app).contains("STALE / READ ERROR:"));
    tap(&mut app, KeyCode::ArrowRight);
    assert_raw(document(&app), 4096, &bytes[4096..]);
    assert_eq!(fs::read(moved.join("held.bin")).unwrap(), bytes);
    assert_eq!(
        fs::read(moved.join("data.bin")).unwrap(),
        b"WRONG REPLACEMENT"
    );
    assert_eq!(
        fs::read(chosen.join("data.bin")).unwrap(),
        b"WRONG DIRECTORY"
    );
}

#[test]
fn same_inode_truncate_grow_and_overwrite_suspend_paging_keep_scroll_and_c_reset() {
    for kind in ["truncate", "grow", "overwrite"] {
        let f = Fixture::new(kind);
        let path = f.0.join("data.bin");
        let bytes = vec![0; 8193];
        fs::write(&path, &bytes).unwrap();
        let before = fs::metadata(&path).unwrap();
        let mut app = app(&f.0);
        open(&mut app, "data.bin");
        tap(&mut app, KeyCode::ArrowRight);
        for _ in 0..30 {
            tap(&mut app, KeyCode::ArrowDown);
        }
        let scroll = app.scroll;
        assert_eq!(scroll, 30);
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        match kind {
            "truncate" => file.set_len(17).unwrap(),
            "grow" => file.set_len(12289).unwrap(),
            "overwrite" => file.write_all_at(b"NEWBYTES", 0).unwrap(),
            _ => unreachable!(),
        }
        mutation_stamp(&file);
        drop(file);
        let after = fs::metadata(&path).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(before.mode(), after.mode());
        // Formatting alone must not stat/read or silently accept a new baseline.
        tap(&mut app, KeyCode::KeyV);
        assert!(!document(&app).contains("STALE / READ ERROR:"));
        tap(&mut app, KeyCode::KeyV);
        tap(&mut app, KeyCode::ArrowRight);
        assert!(document(&app).contains("STALE / READ ERROR: opened file changed since baseline"));
        assert!(document(&app).contains("Last good bytes/range retained; paging suspended."));
        assert_range(document(&app), 4096, 8192, bytes.len());
        assert_raw(document(&app), 4096, &bytes[4096..8192]);
        assert_eq!(app.scroll, scroll);
        let stale = document(&app).to_owned();
        for code in [KeyCode::ArrowLeft, KeyCode::ArrowRight] {
            tap(&mut app, code);
            assert_eq!(document(&app), stale);
            assert_eq!(app.scroll, scroll);
        }
        let new_bytes = fs::read(&path).unwrap();
        tap(&mut app, KeyCode::KeyC);
        assert_eq!(app.scroll, 0);
        assert!(!document(&app).contains("STALE / READ ERROR:"));
        let end = new_bytes.len().min(4096);
        assert_range(document(&app), 0, end, new_bytes.len());
        assert_raw(document(&app), 0, &new_bytes[..end]);
        assert_eq!(
            fs::read(&path).unwrap(),
            new_bytes,
            "App must not write during recovery"
        );
    }
}

#[test]
fn preopen_link_socket_and_directory_swaps_refuse_retained_regular_selection() {
    let f = Fixture::new("preopen");
    fs::write(f.0.join("control"), b"DO NOT READ THROUGH LINK").unwrap();
    for kind in ["link", "socket", "directory"] {
        let path = f.0.join(kind);
        fs::write(&path, b"old regular file").unwrap();
        let explorer = Explorer::new(&f.0).unwrap();
        let i = index(&explorer, kind);
        let mut app = app(&f.0);
        aim(&mut app, kind); // retain the actual old regular-file App binding
        fs::remove_file(&path).unwrap();
        let socket = match kind {
            "link" => {
                std::os::unix::fs::symlink("control", &path).unwrap();
                None
            }
            "socket" => Some(UnixListener::bind(&path).unwrap()),
            "directory" => {
                fs::create_dir(&path).unwrap();
                None
            }
            _ => unreachable!(),
        };
        assert!(explorer.open_reader(i).is_err());
        tap(&mut app, KeyCode::KeyE);
        assert!(document(&app).contains("NO BODY READ: no successful byte range"));
        assert!(document(&app).contains("No safe handle acquired."));
        assert!(!document(&app).contains("\nBYTE GROUPS (8 bytes max)\n"));
        assert!(!document(&app).contains("\nOFFSET "));
        assert!(!document(&app).contains("DO NOT READ THROUGH LINK"));
        let fresh = Explorer::new(&f.0).unwrap();
        assert!(
            fresh.open_reader(index(&fresh, kind)).is_err(),
            "known nonregular kind refused too"
        );
        drop(socket);
    }
    assert_eq!(
        fs::read(f.0.join("control")).unwrap(),
        b"DO NOT READ THROUGH LINK"
    );
}

#[test]
fn permission_refusal_requires_nonprivileged_owned_fixture_and_preserves_mode() {
    // A privileged run is not valid permission-refusal evidence; fail explicitly.
    // SAFETY: identity query only, no credentials/settings modifications.
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "run qualification unprivileged"
    );
    let f = Fixture::new("permissions");
    let path = f.0.join("private.bin");
    fs::write(&path, b"OWNED PRIVATE CONTROL").unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    let explorer = Explorer::new(&f.0).unwrap();
    let mut app = app(&f.0);
    aim(&mut app, "private.bin");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    let backend_refused = explorer
        .open_reader(index(&explorer, "private.bin"))
        .is_err();
    tap(&mut app, KeyCode::KeyE);
    let failed = document(&app).to_owned();
    let mode = fs::metadata(&path).unwrap().mode() & 0o777;
    // Restore only this owned fixture even if the assertions below find a bug.
    fs::set_permissions(&path, permissions).unwrap();
    assert!(backend_refused);
    assert_eq!(mode, 0);
    assert!(failed.contains("STALE / READ ERROR: open reader:"));
    assert!(failed.contains("NO BODY READ:"));
    assert!(!failed.contains("OWNED PRIVATE CONTROL"));
    assert_eq!(fs::read(&path).unwrap(), b"OWNED PRIVATE CONTROL");
}

#[test]
fn fifo_preopen_swap_is_bounded_and_helper_is_reaped() {
    // Established local_explorer_inspection subprocess pattern, not a detached
    // thread that could keep the runner hungry. No devices, shell or FIFO writes.
    if let Some(path) = std::env::var_os("HAIR_RAW_QUAL_FIFO_CHILD") {
        let root = PathBuf::from(path).canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let fifo = root.join("data.bin");
        let explorer = Explorer::new(&root).unwrap();
        let i = index(&explorer, "data.bin");
        fs::remove_file(&fifo).unwrap();
        let component = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: exclusively owned synthetic FIFO and valid NUL-free path.
        assert_eq!(unsafe { libc::mkfifo(component.as_ptr(), 0o600) }, 0);
        assert!(explorer.open_reader(i).is_err(), "reject before body read");
        let fresh = Explorer::new(&root).unwrap();
        assert!(fresh.open_reader(index(&fresh, "data.bin")).is_err());
        return;
    }
    let f = Fixture::new("fifo");
    fs::write(f.0.join("data.bin"), b"regular before snapshot").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "folder_native::raw_reader_qualification_tests::fifo_preopen_swap_is_bounded_and_helper_is_reaped", "--nocapture", "--test-threads=1"])
        .env("HAIR_RAW_QUAL_FIFO_CHILD", &f.0).spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Parent owns cleanup even after kill; helper cannot survive this test.
    assert!(
        status.is_some_and(|s| s.success()),
        "FIFO acquisition failed/hung; helper reaped; no pass claim"
    );
}

#[test]
fn every_byte_has_exact_hex_printable_ascii_and_lossless_escaped_text() {
    let f = Fixture::new("all-bytes");
    let bytes: Vec<u8> = (0..=255).collect();
    let path = f.0.join("bytes.dat");
    fs::write(&path, &bytes).unwrap();
    let mode = fs::metadata(&path).unwrap().mode();
    let mut app = app(&f.0);
    open(&mut app, "bytes.dat");
    assert_range(document(&app), 0, bytes.len(), bytes.len());
    assert_raw(document(&app), 0, &bytes);
    tap(&mut app, KeyCode::KeyV);
    let expected: String = bytes
        .iter()
        .map(|&b| match b {
            b'\n' => "\n".to_owned(),
            b'\\' => "\\\\".to_owned(),
            b' '..=b'~' => (b as char).to_string(),
            _ => format!("\\x{b:02X}"),
        })
        .collect();
    assert_eq!(text_section(document(&app)), expected);
    assert!(
        document(&app)
            .bytes()
            .all(|b| b == b'\n' || (b' '..=b'~').contains(&b))
    );
    assert!(!document(&app).contains('\u{fffd}'));
    tap(&mut app, KeyCode::KeyV);
    assert_raw(document(&app), 0, &bytes);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().mode(), mode);
}

#[test]
fn utf8_split_and_incomplete_eof_use_zero_lookaround_without_dropping_bytes() {
    let f = Fixture::new("utf8-boundary");
    let mut bytes = vec![b'A'; 4095];
    bytes.extend_from_slice(&[
        0xe2, 0x82, 0xac, 0xc3, 0xa9, b'\t', b'\r', b'\n', 0, 0x1b, 0xff, b'\\', 0xe2, 0x82,
    ]);
    fs::write(f.0.join("split.opaque"), &bytes).unwrap();
    let mut app = app(&f.0);
    open(&mut app, "split.opaque");
    assert_raw(document(&app), 0, &bytes[..4096]);
    tap(&mut app, KeyCode::KeyV);
    assert_eq!(
        text_section(document(&app)),
        format!("{}\\xE2", "A".repeat(4095))
    );
    tap(&mut app, KeyCode::ArrowRight);
    assert_range(document(&app), 4096, bytes.len(), bytes.len());
    assert_eq!(
        text_section(document(&app)),
        "\\x82\\xAC\\u{E9}\\x09\\x0D\n\\x00\\x1B\\xFF\\\\\\xE2\\x82"
    );
    assert!(
        !document(&app).contains("\\u{20AC}"),
        "split Euro sign must NOT be inferred across chunks"
    );
    assert!(
        document(&app)
            .bytes()
            .all(|b| b == b'\n' || (b' '..=b'~').contains(&b))
    );
    tap(&mut app, KeyCode::KeyV);
    assert_raw(document(&app), 4096, &bytes[4096..]);
    tap(&mut app, KeyCode::ArrowLeft);
    assert_raw(document(&app), 0, &bytes[..4096]);
    assert_eq!(fs::read(f.0.join("split.opaque")).unwrap(), bytes);
}

#[test]
fn actual_reader_keys_latch_release_focus_modal_and_failed_selection_isolate_last_good() {
    let f = Fixture::new("keys");
    fs::write(f.0.join("data.bin"), vec![0; 8193]).unwrap();
    let mut app = app(&f.0);
    open(&mut app, "data.bin");
    let place = app.local.as_ref().unwrap().explorer.current().clone();
    let pose = (app.pose.position, app.pose.yaw, app.pose.pitch);
    let navigation = app.world.navigation.clone();
    for code in [
        KeyCode::ArrowRight,
        KeyCode::ArrowLeft,
        KeyCode::KeyV,
        KeyCode::KeyI,
        KeyCode::KeyC,
    ] {
        key(&mut app, code, true, false);
        let committed = document(&app).to_owned();
        match code {
            KeyCode::ArrowRight => assert_range(&committed, 4096, 8192, 8193),
            KeyCode::ArrowLeft | KeyCode::KeyC => assert_range(&committed, 0, 4096, 8193),
            KeyCode::KeyV => assert!(committed.starts_with("TEXT:")),
            KeyCode::KeyI => assert!(
                committed.contains("METADATA VIEW")
                    && !committed.contains("\nTEXT (links inactive")
            ),
            _ => unreachable!(),
        }
        for repeat in [true, false, true] {
            key(&mut app, code, true, repeat);
            assert_eq!(document(&app), committed, "held/repeat {code:?}");
        }
        key(&mut app, code, false, false);
    }
    tap(&mut app, KeyCode::KeyI); // back to retained text body
    for _ in 0..5 {
        tap(&mut app, KeyCode::ArrowDown);
    }
    assert_eq!(app.scroll, 5);
    tap(&mut app, KeyCode::KeyI);
    assert_eq!(app.scroll, 0);
    tap(&mut app, KeyCode::ArrowDown);
    tap(&mut app, KeyCode::KeyI);
    assert_eq!(app.scroll, 5, "I restores each view's saved scroll");
    tap(&mut app, KeyCode::KeyV); // raw body again
    let stable = document(&app).to_owned();
    app.focus_changed(false);
    assert!(app.local_held.is_empty() && !app.input.forward && !app.input.running);
    for code in [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::KeyV,
        KeyCode::KeyI,
        KeyCode::KeyC,
    ] {
        tap(&mut app, code);
    }
    assert_eq!(document(&app), stable);
    app.focus_changed(true);
    key(&mut app, KeyCode::ArrowRight, true, true);
    assert_eq!(
        document(&app),
        stable,
        "focus regain repeat is not a new press"
    );
    key(&mut app, KeyCode::ArrowRight, false, false);
    tap(&mut app, KeyCode::ArrowRight);
    assert_range(document(&app), 4096, 8192, 8193);
    tap(&mut app, KeyCode::ArrowLeft);
    let stable = document(&app).to_owned();
    for code in [
        KeyCode::KeyE,
        KeyCode::KeyU,
        KeyCode::KeyR,
        KeyCode::F5,
        KeyCode::KeyO,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::KeyL,
        KeyCode::Tab,
        KeyCode::KeyW,
        KeyCode::ShiftLeft,
    ] {
        tap(&mut app, code);
    }
    assert_eq!(document(&app), stable);
    assert_eq!(app.local.as_ref().unwrap().explorer.current(), &place);
    assert_eq!((app.pose.position, app.pose.yaw, app.pose.pitch), pose);
    assert_eq!(app.world.navigation, navigation);
    assert!(app.web.is_none() && app.world.reader_link.is_none() && app.address.is_none());
    assert!(!app.input.forward && !app.input.running && !app.chooser_pending);
    // Explicitly staged modal boundary controls, not a claim that L/O can open
    // these modals from reading. Keys still go through actual keyboard_input.
    for pending in [true, false] {
        app.chooser_pending = pending;
        app.chooser_active = !pending;
        for code in [
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::KeyV,
            KeyCode::KeyI,
            KeyCode::KeyC,
        ] {
            tap(&mut app, code);
        }
        assert_eq!(document(&app), stable);
        app.complete_folder(Ok(None)).unwrap();
    }
    app.address = Some("not submitted".into());
    for code in [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::KeyV,
        KeyCode::KeyI,
        KeyCode::KeyC,
    ] {
        tap(&mut app, code);
    }
    assert_eq!(document(&app), stable);
    assert_eq!(app.address.as_deref(), Some("not submitted"));
    tap(&mut app, KeyCode::Escape); // close address, not reader
    assert!(app.world.reading && app.address.is_none());
    for selection in [
        Err("owned chooser refusal".into()),
        Ok(Some(f.0.join("absent"))),
    ] {
        assert!(app.complete_folder(selection).is_err());
        assert_eq!(document(&app), stable);
        assert_eq!(app.local.as_ref().unwrap().explorer.current(), &place);
        assert!(app.local.as_ref().unwrap().reader.is_some());
    }
    tap(&mut app, KeyCode::ArrowRight);
    assert_range(document(&app), 4096, 8192, 8193);
    tap(&mut app, KeyCode::Escape);
    assert!(!app.world.reading);
    assert!(
        app.local.as_ref().unwrap().reader.is_none(),
        "Escape disposes the actual owned session"
    );
    let closed = app.world.folder_view.as_ref().unwrap().clone();
    for code in [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::KeyV,
        KeyCode::KeyC,
    ] {
        tap(&mut app, code);
    }
    assert!(!app.world.reading && app.local.as_ref().unwrap().reader.is_none());
    assert_eq!(app.world.folder_view.as_ref().unwrap(), &closed);
    assert_eq!(fs::read(f.0.join("data.bin")).unwrap(), vec![0; 8193]);
}

#[test]
fn held_c_cannot_accept_a_mutation_until_release_and_new_press() {
    let f = Fixture::new("c-latch");
    let path = f.0.join("data.bin");
    fs::write(&path, vec![0; 8193]).unwrap();
    let mut app = app(&f.0);
    open(&mut app, "data.bin");
    key(&mut app, KeyCode::KeyC, true, false);
    let accepted = document(&app).to_owned();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(7).unwrap();
    mutation_stamp(&file);
    drop(file);
    for repeat in [true, false, true] {
        key(&mut app, KeyCode::KeyC, true, repeat);
        assert_eq!(
            document(&app),
            accepted,
            "held C must not accept a changed baseline"
        );
    }
    tap(&mut app, KeyCode::ArrowRight);
    assert!(document(&app).contains("STALE / READ ERROR:"));
    assert_range(document(&app), 0, 4096, 8193);
    key(&mut app, KeyCode::KeyC, false, false);
    tap(&mut app, KeyCode::KeyC);
    assert!(!document(&app).contains("STALE / READ ERROR:"));
    assert_range(document(&app), 0, 7, 7);
    assert_raw(document(&app), 0, &[0; 7]);
    assert_eq!(fs::read(&path).unwrap(), vec![0; 7]);
}

#[test]
fn r_remains_history_only_outside_reader_and_never_retries_changed_bytes() {
    let f = Fixture::new("history");
    let child = f.0.join("Child");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("data.bin"), vec![0; 8193]).unwrap();
    let mut app = app(&f.0);
    aim(&mut app, "Child");
    tap(&mut app, KeyCode::KeyE);
    assert_eq!(app.local.as_ref().unwrap().explorer.current().folder, child);
    open(&mut app, "data.bin");
    let file = fs::OpenOptions::new()
        .write(true)
        .open(child.join("data.bin"))
        .unwrap();
    file.set_len(7).unwrap();
    mutation_stamp(&file);
    drop(file);
    tap(&mut app, KeyCode::ArrowRight);
    let stale = document(&app).to_owned();
    assert!(stale.contains("STALE / READ ERROR:"));
    tap(&mut app, KeyCode::KeyR);
    assert_eq!(
        document(&app),
        stale,
        "R is neither retry nor history while reading"
    );
    assert_eq!(app.local.as_ref().unwrap().explorer.current().folder, child);
    tap(&mut app, KeyCode::Escape);
    assert!(app.local.as_ref().unwrap().reader.is_none());
    tap(&mut app, KeyCode::KeyR);
    assert_eq!(app.local.as_ref().unwrap().explorer.current().folder, f.0);
    assert!(!app.world.reading && app.local.as_ref().unwrap().reader.is_none());
    assert_eq!(fs::read(child.join("data.bin")).unwrap(), vec![0; 7]);
}
