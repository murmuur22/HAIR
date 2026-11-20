use hair::{Pack, World, assets, safe_path};
use std::path::Path;
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
#[test]
fn graph_rejects_duplicate_dangling_unsafe_and_wrong_binding_data() {
    let graph = include_str!("../content/graph.toml");
    let plan = include_str!("../scene.toml");
    for bad in [
        graph.replace("id = \"workshop\"", "id = \"welcome\""),
        graph.replace("next = \"workshop\"", "next = \"missing\""),
        graph.replace("welcome.md", "../BRIEF.md"),
        graph.replace("welcome.md", "/etc/passwd"),
        graph.replace("version = 1", "version = 0"),
        graph.replace("welcome.md", "absent.md"),
    ] {
        assert!(World::parse(root(), &bad, plan).is_err(), "accepted {bad}");
    }
    for bad in [
        plan.replace("destination = \"workshop\"", "destination = \"welcome\""),
        plan.replace("document = \"welcome\"", "document = \"absent\""),
        plan.replace("document_role = \"desk\"", "document_role = \"script\""),
        plan.replace("grid = 0.5", "grid = nan"),
    ] {
        assert!(World::parse(root(), graph, &bad).is_err(), "accepted {bad}");
    }
}
#[test]
fn duplicate_roles_unknown_fields_nonfinite_and_oversize_constraints_rejected() {
    let pack = include_str!("../assets/kenney/pack.toml");
    assert!(Pack::parse(pack).is_ok());
    for bad in [
        pack.replace("role = \"chair\"", "role = \"desk\""),
        pack.replace("max_extent = 2.0", "max_extent = nan"),
        pack.replace("max_extent = 2.0", "max_extent = inf"),
        pack.replace("[3, 2]", "[99, 2]"),
        format!("{pack}\nscript = 'anything'\n"),
    ] {
        assert!(Pack::parse(&bad).is_err(), "accepted {bad}");
    }
}
#[test]
fn corrupt_truncated_and_nonfinite_glb_rejected() {
    let original = std::fs::read(root().join("assets/kenney/desk.glb")).unwrap();
    for length in [0, 12, 20, original.len() - 1] {
        assert!(assets::decode(&original[..length], 2.0).is_err());
    }
    let mut bytes = original.clone();
    let json_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let first_position = 20 + json_size + 8;
    bytes[first_position..first_position + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(assets::decode(&bytes, 2.0).is_err());
    let mut json=br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"../../escape.bin","byteLength":4}],"scenes":[{"nodes":[]}],"scene":0}"#.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut external = Vec::new();
    for n in [
        0x46546c67_u32,
        2,
        20 + json.len() as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        external.extend_from_slice(&n.to_le_bytes());
    }
    external.extend(json);
    let external_error = assets::decode(&external, 2.0).err().unwrap();
    assert!(
        external_error.contains("external resources"),
        "{external_error}"
    );
}
#[cfg(unix)]
#[test]
fn canonical_paths_reject_symlink_escape() {
    let folder = std::env::temp_dir().join(format!("hair-symlink-test-{}", std::process::id()));
    std::fs::create_dir(&folder).unwrap();
    std::os::unix::fs::symlink(root().join("BRIEF.md"), folder.join("escape.md")).unwrap();
    let result = safe_path(&folder, "escape.md");
    std::fs::remove_file(folder.join("escape.md")).unwrap();
    std::fs::remove_dir(folder).unwrap();
    assert!(result.is_err());
}
