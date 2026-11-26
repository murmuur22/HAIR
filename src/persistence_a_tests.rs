//! Disposable actual-input diagnostic. Producer and reopen are different processes.
//! No owner-folder access and no TMPDIR replacement; evidence is retained on disk.
use super::*;
use std::os::fd::AsRawFd;

const CHILD: &str = "directory_map_app_tests::place_mode_app_tests::placement_editor_r2a::persistence_a::persistence_a_process_child";

fn startup(evidence: &Path) -> App {
    // Match run's bundled world/host/two-pack startup, without app()'s preliminary
    // examples visit. Preferences use an owned parent, never the real HOME.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let world = World::load(&root).unwrap();
    let host = hair::host_scene::HostScene::load(&root).unwrap();
    let mut cache = Default::default();
    let themes = ["packs/office.toml", "packs/archive.toml"].into_iter()
        .map(|name| hair::theme::Theme::load_for_scene(&root.join(name), &mut cache, &host).unwrap())
        .collect();
    let mut a = App::new(root, themes, 0, world, false);
    a.restore_preferences_at(&evidence.join("preferences")).unwrap();
    a
}

fn process(evidence: &Path, kind: &str, phase: &str) -> bool {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--ignored", "--nocapture", "--test-threads=1"])
        .env("HAIR_PERSISTENCE_A_EVIDENCE", evidence)
        .env("HAIR_PERSISTENCE_A_KIND", kind)
        .env("HAIR_PERSISTENCE_A_PHASE", phase)
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::write(evidence.join(format!("{phase}.stdout.log")), &out.stdout).unwrap();
    std::fs::write(evidence.join(format!("{phase}.stderr.log")), &out.stderr).unwrap();
    println!("PERSISTENCE_A kind={kind} phase={phase} raw_exit={:?}\n{stdout}{stderr}", out.status.code());
    out.status.success() && stdout.contains(&format!("test {CHILD} ..."))
        && stdout.contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
}

#[test]
fn persistence_a_actual_publication_separate_process_reopen_matrix() {
    let run = fixture("persistence-a-retained");
    let root = run.0.clone();
    // Deliberately retain first-failure evidence, including raw records, even on
    // panic. A new exclusive fixture is used for every invocation.
    std::mem::forget(run);
    println!("PERSISTENCE_A_EVIDENCE={}", root.display());
    let mut failed = Vec::new();
    for kind in ["copy", "existing", "copy-follow", "existing-follow", "copy-pin", "existing-pin"] {
        let evidence = root.join(kind);
        for path in [&evidence, &evidence.join("room"), &evidence.join("source"), &evidence.join("preferences")] {
            std::fs::create_dir(path).unwrap();
        }
        if !process(&evidence, kind, "publish") { failed.push(format!("{kind}/publish")); continue; }
        let before = slice4c::records(&evidence.join("room"));
        if !process(&evidence, kind, "reopen") { failed.push(format!("{kind}/reopen")); }
        assert_eq!(slice4c::records(&evidence.join("room")), before, "read-only reopen must retain every raw byte/inode");
    }
    assert!(failed.is_empty(), "behavioral failures: {failed:?}; retained evidence {}", root.display());
}

fn raw_evidence(evidence: &Path, room: &Path) {
    let mut report = String::new();
    for (path, (ino, raw)) in slice4c::records(room) {
        let m = std::fs::metadata(&path).unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let mut value = [0u8; 64];
        let n = unsafe { libc::fgetxattr(file.as_raw_fd(), c"com.apple.provenance".as_ptr(), value.as_mut_ptr().cast(), value.len(), 0, 0) };
        let origin = if n >= 0 { format!("{:?}", &value[..n as usize]) }
            else { format!("absent/error={}", std::io::Error::last_os_error()) };
        report.push_str(&format!("PATH={} inode={ino} mode={:o} bytes={} provenance={origin}\n", path.display(), m.mode(), raw.len()));
        if path.extension().is_some_and(|x| x == "toml") {
            report.push_str(std::str::from_utf8(&raw).unwrap());
            report.push('\n');
        }
    }
    println!("PERSISTENCE_A_RAW\n{report}");
    std::fs::write(evidence.join("raw-before-reopen.txt"), report).unwrap();
}

