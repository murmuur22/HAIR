//! Library metadata tracer, NOT an App E/input or decoded-media test.
//! No filesystem access: all names and paths are supported UTF-8/control fixtures.
use hair::{
    folder_scene,
    local_explorer::{Entry, Kind, Snapshot},
    text,
};

fn mkv(extension: &str) -> Entry {
    let name = format!("MiXeD\nClip-É-NameTail.{extension}");
    Entry {
        path: std::path::PathBuf::from(format!(
            "/Fixture{}/Parent\tÉ-PathEnd",
            "/SegmentMiXeD".repeat(20),
        ))
        .join(&name),
        name: name.into(),
        kind: Kind::File,
    }
}
fn assert_no_fabricated_decoded_fields(metadata: &str) {
    let lower = metadata.to_ascii_lowercase();
    for line in lower.lines() {
        let line = line.trim();
        if line.starts_with("codec:") || line.starts_with("duration:") {
            assert!(
                line.contains("unknown")
                    || line.contains("unavailable")
                    || line.contains("not decoded"),
                "metadata-only inspection must not invent decoded fields: {line}",
            );
        }
    }
    for invented in ["h.264", "h264", "hevc", "1920x1080", "00:01:30"] {
        assert!(
            !lower.contains(invented),
            "fabricated decoded media claim: {invented}"
        );
    }
}

#[test]
fn mkv_kind_and_inspector_explain_video_identity_and_unavailable_playback() {
    let mut failures = Vec::new();
    for extension in ["mkv", "MKV", "MkV"] {
        let entry = mkv(extension);
        let original = entry.clone();
        let kind = folder_scene::kind(&entry);
        let metadata = folder_scene::metadata(&entry);
        let identity = kind.to_ascii_lowercase();
        // Exact extension remains in NAME; the kind must explain it in ordinary
        // language, rather than letting a raw suffix stand in for a kind label.
        if !identity.contains("video") || !identity.contains("mkv") {
            failures.push(format!(
                "{extension}: kind lacks MKV/video identity: {kind:?}"
            ));
        }
        let lower = metadata.to_ascii_lowercase();
        let unavailable = [
            "unsupported",
            "unavailable",
            "not available",
            "cannot play",
            "can't play",
        ];
        if !lower.contains("playback") || !unavailable.iter().any(|word| lower.contains(word)) {
            failures.push(format!(
                "{extension}: inspector lacks clear unavailable playback: {metadata:?}"
            ));
        }
        assert_no_fabricated_decoded_fields(&metadata);
        assert_eq!(entry, original);
        println!("MKV_INSPECTOR extension={extension} kind={kind:?} metadata={metadata:?}");
    }
    assert!(
        failures.is_empty(),
        "plain MKV inspector missing feature: {failures:?}"
    );
}

fn body(world: &hair::World, width: u32, height: u32, scroll: usize) -> Vec<[u32; 3]> {
    text::overlay(world, width, height, scroll)
        .iter()
        .filter(|v| v.color == [0.9, 0.96, 0.91])
        .map(|v| v.position.map(f32::to_bits))
        .collect()
}

#[test]
fn metadata_keeps_exact_escaped_identity_and_full_path_is_inspectable_in_library_reader() {
    let entry = mkv("MKV");
    let original = entry.clone();
    let metadata = folder_scene::metadata(&entry);
    let escaped_name = folder_scene::display(&entry.name);
    let escaped_path = folder_scene::display(entry.path.as_os_str());
    assert_eq!(escaped_name, r#""MiXeD\nClip-\u{c9}-NameTail.MKV""#);
    let expected_path = format!(
        "\"/Fixture{}/Parent\\t\\u{{c9}}-PathEnd/MiXeD\\nClip-\\u{{c9}}-NameTail.MKV\"",
        "/SegmentMiXeD".repeat(20),
    );
    assert_eq!(escaped_path, expected_path);
    assert!(metadata.contains(&format!("\n{escaped_name}\n")));
    assert!(metadata.contains(&format!("\n{escaped_path}\n")));
    assert!(
        !metadata.contains('~'),
        "exact metadata is not a truncated HUD preview"
    );
    assert!(metadata.contains("No writes, execution, default-open or link following."));
    assert_no_fabricated_decoded_fields(&metadata);
    // Existing non-file classification must not be inferred from a .MKV suffix.
    for kind in [Kind::Folder, Kind::Symlink, Kind::Special] {
        let mut control = entry.clone();
        control.kind = kind;
        assert_eq!(folder_scene::kind(&control), kind.label());
    }
    let snapshot = Snapshot {
        folder: entry.path.parent().unwrap().to_path_buf(),
        entries: vec![entry.clone()],
    };
    let (mut world, _) = folder_scene::build(&snapshot, 0, false).unwrap();
    let source = world.current.clone();
    let navigation = world.navigation.clone();
    let binding = world.folder_view.as_ref().unwrap().bindings.clone();
    // Deliberately direct library setup. App::local_interact calls metadata and
    // installs it into this reader, but this does NOT drive or qualify E.
    world.folder_view.as_mut().unwrap().reader = metadata.clone();
    world.reading = true;
    let alternate_path = escaped_path.replace("PathEnd", "ZZZZZZZ");
    let control_document = metadata.replace(&escaped_path, &alternate_path);
    assert_ne!(control_document, metadata);
    for (width, height) in [(960, 640), (640, 480)] {
        for collapsed in [false, true] {
            world.tree_collapsed = collapsed;
            let last = text::local_reader_scroll_max(&world, width, height);
            let mut path_tail_visible = false;
            for scroll in 0..=last {
                let actual = body(&world, width, height, scroll);
                world.folder_view.as_mut().unwrap().reader = control_document.clone();
                let control = body(&world, width, height, scroll);
                world.folder_view.as_mut().unwrap().reader = metadata.clone();
                if actual != control {
                    assert!(
                        !actual.is_empty(),
                        "path-tail positive control has no body ink"
                    );
                    path_tail_visible = true;
                    break;
                }
            }
            assert!(
                path_tail_visible,
                "escaped path tail not inspectable at {width}x{height} collapsed={collapsed}"
            );
            assert_eq!(world.document().unwrap(), metadata);
            assert_eq!(world.current, source);
            assert_eq!(world.navigation, navigation);
            assert_eq!(world.folder_view.as_ref().unwrap().bindings, binding);
        }
    }
    assert_eq!(entry, original);
}
