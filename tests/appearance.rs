use hair::theme::{AssetCache, Theme};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hair-role-{label}-{}", std::process::id()));
    fs::create_dir(&p).unwrap();
    p
}
fn pack(role: &str, file: &str, scale: &str) -> String {
    format!(
        "version=2\nname='test'\n[[roles]]\nrole='{role}'\nfile='{file}'\nmax_extent=3.0\nscale={scale}\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n"
    )
}
#[test]
fn cached_models_cannot_bypass_relative_path_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let d = dir("cache-boundary");
    fs::copy(root.join("assets/kenney/desk.glb"), d.join("model.glb")).unwrap();
    let path = d.join("pack.toml");
    let mut cache = AssetCache::default();
    fs::write(&path, pack("document", "model.glb", "1.0")).unwrap();
    Theme::load(&path, &mut cache).unwrap();
    fs::write(
        &path,
        pack("document", d.join("model.glb").to_str().unwrap(), "1.0"),
    )
    .unwrap();
    let t = Theme::load(&path, &mut cache).unwrap();
    assert!(
        t.diagnostics
            .iter()
            .any(|d| d.contains("ROLE_FALLBACK document")),
        "cached absolute path escaped validation"
    );
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn role_fallback_fit_and_forbidden_layout_preserve_host() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let d = dir("contracts");
    let path = d.join("pack.toml");
    fs::copy(root.join("assets/kenney/desk.glb"), d.join("model.glb")).unwrap();
    let host = hair::host_scene::HostScene::shared_default();
    let mut cache = AssetCache::default();
    for text in [
        "version=2\nname='empty'\nroles=[]\n".to_owned(),
        pack("unsupported-role", "model.glb", "1.0"),
        pack("document", "absent.glb", "1.0"),
        pack("document", "model.glb", "nan"),
    ] {
        fs::write(&path, text).unwrap();
        let t = Theme::load_for_scene(&path, &mut cache, &host).unwrap();
        assert_eq!(t.host, host);
        assert!(
            t.diagnostics
                .iter()
                .any(|d| d.contains("ROLE_FALLBACK document"))
        );
        assert_eq!(
            t.document,
            [host.anchor("document").at[0], host.anchor("document").at[2]]
        );
    }
    let base = pack("document", "model.glb", "4.0");
    fs::write(&path, &base).unwrap();
    let t = Theme::load_for_scene(&path, &mut cache, &host).unwrap();
    assert!(
        t.diagnostics
            .iter()
            .any(|d| d.starts_with("ROLE_FIT document"))
    );
    let a = host.anchor("document");
    let model = hair::assets::load(&d.join("model.glb"), 3.0).unwrap();
    let begin = host.vertices().len();
    let fitted = &t.vertices[begin..begin + model.vertices.len()];
    for v in fitted {
        for i in 0..3 {
            let lo = if i == 1 {
                a.at[i]
            } else {
                a.at[i] - a.size[i] * 0.5
            };
            let hi = if i == 1 {
                a.at[i] + a.size[i]
            } else {
                a.at[i] + a.size[i] * 0.5
            };
            assert!(
                v.position[i] >= lo - 0.00001 && v.position[i] <= hi + 0.00001,
                "outside envelope: {:?}",
                v.position
            );
        }
    }
    let pair = model
        .vertices
        .iter()
        .position(|v| v.position != model.vertices[0].position)
        .unwrap();
    let source_delta = glam::Vec3::from_array(model.vertices[pair].position)
        - glam::Vec3::from_array(model.vertices[0].position);
    let fit_delta =
        glam::Vec3::from_array(fitted[pair].position) - glam::Vec3::from_array(fitted[0].position);
    let factor = fit_delta.length() / source_delta.length();
    for (v, src) in fitted.iter().zip(&model.vertices) {
        for i in 0..3 {
            assert!(
                ((v.position[i] - fitted[0].position[i])
                    - (src.position[i] - model.vertices[0].position[i]) * factor)
                    .abs()
                    < 0.0001,
                "nonuniform fit"
            );
        }
    }
    for key in [
        "room=[-4.0,-4.0,4.0,4.0]",
        "spawn=[0.0,0.0]",
        "document=[2.0,2.0]",
        "portal=[0.0,0.0]",
        "slots=[]",
        "solid=true",
        "at=[1.0,0.0,1.0]",
    ] {
        for text in [format!("{key}\n{base}"), format!("{base}\n{key}\n")] {
            fs::write(&path, text).unwrap();
            assert!(
                Theme::load(&path, &mut cache).is_err(),
                "accepted layout {key}"
            );
        }
    }
    fs::write(&path, "version=1\nname='old'\n").unwrap();
    let err = Theme::load(&path, &mut cache).err().unwrap();
    assert!(err.contains("legacy combined v1"));
    fs::remove_dir_all(d).unwrap();
}
