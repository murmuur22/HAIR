//! Source App + authentic extracted 0.4.3 R2C resources, NOT a new packaged
//! native-input probe. Never execute or modify the historical bundle.
use super::*;

fn metadata(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u64>> {
    fn walk(path: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u64>>) {
        let m = std::fs::symlink_metadata(path).unwrap();
        assert!(!m.file_type().is_symlink(), "unexpected bundle symlink: {}", path.display());
        out.insert(path.to_path_buf(), vec![m.dev(), m.ino(), u64::from(m.mode()), m.nlink(),
            u64::from(m.uid()), u64::from(m.gid()), m.len(), m.mtime() as u64,
            m.mtime_nsec() as u64, m.ctime() as u64, m.ctime_nsec() as u64,
            u64::from(std::os::macos::fs::MetadataExt::st_flags(&m))]);
        if m.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() { walk(&entry.unwrap().path(), out); }
        }
    }
    let mut out = Default::default(); walk(root, &mut out); out
}

#[test]
#[ignore = "explicit read-only historical extracted Resources probe; HAIR_DECORATION_B_EXTRACTED_ROOT required"]
fn decoration_b_source_app_extracted_resources_catalog_publication() {
    let extracted = PathBuf::from(std::env::var_os("HAIR_DECORATION_B_EXTRACTED_ROOT").expect("explicit historical extraction parent"));
    assert!(extracted.is_absolute());
    for (name, executable) in [
        ("HAIR Place Visitor Editor R2C Private.app", "hair"),
        ("HAIR Appearance Workshop Editor R2C Private.app", "hair-workshop"),
    ] {
        let bundle = extracted.join(name);
        let before_bytes = slice4c::records(&bundle);
        let before_metadata = metadata(&bundle);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let root = hair::resources::resolve_root(&bundle.join("Contents/MacOS").join(executable), None).unwrap();
            assert_eq!(root, bundle.join("Contents/Resources").canonicalize().unwrap());
            assert!(root.is_absolute());
            let owned = fixture("decoration-b-extracted-resources");
            let room = owned.0.join("room"); std::fs::create_dir(&room).unwrap();
            std::fs::create_dir(owned.0.join("preferences")).unwrap();
            let world = World::load(&root).unwrap();
            let host = hair::host_scene::HostScene::load(&root).unwrap();
            let mut cache = Default::default();
            let themes = ["packs/office.toml", "packs/archive.toml"].into_iter()
                .map(|name| hair::theme::Theme::load_for_scene(&root.join(name), &mut cache, &host).unwrap()).collect();
            let mut a = App::new(root.clone(), themes, 0, world, false);
            a.restore_preferences_at(&owned.0.join("preferences")).unwrap();
            a.complete_folder(Ok(Some(room.clone()))).unwrap();
            let untouched = slice4c::records(&room);
            // Real catalog Enter is the mandatory checked acquisition boundary.
            select(&mut a);
            let selected = a.themes[0].decorations.entries[0].clone();
            let original = a.themes[0].decorations.asset(&selected.asset).unwrap().clone();
            let raw = std::fs::read(root.join("packs").join(&original.spec.file)).unwrap();
            let proposed = a.decoration_position().unwrap();
            assert!(proposed.is_pinned());
            assert_eq!(slice4c::records(&room), untouched, "acquisition is write-free");
            enter_positioning(&mut a); aim_floor(&mut a, [-2., 0.]);
            tap(&mut a, KeyCode::Enter);
            assert_eq!(a.editor_phase(), "Review", "{}", a.local_status);
            assert_eq!(slice4c::records(&room), untouched, "review is write-free");
            let proposed = a.decoration_position().unwrap();
            tap(&mut a, KeyCode::Enter);
            assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
            let saved = hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap();
            assert_eq!(saved.decorations, vec![proposed.clone()]);
            assert_eq!(saved.incomplete_records, 0); assert!(saved.warnings.is_empty());
            let empty = hair::decoration::Catalog::default();
            let (entry, retained) = saved.decorations[0].resolve(&empty).unwrap();
            assert_eq!(entry, &selected);
            let mut spec = original.spec.clone(); spec.file = "asset.glb".into();
            assert_eq!(retained.spec, spec);
            assert_eq!(bytes(&retained.model.vertices), bytes(&original.model.vertices));
            assert_eq!(retained.model.textures.len(), original.model.textures.len());
            for (held, source) in retained.model.textures.iter().zip(&original.model.textures) {
                assert_eq!((held.width, held.height, &held.rgba), (source.width, source.height, &source.rgba));
            }
            let payload = room.join(format!(".hair-layout/ac-{}/asset.glb", proposed.closure_identity().unwrap()));
            assert_eq!(std::fs::read(payload).unwrap(), raw);
            let installed = &a.themes[0].host.decorations[&proposed.placement_id];
            assert_eq!(installed, &proposed);
            let preview = a.themes[0].decoration_preview(installed).unwrap();
            assert!(preview.len() > 36);
            let scene = a.themes[0].vertices_for_at(&a.world, 0.);
            assert!(scene.windows(preview.len()).any(|w| bytes(w) == bytes(&preview)), "actual scene must consume acquired retained art");
            let published = slice4c::records(&room);
            tap(&mut a, KeyCode::F5);
            assert_eq!(hair::placement_store::read_layout(&a.local.as_ref().unwrap().explorer).unwrap(), saved);
            assert_eq!(slice4c::records(&room), published);
            assert_eq!(bytes(&a.themes[0].decoration_preview(&proposed).unwrap()), bytes(&preview));
            println!("SOURCE_APP_EXTRACTED_RESOURCES_PASS bundle={} root={} entry={} asset={} exact_payload=true installed_retained_art=true; NOT new packaged/native input", bundle.display(), root.display(), selected.id, original.spec.id);
        }));
        // Also check immutable bundle bytes/inodes/stamps on an acquisition refusal.
        assert_eq!(slice4c::records(&bundle), before_bytes, "historical bundle byte/inode mutation");
        assert_eq!(metadata(&bundle), before_metadata, "historical bundle metadata mutation");
        println!("EXTRACTED_BUNDLE_UNCHANGED {}", bundle.display());
        if let Err(panic) = result { std::panic::resume_unwind(panic); }
    }
}
