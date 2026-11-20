#![cfg(feature = "workshop")]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[test]
#[ignore = "real offscreen current executable, synthetic egui outcomes and core writers; no OS panel"]
fn executable_save_export_errors_and_recovery_are_visible_and_non_destructive() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-clarity-gpu-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let input = base.join("Downloads");
    fs::create_dir(&input).unwrap();
    fs::copy(
        root.join("milestone-workshop/package/starter/model.glb"),
        input.join("sample.glb"),
    )
    .unwrap();
    let captures = base.join("captures");
    fs::create_dir(&captures).unwrap();
    let binary = std::env::var_os("CLARITY_WORKSHOP")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_hair-workshop").into());
    let out = Command::new(&binary)
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--offscreen-clarity-proof")
        .arg(input.join("sample.glb"))
        .arg(base.join("HAIR Project"))
        .arg(base.join("HAIR Appearance Pack"))
        .arg(&captures)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{log}");
    fs::write(base.join("process.log"), &log).unwrap();
    assert!(out.status.success(), "{log}");
    assert!(
        !log.contains("WORKSHOP_OFFSCREEN_PASS actions="),
        "clarity mode must not claim it opened a project"
    );
    for token in [
        "WORKSHOP_CLARITY_PASS",
        "Saving editable project",
        "Exporting appearance pack",
        "cancel_preserved=true",
        "retry_writer=true",
        "dirty_after_export=true",
    ] {
        assert!(log.contains(token), "missing {token}: {log}");
    }
    for name in ["office.png", "reader-welcome.png", "reader-workshop.png"] {
        let path = captures.join(name);
        let mut r = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut pixels).unwrap();
        assert!(
            pixels
                .chunks(4)
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 100
        );
    }
    let saved = hair::authoring::Project::open(&base.join("HAIR Project/project.toml")).unwrap();
    let theme = hair::theme::Theme::load(
        &base.join("HAIR Appearance Pack/pack.toml"),
        &mut Default::default(),
    )
    .unwrap();
    assert_eq!(saved.yaw_degrees, 15.0);
    assert!(!theme.vertices.is_empty());
    assert_eq!(
        fs::read(input.join("sample.glb")).unwrap(),
        fs::read(base.join("HAIR Appearance Pack/model.glb")).unwrap()
    );
    println!(
        "CLARITY_CAPTURES={} binary={} native=false human=false",
        base.display(),
        binary.display()
    );
}
