use hair::theme::Theme;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hair-slice4a-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        for name in ["chairDesk.glb", "desk.glb"] {
            std::fs::copy(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("packs")
                    .join(name),
                path.join(name),
            )
            .unwrap();
        }
        Self(path)
    }
    fn load(&self, text: &str) -> hair::Result<Theme> {
        std::fs::write(self.0.join("pack.toml"), text).unwrap();
        Theme::load(&self.0.join("pack.toml"), &mut Default::default())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn manifest() -> String {
    "version=3\nname='Real catalog'\nroles=[]\n\
     [decorations]\n\
     [[decorations.assets]]\nid='chair-mesh'\nfile='chairDesk.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n\
     [[decorations.assets]]\nid='desk-mesh'\nfile='desk.glb'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n\
     [[decorations.entries]]\nid='chair'\ntag='hair.seating.chair'\nasset='chair-mesh'\nname='Desk chair'\n\
     [[decorations.entries]]\nid='desk'\ntag='hair.furniture.desk'\nasset='desk-mesh'\nname='Writing desk'\n[decorations.entries.defaults]\ncollision=true\n".into()
}
#[test]
fn slice4a_real_catalog_loads_through_theme_parser() {
    let f = Fixture::new();
    let text = manifest();
    let result = f.load(&text);
    assert!(
        result.is_ok(),
        "actual loader refused catalog: {:?}",
        result.err()
    );
    assert_eq!(
        std::fs::read_to_string(f.0.join("pack.toml")).unwrap(),
        text
    );
}

#[test]
fn slice4a_identity_order_defaults_and_real_meshes_survive_shared_contract_roundtrip() {
    let f = Fixture::new();
    let original_chair = std::fs::read(f.0.join("chairDesk.glb")).unwrap();
    let mut value: toml::Value = toml::from_str(&manifest()).unwrap();
    let spec: hair::decoration::Spec = value["decorations"].clone().try_into().unwrap();
    let encoded = toml::to_string(&spec).unwrap();
    let decoded: hair::decoration::Spec = toml::from_str(&encoded).unwrap();
    decoded.validate().unwrap();
    assert_eq!(spec.entries, decoded.entries);
    value["decorations"]["entries"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let theme = f.load(&toml::to_string(&value).unwrap()).unwrap();
    assert_eq!(
        theme
            .decorations
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["chair", "desk"]
    );
    let chair = theme.decorations.entry("chair").unwrap();
    let desk = theme.decorations.entry("desk").unwrap();
    assert_ne!(chair.id, chair.tag);
    assert_ne!(chair.id, chair.asset);
    assert!(!chair.defaults.collision);
    assert!(desk.defaults.collision);
    let a = theme.decorations.asset(&chair.asset).unwrap();
    let b = theme.decorations.asset(&desk.asset).unwrap();
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&a.model.vertices),
        bytemuck::cast_slice::<_, u8>(&b.model.vertices)
    );
    assert_eq!(
        std::fs::read(f.0.join("chairDesk.glb")).unwrap(),
        original_chair
    );
}

#[test]
fn slice4a_duplicate_identity_missing_reference_and_unknown_fields_refuse_without_repair() {
    let f = Fixture::new();
    for (old, new) in [
        ("id='desk-mesh'", "id='chair-mesh'"),
        ("id='desk'", "id='chair'"),
        ("tag='hair.furniture.desk'", "tag='hair.seating.chair'"),
        ("asset='desk-mesh'", "asset='absent'"),
        ("id='chair'", "id='../chair'"),
        ("tag='hair.seating.chair'", "tag='https://invalid/chair'"),
        ("name='Desk chair'", "name=''"),
        ("collision=true", "collision=true\nfile_collision=true"),
        ("[decorations]", "[decorations]\nfuture=true"),
        (
            "file='chairDesk.glb'",
            "file='chairDesk.glb'\nplacement_id='forbidden'",
        ),
        ("name='Desk chair'", "name='Desk chair'\npinned=true"),
        ("version=3", "version=2"),
    ] {
        let text = manifest().replace(old, new);
        assert!(f.load(&text).is_err(), "unexpected acceptance: {new}");
        assert_eq!(
            std::fs::read_to_string(f.0.join("pack.toml")).unwrap(),
            text
        );
    }
}