fn geometry(a: &App) -> Vec<u8> {
    bytes(&a.themes[a.theme_index].vertices_for_at(&a.world, 3.))
}

// Read-only diagnostic snapshots, not replacement acquisition or retry authority.
// Preserve both bytes and path/descriptor identities at the actual UI boundaries.
fn pin_source_evidence(a: &App, evidence: &Path, phase: &str) {
    let theme = &a.themes[a.theme_index];
    let entry = &theme.decorations.entries[0];
    let asset = theme.decorations.asset(&entry.asset).unwrap();
    let mut report = format!("theme={} entry={entry:?} fit={:?}\n", theme.source.display(), asset.spec);
    for (kind, path) in [("manifest", theme.source.clone()),
        ("asset", theme.source.parent().unwrap().join(&asset.spec.file))] {
        let file = std::fs::File::open(&path).unwrap();
        for (identity, m) in [("descriptor", file.metadata().unwrap()), ("path", std::fs::symlink_metadata(&path).unwrap())] {
            report += &format!("{kind} {identity} path={} dev={} ino={} mode={:o} links={} bytes={} mtime={}.{} ctime={}.{}\n",
                path.display(), m.dev(), m.ino(), m.mode(), m.nlink(), m.len(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec());
        }
        let mut raw = vec![];
        std::io::Read::read_to_end(&mut &file, &mut raw).unwrap();
        std::fs::write(evidence.join(format!("pin-source-{phase}-{kind}.bin")), raw).unwrap();
    }
    println!("PERSISTENCE_A_PIN_SOURCE phase={phase}\n{report}");
    std::fs::write(evidence.join(format!("pin-source-{phase}.txt")), report).unwrap();
}

#[test]
#[ignore = "explicit separate-process persistence diagnostic child"]
fn persistence_a_process_child() {
    let evidence = PathBuf::from(std::env::var_os("HAIR_PERSISTENCE_A_EVIDENCE").unwrap());
    assert!(evidence.starts_with(std::env::temp_dir()));
    let room = evidence.join("room");
    let kind = std::env::var("HAIR_PERSISTENCE_A_KIND").unwrap();
    let phase = std::env::var("HAIR_PERSISTENCE_A_PHASE").unwrap();
    println!("PERSISTENCE_A_PROCESS pid={} phase={phase} kind={kind} TMPDIR={:?}", std::process::id(), std::env::var_os("TMPDIR"));
    let mut a = startup(&evidence);
    if phase == "publish" {
        std::fs::write(evidence.join("source/A.txt"), b"A.txt").unwrap();
        let source_before = slice4c::records(&evidence.join("source"));
        if kind.starts_with("copy") {
            a.complete_folder(Ok(Some(evidence.join("source")))).unwrap();
            carried(&mut a);
            a.complete_folder(Ok(Some(room.clone()))).unwrap();
            tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::KeyP);
        } else {
            std::fs::write(room.join("A.txt"), b"A.txt").unwrap();
            std::fs::write(room.join("Z.txt"), b"unrelated").unwrap();
            a.complete_folder(Ok(Some(room.clone()))).unwrap();
            aim_file(&mut a, "A.txt"); tap(&mut a, KeyCode::KeyP); tap(&mut a, KeyCode::KeyX);
        }
        let unrelated = room.join("Z.txt");
        let old_unrelated = unrelated.exists().then(|| (std::fs::metadata(&unrelated).unwrap().ino(), std::fs::read(&unrelated).unwrap()));
        size_control(&mut a, "Size: 150%");
        size_control(&mut a, "Animation: Breathe");
        // Change the base as well as pose/scale/animation through actual input.
        tap(&mut a, KeyCode::KeyG);
        tap(&mut a, KeyCode::ArrowRight);
        enter_positioning(&mut a); aim_floor(&mut a, [2., 0.]);
        tap(&mut a, KeyCode::Enter); tap(&mut a, KeyCode::Enter);
        println!("PERSISTENCE_A_FILE_STATUS={}", a.local_status);
        assert!(a.local_status.contains(if kind.starts_with("copy") { "Placed" } else { "reposition saved" }), "{}", a.local_status);
        if kind.starts_with("copy") { assert_eq!(a.inventory.items().len(), 1, "Copy retains carried item"); }
        if kind.contains('-') {
            if a.place_mode.is_none() { tap(&mut a, KeyCode::KeyP); }
            tap(&mut a, KeyCode::KeyB); tap(&mut a, KeyCode::Enter);
            if kind.ends_with("pin") { pin_source_evidence(&a, &evidence, "selected"); }
            // Preserve the old Follow writer diagnostic, not a new UI default.
            if kind.ends_with("follow") { a.legacy_follow_fixture(); }
            size_control(&mut a, "Size: 75%");
            tap(&mut a, KeyCode::ArrowRight);
            if kind.ends_with("pin") { tap(&mut a, KeyCode::KeyV); }
            enter_positioning(&mut a); aim_floor(&mut a, [-2., 0.]);
            tap(&mut a, KeyCode::Enter);
            if kind.ends_with("pin") { pin_source_evidence(&a, &evidence, "confirming"); }
            tap(&mut a, KeyCode::Enter);
            println!("PERSISTENCE_A_DECORATION_STATUS={}", a.local_status);
            assert!(a.local_status.contains(if kind.ends_with("pin") { "decoration pinned" } else { "decoration saved" }), "{}", a.local_status);
        }
        let live = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
        assert_eq!(live.positions.len(), 1); assert!(live.warnings.is_empty()); assert_eq!(live.incomplete_records, 0);
        std::fs::write(evidence.join("expected-position.toml"), toml::to_string(&live.positions[0].1).unwrap()).unwrap();
        if kind.contains('-') {
            assert_eq!(live.decorations.len(), 1);
            std::fs::write(evidence.join("expected-decoration.toml"), toml::to_string(&live.decorations[0]).unwrap()).unwrap();
        }
        assert_eq!(slice4c::records(&evidence.join("source")), source_before);
        if let Some(old) = old_unrelated { assert_eq!((std::fs::metadata(&unrelated).unwrap().ino(), std::fs::read(&unrelated).unwrap()), old); }
        raw_evidence(&evidence, &room);
        std::fs::write(evidence.join("expected-scene.bin"), geometry(&a)).unwrap();
        std::fs::write(evidence.join("expected-file.bin"), file_mesh(&a, "A.txt")).unwrap();
        std::fs::write(evidence.join("expected-base.bin"), base_mesh(&a, "A.txt")).unwrap();
        a.menu = Some(hair::presentation::Menu::pause());
        assert!(a.activate_menu(hair::presentation::Item::Quit));
        drop(a); // Parent waits for process exit before launching the reopener.
        return;
    }
    assert_eq!(phase, "reopen");
    let before = slice4c::records(&room);
    let expected: hair::placement_store::Position = toml::from_str(&std::fs::read_to_string(evidence.join("expected-position.toml")).unwrap()).unwrap();
    a.complete_folder(Ok(Some(room.clone()))).unwrap();
    println!("PERSISTENCE_A_REOPEN_STATUS={}", a.local_status);
    for refresh in [false, true] {
        if refresh { tap(&mut a, KeyCode::F5); }
        let id = a.world.folder_view.as_ref().unwrap().bindings.iter().find(|(_, b)| b.entry.name == "A.txt").unwrap().0;
        assert_eq!(a.themes[0].host.file_placements.get(id).map(|p| &p.position), Some(&expected), "installed file reset: {}", a.local_status);
        assert_eq!(file_mesh(&a, "A.txt"), std::fs::read(evidence.join("expected-file.bin")).unwrap());
        assert_eq!(base_mesh(&a, "A.txt"), std::fs::read(evidence.join("expected-base.bin")).unwrap());
        if kind.contains('-') {
            let d: hair::placement_store::Decoration = toml::from_str(&std::fs::read_to_string(evidence.join("expected-decoration.toml")).unwrap()).unwrap();
            assert_eq!(a.themes[0].host.decorations.get(&d.placement_id), Some(&d), "installed decoration disappeared: {}", a.local_status);
            let installed = &a.themes[0].host.decorations[&d.placement_id];
            assert!(a.themes[0].decoration_preview(installed).unwrap().len() > 36);
        }
        assert_eq!(geometry(&a), std::fs::read(evidence.join("expected-scene.bin")).unwrap(), "loaded renderer scene geometry must consume saved attributes");
        assert_eq!(slice4c::records(&room), before);
    }
    println!("PERSISTENCE_A_INSTALLED_AND_GEOMETRY_PASS kind={kind}");
}
