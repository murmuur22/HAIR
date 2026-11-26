use std::{fs, path::PathBuf, process::Command};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn proof_output(input: &std::path::Path, output: &std::path::Path) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hair"))
        .arg("--folder")
        .arg(input)
        .arg("--folder-flow-proof")
        .arg("--smoke-output")
        .arg(output)
        .env("HAIR_RESOURCE_ROOT", env!("CARGO_MANIFEST_DIR"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(45) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native proof exceeded 45s; owned child killed and reaped")
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}
#[test]
fn capture_output_refuses_chosen_folder_descendant_ancestor_and_alias() {
    let f =
        Fixture(std::env::temp_dir().join(format!("hair-folder-overlap-{}", std::process::id())));
    fs::create_dir(&f.0).unwrap();
    let input = f.0.join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("Original.txt"), b"do not alter").unwrap();
    let child = input.join("nested");
    fs::create_dir(&child).unwrap();
    let mut outputs = vec![input.clone(), child, f.0.clone()];
    #[cfg(unix)]
    {
        let alias = f.0.join("input-alias");
        std::os::unix::fs::symlink(&input, &alias).unwrap();
        outputs.push(alias);
    }
    for output in outputs {
        let o = proof_output(&input, &output);
        let stderr = String::from_utf8_lossy(&o.stderr);
        let stdout = String::from_utf8_lossy(&o.stdout);
        assert!(!o.status.success());
        assert!(
            stderr.contains("separate from selected content/pack roots"),
            "must refuse selected folder overlap before rendering: {stderr}"
        );
        assert!(!stdout.contains("SMOKE_OUTPUT="));
        assert!(!output.join("office.png").exists());
        assert!(!output.join("reader-workshop.png").exists());
        assert_eq!(
            fs::read(input.join("Original.txt")).unwrap(),
            b"do not alter"
        );
    }
}
#[test]
fn unchanged_native_executable_can_explore_a_folder_via_its_real_adapter_and_renderer() {
    let f =
        Fixture(std::env::temp_dir().join(format!("hair-folder-native-{}", std::process::id())));
    fs::create_dir(&f.0).unwrap();
    let input = f.0.join("input");
    let output = f.0.join("captures");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::create_dir(input.join("Child")).unwrap();
    fs::write(
        input.join("Read me.txt"),
        b"LOCAL FILE ORIGINAL\nSecond line\n",
    )
    .unwrap();
    // More than the old eight physical station slots: every entry must remain accessible.
    for i in 0..75 {
        fs::write(input.join(format!("File {i:03}.bin")), []).unwrap();
    }
    let before = fs::read(input.join("Read me.txt")).unwrap();
    let o = proof_output(&input, &output);
    assert_eq!(
        fs::read(input.join("Read me.txt")).unwrap(),
        before,
        "input bytes must remain unchanged even on diagnostic failure"
    );
    let text = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    for line in text
        .lines()
        .filter(|line| line.starts_with("INVENTORY_POINTER_") || line.starts_with("GPU_ADAPTER="))
    {
        println!("{line}");
    }
    assert!(
        o.status.success(),
        "native folder proof should execute, status={:?}, stdout={text}, stderr={err}",
        o.status.code()
    );
    assert!(
        text.contains("INVENTORY_POINTER_APP_OK"),
        "actual collected-item pointer/key/focus adapter and changed Metal PNG must pass: {text}"
    );
    assert!(
        text.contains("DIRECTORY_MAP_CONFIRMED_TRAVEL_OK"),
        "packaged proof must exercise explicit map confirmation: {text}"
    );
    assert!(
        text.contains("LOCAL_FOLDER_FLOW_OK"),
        "not a baseline browser/component-only proof: {text}"
    );
    assert!(
        text.contains("NATIVE_ADAPTER_SYNTHETIC"),
        "must label injected keys separately from real human input: {text}"
    );
    assert!(
        output.join("office.png").is_file(),
        "actual closed-reader capture missing"
    );
    assert!(
        output.join("reader-workshop.png").is_file(),
        "actual selected-file reader capture missing"
    );
}