#[test]
fn slice4a_catalog_paths_finite_fit_and_model_profile_are_strict_even_with_warm_cache() {
    let f = Fixture::new();
    for (old, new) in [
        ("file='chairDesk.glb'", "file='/machine/chair.glb'"),
        ("file='chairDesk.glb'", "file='https://invalid/chair.glb'"),
        ("file='chairDesk.glb'", "file='../chair.glb'"),
        ("file='chairDesk.glb'", "file='a/b/c/d/e/f/g/h/i.glb'"),
        ("file='chairDesk.glb'", "file='missing.glb'"),
        ("max_extent=3.0", "max_extent=nan"),
        ("max_extent=3.0", "max_extent=0.01"),
        ("max_extent=3.0", "max_extent=10.1"),
        ("scale=1.0", "scale=inf"),
        ("scale=1.0", "scale=0.0"),
        ("yaw_degrees=0.0", "yaw_degrees=181.0"),
        ("pivot=[0.0,0.0,0.0]", "pivot=[0.0,nan,0.0]"),
        ("pivot=[0.0,0.0,0.0]", "pivot=[0.0,3.1,0.0]"),
    ] {
        let mut cache = Default::default();
        f.load(&manifest()).unwrap();
        Theme::load(&f.0.join("pack.toml"), &mut cache).unwrap();
        std::fs::write(f.0.join("pack.toml"), manifest().replace(old, new)).unwrap();
        assert!(
            Theme::load(&f.0.join("pack.toml"), &mut cache).is_err(),
            "{new}"
        );
    }
    let bytes = std::fs::read(f.0.join("chairDesk.glb")).unwrap();
    std::fs::write(f.0.join("chairDesk.glb"), b"not a GLB").unwrap();
    assert!(f.load(&manifest()).is_err());
    assert_eq!(
        std::fs::read(f.0.join("chairDesk.glb")).unwrap(),
        b"not a GLB"
    );
    std::fs::write(f.0.join("chairDesk.glb"), &bytes).unwrap();
}

#[test]
fn slice4a_nofollow_leaf_ancestor_and_size_limits_use_existing_acquisition() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    symlink("chairDesk.glb", f.0.join("alias.glb")).unwrap();
    assert!(
        f.load(&manifest().replace("file='chairDesk.glb'", "file='alias.glb'"))
            .is_err()
    );
    symlink(".", f.0.join("alias-dir")).unwrap();
    assert!(
        f.load(&manifest().replace("file='chairDesk.glb'", "file='alias-dir/chairDesk.glb'"))
            .is_err()
    );
    std::fs::File::create(f.0.join("large.glb"))
        .unwrap()
        .set_len(8_000_001)
        .unwrap();
    assert!(
        f.load(&manifest().replace("file='chairDesk.glb'", "file='large.glb'"))
            .is_err()
    );
    assert!(f.load(&(manifest() + &" ".repeat(32769))).is_err());
}

