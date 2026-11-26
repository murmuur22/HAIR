use std::{path::Path, process::Command};

#[test]
fn captures_reject_selected_site_and_pack_overlap_in_both_directions_and_aliases() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let p = std::env::temp_dir().join(format!("hair-source-boundary-{}", std::process::id()));
    std::fs::create_dir(&p).unwrap();
    let site = p.join("site");
    let pack = p.join("pack");
    for root in [&site, &pack] {
        std::fs::create_dir(root).unwrap();
        std::fs::create_dir(root.join("captures")).unwrap();
    }
    std::fs::write(site.join("index.md"), "# Read only\n").unwrap();
    fn copy_fixture_tree(source: &Path, destination: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let name = std::path::PathBuf::from(entry.file_name());
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                std::fs::create_dir(destination.join(&name)).unwrap();
                files.extend(
                    copy_fixture_tree(&entry.path(), &destination.join(&name))
                        .into_iter()
                        .map(|(path, bytes)| (name.join(path), bytes)),
                );
            } else {
                assert!(
                    kind.is_file(),
                    "pack fixture must contain only regular files/directories"
                );
                let bytes = std::fs::read(entry.path()).unwrap();
                std::fs::write(destination.join(&name), &bytes).unwrap();
                files.push((name, bytes));
            }
        }
        files
    }
    let pack_files = copy_fixture_tree(&project.join("packs"), &pack);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&site, p.join("site-alias")).unwrap();
        std::os::unix::fs::symlink(&pack, p.join("pack-alias")).unwrap();
    }
    for (flag, root, selection) in [
        ("--site", &site, site.clone()),
        ("--pack", &pack, pack.join("archive.toml")),
    ] {
        let mut outputs = vec![root.clone(), root.join("captures"), p.clone()];
        #[cfg(unix)]
        outputs.push(p.join(if flag == "--site" {
            "site-alias"
        } else {
            "pack-alias"
        }));
        for output in outputs {
            let result = Command::new(env!("CARGO_BIN_EXE_hair"))
                .env("HAIR_RESOURCE_ROOT", project)
                .arg(flag)
                .arg(&selection)
                .args(["--headless-smoke", "--smoke-output"])
                .arg(&output)
                .output()
                .unwrap();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(!result.status.success(), "{flag} {output:?}: {text}");
            assert!(
                text.contains("selected content/pack"),
                "{flag} {output:?}: {text}"
            );
            assert!(!text.contains("ADAPTER"), "reject before GPU: {text}");
            assert!(!output.join("office.png").exists());
        }
        #[cfg(unix)]
        {
            let alias = p.join(if flag == "--site" {
                "site-alias"
            } else {
                "pack-alias/archive.toml"
            });
            let result = Command::new(env!("CARGO_BIN_EXE_hair"))
                .env("HAIR_RESOURCE_ROOT", project)
                .arg(flag)
                .arg(alias)
                .args(["--headless-smoke", "--smoke-output"])
                .arg(root)
                .output()
                .unwrap();
            assert!(!result.status.success());
            let error = String::from_utf8_lossy(&result.stderr);
            let expected = if flag == "--site" {
                "selected site root symlink refused"
            } else {
                "selected content/pack"
            };
            assert!(error.contains(expected), "{error}");
            assert!(!String::from_utf8_lossy(&result.stdout).contains("ADAPTER"));
        }
    }
    assert_eq!(
        std::fs::read(site.join("index.md")).unwrap(),
        b"# Read only\n"
    );
    assert_eq!(std::fs::read_dir(site.join("captures")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(pack.join("captures")).unwrap().count(), 0);
    for (path, bytes) in pack_files {
        assert_eq!(
            std::fs::read(project.join("packs").join(&path)).unwrap(),
            bytes
        );
        assert_eq!(std::fs::read(pack.join(&path)).unwrap(), bytes);
    }
    std::fs::remove_dir_all(&p).unwrap();
}
