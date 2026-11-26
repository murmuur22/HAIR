//! Explicitly authored synthetic INPUT, not a decoded-model injection.
//! Three-box asymmetric stepped prism, 108 vertices; embedded 32x32 RG/BY PNG.
use super::*;

pub(super) fn rgba() -> Vec<u8> {
    let mut pixels = vec![];
    for y in 0..32 {
        for x in 0..32 {
            pixels.extend(match (x < 16, y < 16) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 0, 255],
            });
        }
    }
    pixels
}
pub(super) fn png_bytes(pixels: &[u8]) -> Vec<u8> {
    let mut bytes = vec![];
    let mut encoder = png::Encoder::new(&mut bytes, 32, 32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    bytes
}
pub(super) fn write_input(path: &Path) {
    write_input_with_matte(path, false);
}
// Historical theme/retained-pin callers keep their original authored bytes.
// Local-file witnesses explicitly opt into the stricter production matte profile.
pub(super) fn write_input_with_matte(path: &Path, matte: bool) {
    let mut positions: Vec<[f32; 3]> = vec![];
    let mut normals: Vec<[f32; 3]> = vec![];
    let mut uvs: Vec<[f32; 2]> = vec![];
    // Asymmetric height and depth: left tower, low right wing, rear raised tab.
    for (lo, hi) in [([-0.5, 0., -0.2], [0., 1., 0.2]),
                     ([0., 0., -0.2], [0.5, 0.55, 0.2]),
                     ([0.15, 0.55, -0.35], [0.4, 0.8, -0.1])] {
        let [x, y, z] = lo; let [a, b, c] = hi;
        for (quad, normal) in [
            ([[x,y,c],[a,y,c],[a,b,c],[x,b,c]], [0.,0.,1.]),
            ([[a,y,z],[x,y,z],[x,b,z],[a,b,z]], [0.,0.,-1.]),
            ([[a,y,c],[a,y,z],[a,b,z],[a,b,c]], [1.,0.,0.]),
            ([[x,y,z],[x,y,c],[x,b,c],[x,b,z]], [-1.,0.,0.]),
            ([[x,b,c],[a,b,c],[a,b,z],[x,b,z]], [0.,1.,0.]),
            ([[x,y,z],[a,y,z],[a,y,c],[x,y,c]], [0.,-1.,0.]),
        ] {
            let uv = [[0.,1.],[1.,1.],[1.,0.],[0.,0.]];
            for i in [0,1,2,0,2,3] { positions.push(quad[i]); normals.push(normal); uvs.push(uv[i]); }
        }
    }
    let n = positions.len();
    assert_eq!(n, 108);
    let mut blob = vec![];
    blob.extend_from_slice(bytemuck::cast_slice(&positions));
    blob.extend_from_slice(bytemuck::cast_slice(&normals));
    blob.extend_from_slice(bytemuck::cast_slice(&uvs));
    let index_offset = blob.len();
    for i in 0..n as u16 { blob.extend(i.to_le_bytes()); }
    let image = png_bytes(&rgba());
    let image_offset = blob.len();
    blob.extend_from_slice(&image);
    let raw_len = blob.len();
    while !blob.len().is_multiple_of(4) { blob.push(0); }
    let material = if matte { ",\"metallicFactor\":0" } else { "" };
    let mut json = format!(r#"{{"asset":{{"version":"2.0","generator":"HAIR slice6C synthetic INPUT"}},"buffers":[{{"byteLength":{raw_len}}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{p}}},{{"buffer":0,"byteOffset":{p},"byteLength":{p}}},{{"buffer":0,"byteOffset":{uv_offset},"byteLength":{uv_len}}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_len}}},{{"buffer":0,"byteOffset":{index_offset},"byteLength":{index_len}}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":{n},"type":"VEC3","min":[-0.5,0,-0.35],"max":[0.5,1,0.2]}},{{"bufferView":1,"componentType":5126,"count":{n},"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":{n},"type":"VEC2"}},{{"bufferView":4,"componentType":5123,"count":{n},"type":"SCALAR"}}],
"images":[{{"bufferView":3,"mimeType":"image/png"}}],"textures":[{{"source":0}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}{material}}}}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3,"material":0}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,
        p=n*12, uv_offset=n*24, uv_len=n*8, image_len=image.len(), index_len=n*2).into_bytes();
    while !json.len().is_multiple_of(4) { json.push(b' '); }
    let mut out = vec![];
    for word in [0x46546c67_u32, 2, (28+json.len()+blob.len()) as u32, json.len() as u32, 0x4e4f534a] { out.extend(word.to_le_bytes()); }
    out.extend(json); out.extend((blob.len() as u32).to_le_bytes()); out.extend(0x004e4942_u32.to_le_bytes()); out.extend(blob);
    std::fs::write(path, &out).unwrap();
    let model = hair::assets::load(path, 3.).unwrap();
    assert_eq!(model.vertices.len(), n);
    assert_eq!(model.min, [-0.5, 0., -0.35]); assert_eq!(model.max, [0.5, 1., 0.2]);
    assert_eq!(model.textures.len(), 1);
    let t = &model.textures[0];
    assert_eq!((t.width,t.height), (32,32)); assert_eq!(t.rgba, rgba());
    assert!(model.vertices.iter().all(|v| v.texture == 1));
}
