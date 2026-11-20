//! Test-only synthetic bytes and owned filesystem fixtures. No production loader calls.
//! Filesystem policy follows existing local_model_preview/text_safety fixture helpers.
pub const POINTS: [[f32; 3]; 4] = [
    [1.0, 2.0, 3.0],
    [5.0, 2.0, 3.0],
    [1.0, 4.0, 3.0],
    [2.0, 2.5, 9.0],
];
pub const INDEX_ACCESSOR: &str =
    r#"{"bufferView":2,"componentType":5123,"count":12,"type":"SCALAR"}"#;
pub const PRIMITIVE: &str =
    r#"{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0,"mode":4}"#;

pub fn document() -> String {
    r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"buffers":[{"byteLength":120}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":48,"byteLength":48,"target":34962},{"buffer":0,"byteOffset":96,"byteLength":24,"target":34963}],"accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[1,2,3],"max":[5,4,9]},{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":2,"componentType":5123,"count":12,"type":"SCALAR"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.91,0.13,0.67,1],"metallicFactor":0,"roughnessFactor":1},"doubleSided":true}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0,"mode":4}]}]}"#.into()
}
pub fn binary() -> Vec<u8> {
    let mut bin = Vec::new();
    for point in POINTS {
        for value in point {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    for _ in POINTS {
        for value in [0.0_f32, 1.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [0_u16, 2, 1, 0, 1, 3, 1, 2, 3, 2, 0, 3] {
        bin.extend_from_slice(&index.to_le_bytes());
    }
    assert_eq!(bin.len(), 120);
    bin
}
pub fn replace(doc: &str, from: &str, to: &str) -> String {
    assert_eq!(
        doc.matches(from).count(),
        1,
        "unique fixture mutation: {from}"
    );
    doc.replacen(from, to, 1)
}
pub fn root_field(doc: &str, field: &str) -> String {
    format!("{},{field}}}", doc.strip_suffix('}').unwrap())
}
pub fn glb(doc: &str, bin: &[u8]) -> Vec<u8> {
    let mut json = doc.as_bytes().to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bin = bin.to_vec();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut bytes = Vec::new();
    for word in [
        0x4654_6c67_u32,
        2,
        (28 + json.len() + bin.len()) as u32,
        json.len() as u32,
        0x4e4f_534a,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    bytes.extend_from_slice(&bin);
    bytes
}
pub fn control() -> Vec<u8> {
    glb(&document(), &binary())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod owned {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};
    pub struct Fixture(pub PathBuf);
    impl Fixture {
        pub fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let base = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit owned TMPDIR"))
                .canonicalize()
                .unwrap();
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap();
            assert!(base.starts_with(source.parent().unwrap()));
            assert!(!base.starts_with(&source) && !source.starts_with(&base));
            let path = base.join(format!(
                "gs{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("exclusive fixture, never reuse leftovers");
            Self(path)
        }
        pub fn file(&self, name: &str, bytes: &[u8]) {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(self.0.join(name))
                .unwrap();
            file.write_all(bytes).unwrap();
            file.flush().unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
