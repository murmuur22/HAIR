#![cfg(feature = "workshop")]
use std::{fs, path::Path, process::Command};
fn pixels(p: &Path) -> Vec<u8> {
    let mut r = png::Decoder::new(std::io::BufReader::new(fs::File::open(p).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    r.next_frame(&mut b).unwrap();
    b
}
#[test]
#[ignore = "finite real GPU + synthetic egui actions + actual paired executables; no private input/native window"]
fn synthetic_workshop_v2_export_and_shared_host_visitor() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = std::env::temp_dir().join(format!("hair-shared-pair-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    let workshop = std::env::var("SHARED_PROOF_WORKSHOP")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_hair-workshop").into());
    let visitor =
        std::env::var("SHARED_PROOF_VISITOR").unwrap_or_else(|_| env!("CARGO_BIN_EXE_hair").into());
    let model = root.join("milestone-workshop/package/starter/model.glb");
    let original = fs::read(&model).unwrap();
    let captures = base.join("workshop");
    fs::create_dir(&captures).unwrap();
    let out = Command::new(&workshop)
        .env("HAIR_RESOURCE_ROOT", root)
        .arg("--offscreen-proof")
        .arg(&model)
        .arg(base.join("project"))
        .arg(base.join("pack"))
        .arg(&captures)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(base.join("workshop.log"), &log).unwrap();
    println!("{log}");
    assert!(out.status.success(), "{log}");
    assert!(log.contains("WORKSHOP_OFFSCREEN_PASS"));
    let manifest = fs::read_to_string(base.join("pack/pack.toml")).unwrap();
    let v: toml::Value = toml::from_str(&manifest).unwrap();
    assert_eq!(v["version"].as_integer(), Some(2));
    assert_eq!(v["roles"][0]["role"].as_str(), Some("document"));
    assert_eq!(
        v["roles"][0]["yaw_degrees"]
            .as_float()
            .or_else(|| v["roles"][0]["yaw_degrees"].as_integer().map(|n| n as f64)),
        Some(15.0)
    );
    for key in [
        "room", "spawn", "portal", "document", "slots", "solid", "at",
    ] {
        assert!(v.get(key).is_none(), "layout in export: {key}")
    }
    let p = hair::authoring::Project::open(&base.join("project/project.toml")).unwrap();
    assert!(p.document_role);
    assert_eq!(p.yaw_degrees, 15.0);
    assert_eq!(p.at, [0.0; 3]);
    assert!(!p.solid);
    assert_eq!(fs::read(base.join("pack/model.glb")).unwrap(), original);
    let site = base.join("site");
    fs::create_dir(&site).unwrap();
    fs::write(
        site.join("index.md"),
        "# Synthetic shared site\n[next](next.md)\n![Original swatch](swatch.png)\n",
    )
    .unwrap();
    fs::write(
        site.join("next.md"),
        "# Shared next document\n[entry](index.md)\n![Original swatch](swatch.png)\n",
    )
    .unwrap();
    fs::copy(
        root.join("examples/site-a/swatch.png"),
        site.join("swatch.png"),
    )
    .unwrap();
    let host = fs::read_to_string(root.join("host-scene.toml"))
        .unwrap()
        .replace("Shared demo room", "Synthetic shared host")
        .replace("at = [0.0, 0.1, 0.3]", "at = [0.6, 0.1, 0.3]")
        .replace(
            "geometry = []",
            "geometry = [{at=[-2.5,0.1,-1.5],size=[0.6,1.0,0.6]}]",
        );
    fs::write(site.join("host-scene.toml"), &host).unwrap();
    for custom in [false, true] {
        let dest = base.join(if custom {
            "visitor-export"
        } else {
            "visitor-builtins"
        });
        fs::create_dir(&dest).unwrap();
        let mut cmd = Command::new(&visitor);
        cmd.env("HAIR_RESOURCE_ROOT", root).arg("--site").arg(&site);
        if custom {
            cmd.arg("--pack").arg(base.join("pack/pack.toml"));
        }
        let out = cmd
            .args(["--headless-smoke", "--smoke-output"])
            .arg(&dest)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        fs::write(dest.join("process.log"), &log).unwrap();
        println!("{log}");
        assert!(out.status.success(), "{log}");
        assert!(!log.contains("PACK_FALLBACK"));
        assert!(log.contains("HOST_SCENE=Synthetic shared host"));
        assert!(
            log.contains(
                "SHARED_RESKIN_PRESERVED pose=true scene=true anchors=true collision=true"
            )
        );
        assert!(log.contains("THEME_SWITCH_PRESERVED page=next.md"));
        assert!(log.contains("Shared next document"));
        assert!(log.contains("reading=true"));
        assert!(log.contains(if custom {
            "THEME=Workshop object"
        } else {
            "THEME=office"
        }));
        let a = pixels(&dest.join("office.png"));
        let b = pixels(&dest.join("reader-workshop.png"));
        let changed = a.chunks(4).zip(b.chunks(4)).filter(|(a, b)| a != b).count();
        assert!(changed > 100, "skins must change actual pixels");
        println!("SHARED_APPEARANCE_CHANGED_PIXELS custom={custom} pixels={changed}");
    }
    assert_eq!(
        fs::read(site.join("host-scene.toml")).unwrap(),
        host.as_bytes()
    );
    assert_eq!(fs::read(&model).unwrap(), original);
    println!(
        "SHARED_PAIRED_PROOF={} workshop={workshop} visitor={visitor} native=false human=false",
        base.display()
    );
}