#[test]
fn slice4a_catalog_count_identity_and_whole_pack_budgets_are_shared_with_roles() {
    let f = Fixture::new();
    let mut value: toml::Value = toml::from_str(&manifest()).unwrap();
    let entries = value["decorations"]["entries"].as_array_mut().unwrap();
    for i in 2..32 {
        let mut entry = entries[0].clone();
        entry["id"] = format!("choice-{i:02}").into();
        entry["tag"] = format!("hair.choice-{i:02}").into();
        entries.push(entry);
    }
    assert_eq!(
        f.load(&toml::to_string(&value).unwrap())
            .unwrap()
            .decorations
            .entries
            .len(),
        32
    );
    let mut extra = value["decorations"]["entries"][0].clone();
    extra["id"] = "overflow".into();
    extra["tag"] = "hair.overflow".into();
    value["decorations"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    assert!(f.load(&toml::to_string(&value).unwrap()).is_err());
    let mut text = manifest().replace("roles=[]\n", "");
    for file in ["wall.glb", "lampSquareCeiling.glb"] {
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("packs")
                .join(file),
            f.0.join(file),
        )
        .unwrap();
        text.push_str(&format!("\n[[roles]]\nrole='{file}'\nfile='{file}'\nmax_extent=3.0\nscale=1.0\nyaw_degrees=0.0\npivot=[0.0,0.0,0.0]\n"));
    }
    assert!(
        f.load(&text)
            .err()
            .unwrap()
            .contains("aggregate whole-pack budget")
    );
}

#[test]
fn slice4a_catalog_preserves_legacy_v2_v3_document_surfaces_sky_and_host_geometry() {
    let f = Fixture::new();
    let mut value: toml::Value = toml::from_str(&manifest()).unwrap();
    value.as_table_mut().unwrap().insert(
        "sky".into(),
        toml::from_str::<toml::Value>("kind='colour'\nrgb=[0.1,0.2,0.3]").unwrap(),
    );
    value.as_table_mut().unwrap().insert("surfaces".into(), toml::from_str::<toml::Value>("surfaces=[{surface='wall',tint=[0.2,0.3,0.4],meters_per_tile=1.0},{surface='floor',tint=[0.5,0.4,0.3],meters_per_tile=1.0}]").unwrap()["surfaces"].clone());
    value["roles"] = toml::from_str::<toml::Value>("roles=[{role='document',file='desk.glb',max_extent=3.0,scale=1.0,yaw_degrees=0.0,pivot=[0.0,0.0,0.0]}]").unwrap()["roles"].clone();
    let with = f.load(&toml::to_string(&value).unwrap()).unwrap();
    value.as_table_mut().unwrap().remove("decorations");
    let legacy = f.load(&toml::to_string(&value).unwrap()).unwrap();
    assert!(legacy.decorations.entries.is_empty());
    assert_eq!(with.host, legacy.host);
    assert_eq!(with.sky, legacy.sky);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&with.vertices),
        bytemuck::cast_slice::<_, u8>(&legacy.vertices)
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(with.inventory_document_shape()),
        bytemuck::cast_slice::<_, u8>(legacy.inventory_document_shape())
    );
    value.as_table_mut().unwrap().remove("sky");
    value.as_table_mut().unwrap().remove("surfaces");
    value["version"] = 2.into();
    assert!(
        f.load(&toml::to_string(&value).unwrap())
            .unwrap()
            .decorations
            .entries
            .is_empty()
    );
    assert!(
        f.load("version=3\nname='Empty'\nroles=[]\n[decorations]\nassets=[]\nentries=[]\n")
            .unwrap()
            .decorations
            .entries
            .is_empty()
    );
}

#[test]
fn slice4a_bundled_themes_have_distinct_real_catalogs_without_extra_model_loads() {
    for name in ["office", "archive"] {
        let mut cache = Default::default();
        let theme = Theme::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("packs/{name}.toml")),
            &mut cache,
        )
        .unwrap();
        assert_eq!(cache.loads, 3);
        assert_eq!(theme.decorations.entries.len(), 2);
        let a = theme
            .decorations
            .asset(&theme.decorations.entries[0].asset)
            .unwrap();
        let b = theme
            .decorations
            .asset(&theme.decorations.entries[1].asset)
            .unwrap();
        assert_ne!(
            bytemuck::cast_slice::<_, u8>(&a.model.vertices),
            bytemuck::cast_slice::<_, u8>(&b.model.vertices)
        );
    }
}
