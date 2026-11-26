// Disposable owned fixtures only. Production decoder/loader and real bundled art.
fn slice6a_pack(f: &Fixture) -> (PathBuf, crate::theme::Theme) {
    let pack = f.0.join("pack");
    std::fs::create_dir(&pack).unwrap();
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/chairDesk.glb")).unwrap();
    std::fs::write(pack.join("chair.glb"), bytes).unwrap();
    std::fs::write(pack.join("pack.toml"), "version=3\nname='Owned tracer'\nroles=[]\n[decorations]\n[[decorations.assets]]\nid='chair-mesh'\nfile='chair.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n[[decorations.entries]]\nid='chair'\ntag='hair.seating.chair'\nasset='chair-mesh'\nname='Real chair'\n").unwrap();
    let theme = crate::theme::Theme::load(&pack.join("pack.toml"), &mut Default::default()).unwrap();
    (pack, theme)
}
#[test]
fn slice6a_authentic_pin_survives_pack_removal_theme_switch_and_cold_process() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let (pack, theme) = slice6a_pack(&f);
    let e = Explorer::new(&f.0).unwrap();
    assert_eq!(publish(&e, OsStr::new("saved.txt"), b"unchanged owned file", &pos(), &AtomicBool::new(false)), Outcome::Committed { synced: true });
    let saved_ino = std::fs::metadata(f.0.join("saved.txt")).unwrap().ino();
    let entry = theme.decorations.entry("chair").unwrap();
    let d = Decoration::from_entry(entry, [-2., 0.], 0.2, [0.8; 3]).unwrap();
    let other = slice5a_decoration([2., 0.]);
    assert_eq!(place_decoration_checked(&e, &other, |_| Ok(())), Outcome::Committed { synced: true });
    let other_path = slice5a_record(&f, &other);
    let other_bytes = std::fs::read(&other_path).unwrap();
    let other_ino = std::fs::metadata(&other_path).unwrap().ino();
    let source = std::fs::read(pack.join("chair.glb")).unwrap();
    let source_ino = std::fs::metadata(pack.join("chair.glb")).unwrap().ino();
    let manifest = std::fs::read(pack.join("pack.toml")).unwrap();
    let model = theme.decorations.asset(&entry.asset).unwrap().model.clone();
    let out = pin_decoration_checked(&e, &theme, "chair", &d, |live| {
        crate::placement_scene::validate_decoration_candidate(&e, live, [0., 2.], 0)
    });
    assert_eq!(out, Outcome::Committed { synced: true }, "authentic owned pin must publish, not a tag-only placeholder");
    assert_eq!(std::fs::read(pack.join("chair.glb")).unwrap(), source);
    assert_eq!(std::fs::metadata(pack.join("chair.glb")).unwrap().ino(), source_ino);
    assert_eq!(std::fs::read(pack.join("pack.toml")).unwrap(), manifest);
    // Remove ONLY the disposable fixture pack, never the bundled original.
    std::fs::remove_dir_all(&pack).unwrap();
    let reopened = read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
    assert_eq!(reopened.positions, vec![("saved.txt".into(), pos())]);
    assert_eq!(std::fs::read(f.0.join("saved.txt")).unwrap(), b"unchanged owned file");
    assert_eq!(std::fs::metadata(f.0.join("saved.txt")).unwrap().ino(), saved_ino);
    let pinned = reopened.decorations.iter().find(|x| x.placement_id == d.placement_id).unwrap();
    assert_eq!(pinned.tag, d.tag);
    assert_eq!((pinned.at, pinned.yaw, pinned.size), (d.at, d.yaw, d.size));
    assert_eq!(pinned.bounds().unwrap(), d.bounds().unwrap());
    assert_eq!(pinned.collision, d.collision);
    let empty = crate::decoration::Catalog::default();
    let (retained_entry, retained) = pinned.resolve(&empty).unwrap();
    assert_eq!(retained_entry, entry);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&retained.model.vertices), bytemuck::cast_slice::<_, u8>(&model.vertices));
    let closure = std::fs::read_dir(f.0.join(".hair-layout")).unwrap().map(|x| x.unwrap().path())
        .find(|p| p.file_name().unwrap().to_str().unwrap().starts_with("ac-")).unwrap();
    assert_eq!(std::fs::read(closure.join("asset.glb")).unwrap(), source);
    let descriptor: super::pinned::Descriptor = toml::from_str(&std::fs::read_to_string(closure.join("closure.toml")).unwrap()).unwrap();
    assert_eq!(descriptor.sha256, super::pinned::digest(&source));
    assert_eq!(descriptor.source_manifest, super::pinned::digest(&manifest));
    assert_eq!(descriptor.asset.id, entry.asset);
    assert_eq!(descriptor.asset.file, "asset.glb");
    println!("SLICE6A authentic closure files=1 bytes={} vertices={} textures={} sha256={} source_manifest={}", source.len(), retained.model.vertices.len(), retained.model.textures.len(), descriptor.sha256, descriptor.source_manifest);
    // Actual switched Theme uses a different authentic mesh under the same tag.
    let switched_fixture = Fixture::new();
    let (switched_pack, _) = slice6a_pack(&switched_fixture);
    std::fs::write(switched_pack.join("chair.glb"), std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/desk.glb")).unwrap()).unwrap();
    let switched = crate::theme::Theme::load(&switched_pack.join("pack.toml"), &mut Default::default()).unwrap();
    assert_eq!(pinned.resolve(&switched.decorations).unwrap().1.encoded.as_ref(), source.as_slice());
    assert_ne!(other.resolve(&switched.decorations).unwrap().1.encoded.as_ref(), source.as_slice());
    let (world, mut host) = crate::folder_scene::build(e.current(), 0, true).unwrap();
    assert!(crate::placement_scene::apply(&reopened, &world, &mut host).unwrap().is_empty());
    let rendered = crate::theme::Theme::load_for_scene(&switched_pack.join("pack.toml"), &mut Default::default(), &host).unwrap();
    rendered.validate_decorations().unwrap();
    assert_eq!(bytemuck::cast_slice::<_, u8>(&rendered.decoration_preview(pinned).unwrap()), bytemuck::cast_slice::<_, u8>(&retained.placed(pinned).unwrap()));
    assert_eq!(std::fs::read(other_path.clone()).unwrap(), other_bytes);
    assert_eq!(std::fs::metadata(other_path).unwrap().ino(), other_ino);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "placement_store::tests::slice6a_cold_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE6A_ROOT", &f.0).output().unwrap();
    assert!(child.status.success(), "{}{}", String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
}
#[test]
#[ignore = "fresh-process slice6a fixture reader"]
fn slice6a_cold_child() {
    let path = PathBuf::from(std::env::var_os("HAIR_SLICE6A_ROOT").unwrap());
    assert!(path.starts_with(std::env::temp_dir()));
    assert!(!path.join("pack").exists());
    let layout = read_layout(&Explorer::new(&path).unwrap()).unwrap();
    let empty = crate::decoration::Catalog::default();
    let resolved: Vec<_> = layout.decorations.iter().filter_map(|d| d.resolve(&empty).ok()).collect();
    assert_eq!(resolved.len(), 1);
    assert!(!resolved[0].1.model.vertices.is_empty());
    let authentic = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/chairDesk.glb")).unwrap();
    assert_eq!(resolved[0].1.encoded.as_ref(), authentic.as_slice());
    let expected = crate::assets::decode(&authentic, 3.).unwrap();
    assert_eq!(bytemuck::cast_slice::<_, u8>(&resolved[0].1.model.vertices), bytemuck::cast_slice::<_, u8>(&expected.vertices));
    assert_eq!(resolved[0].1.model.textures.len(), expected.textures.len());
    for (a, b) in resolved[0].1.model.textures.iter().zip(&expected.textures) {
        assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba));
    }
    assert_eq!(layout.incomplete_records, 0);
    assert!(layout.warnings.is_empty());
}
