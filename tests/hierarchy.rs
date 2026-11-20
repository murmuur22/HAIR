use hair::assets;

fn glb(nodes: &str, scenes: &str, selected: &str, geometry: bool) -> Vec<u8> {
    let mut blob = Vec::new();
    let mesh = if geometry {
        for value in [
            0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, // positions
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, // normals
        ] {
            blob.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0_u16, 1, 2, 0] {
            blob.extend_from_slice(&index.to_le_bytes());
        }
        r#", "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36},{"buffer":0,"byteOffset":72,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2}]}]"#
    } else {
        blob.extend_from_slice(&[0; 4]);
        ""
    };
    let mut json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{}}}],"nodes":[{nodes}],"scenes":[{scenes}]{selected}{mesh}}}"#,
        blob.len()
    )
    .into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    for word in [
        0x46546c67_u32,
        2,
        (28 + json.len() + blob.len()) as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend(json);
    bytes.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x004e4942_u32.to_le_bytes());
    bytes.extend(blob);
    bytes
}

fn rejects_repeated_hierarchy(bytes: &[u8]) {
    let error = assets::decode(bytes, 2.0).err().expect("must reject");
    assert!(
        error.contains("repeated node in selected scene hierarchy"),
        "expected repeated hierarchy rejection, got: {error}"
    );
}

#[test]
fn depth8_repeated_children_rejected_as_hierarchy_not_empty_geometry() {
    // Fast on the unfixed decoder. Keep depth24 out of unbounded cargo tests.
    let nodes = (0..=8)
        .map(|i| {
            if i < 8 {
                format!(r#"{{"children":[{},{}]}}"#, i + 1, i + 1)
            } else {
                "{}".to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    rejects_repeated_hierarchy(&glb(&nodes, r#"{"nodes":[0]}"#, ",\"scene\":0", false));
}

#[test]
fn shared_node_via_two_parents_rejected() {
    rejects_repeated_hierarchy(&glb(
        r#"{"children":[1,2]},{"children":[3]},{"children":[3]},{"mesh":0}"#,
        r#"{"nodes":[0]}"#,
        ",\"scene\":0",
        true,
    ));
}

#[test]
fn duplicate_scene_roots_rejected() {
    rejects_repeated_hierarchy(&glb(
        r#"{"mesh":0}"#,
        r#"{"nodes":[0,0]}"#,
        ",\"scene\":0",
        true,
    ));
}

#[test]
fn cycles_rejected_as_repeated_hierarchy() {
    for nodes in [
        r#"{"children":[0]}"#,
        r#"{"children":[1]},{"children":[0]}"#,
    ] {
        rejects_repeated_hierarchy(&glb(nodes, r#"{"nodes":[0]}"#, ",\"scene\":0", false));
    }
}

#[test]
fn unique_node_tree_with_distinct_nodes_sharing_mesh_is_valid() {
    let model = assets::decode(
        &glb(
            r#"{"children":[1,2]},{"mesh":0},{"mesh":0,"translation":[1,0,0]}"#,
            r#"{"nodes":[0]}"#,
            ",\"scene\":0",
            true,
        ),
        2.0,
    )
    .unwrap();
    assert_eq!(model.vertices.len(), 6);
    assert_eq!(model.min, [0.0, 0.0, 0.0]);
    assert_eq!(model.max, [2.0, 1.0, 0.0]);
    assert_eq!(model.vertices[3].position, [1.0, 0.0, 0.0]);
}

#[test]
fn nodes_reused_in_separate_scenes_are_valid() {
    for selected in [",\"scene\":0", ",\"scene\":1", ""] {
        let model = assets::decode(
            &glb(
                r#"{"mesh":0}"#,
                r#"{"nodes":[0]},{"nodes":[0]}"#,
                selected,
                true,
            ),
            2.0,
        )
        .unwrap();
        assert_eq!(model.vertices.len(), 3);
    }
}

#[test]
fn unselected_scene_repeated_roots_do_not_invalidate_selected_scene() {
    let model = assets::decode(
        &glb(
            r#"{"mesh":0}"#,
            r#"{"nodes":[0]},{"nodes":[0,0]}"#,
            ",\"scene\":0",
            true,
        ),
        2.0,
    )
    .unwrap();
    assert_eq!(model.vertices.len(), 3);
}
