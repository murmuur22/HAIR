// Offline Apple system compiler only. No crate download or native source-path API.
fn main() {
    println!("cargo:rerun-if-changed=native/audio_pcm.c");
    println!("cargo:rerun-if-changed=native/video_still.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let object = out.join("hair_audio_pcm.o");
    let library = out.join("libhair_audio_pcm.a");
    let status = std::process::Command::new("/usr/bin/clang")
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-O2",
            "-isysroot",
            "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk",
            "-c",
            "native/audio_pcm.c",
            "-o",
        ])
        .arg(&object)
        .status()
        .expect("fixed Apple clang unavailable");
    assert!(status.success(), "audio PCM native compile failed");
    let status = std::process::Command::new("/usr/bin/ar")
        .arg("crs")
        .arg(&library)
        .arg(&object)
        .status()
        .expect("fixed Apple ar unavailable");
    assert!(status.success(), "audio PCM archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=hair_audio_pcm");
    println!("cargo:rustc-link-lib=framework=AudioToolbox");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    let object = out.join("hair_video_still.o");
    let library = out.join("libhair_video_still.a");
    let status = std::process::Command::new("/usr/bin/clang")
        .args([
            "-std=gnu11",
            "-fobjc-arc",
            "-fblocks",
            "-fno-modules",
            "-mmacosx-version-min=13.0",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-O2",
            "-isysroot",
            "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk",
            "-c",
            "native/video_still.m",
            "-o",
        ])
        .arg(&object)
        .status()
        .expect("fixed Apple clang unavailable");
    assert!(
        status.success(),
        "held-byte video still native compile failed"
    );
    let status = std::process::Command::new("/usr/bin/ar")
        .arg("crs")
        .arg(&library)
        .arg(&object)
        .status()
        .expect("fixed Apple ar unavailable");
    assert!(status.success(), "held-byte video still archive failed");
    println!("cargo:rustc-link-lib=static=hair_video_still");
    for framework in ["Foundation", "AVFoundation", "CoreMedia", "CoreGraphics"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
