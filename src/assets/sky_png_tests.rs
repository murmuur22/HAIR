//! Sky-specific strict PNG profile negatives; prepared, not executed by writer.
use super::*;

fn encode(
    w: u32,
    h: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    pixels: &[u8],
    stored: bool,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if stored {
            encoder.set_compression(png::Compression::NoCompression);
        }
        if color == png::ColorType::Indexed {
            encoder.set_palette(vec![1, 2, 3]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    bytes
}
fn chunk(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(kind), payload)
            .unwrap();
        writer.write_image_data(&[1, 2, 3, 255]).unwrap();
        writer.finish().unwrap();
    }
    assert_eq!(&bytes[37..41], &kind);
    bytes[33..45 + payload.len()].to_vec()
}
fn insert(bytes: &[u8], kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = bytes[..33].to_vec();
    out.extend(chunk(kind, payload));
    out.extend_from_slice(&bytes[33..]);
    out
}
fn dimensions(bytes: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut header = bytes[16..29].to_vec();
    header[..4].copy_from_slice(&w.to_be_bytes());
    header[4..8].copy_from_slice(&h.to_be_bytes());
    let mut out = bytes[..8].to_vec();
    out.extend(chunk(*b"IHDR", &header));
    out.extend_from_slice(&bytes[33..]);
    out
}
fn control() -> Vec<u8> {
    encode(
        2,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[19, 37, 83, 0, 201, 7, 129, 254],
        false,
    )
}

#[test]
fn sky_axis_pixel_rgba_and_work_preflight_are_checked_independently() {
    assert_eq!(
        sky_png_budget(1200, 673, 1, SKY_PNG_WORK_CAP).unwrap(),
        1200 * 673 * 4
    );
    assert_eq!(
        sky_png_budget(4096, 1024, 1, SKY_PNG_WORK_CAP).unwrap(),
        16_777_216
    );
    for (w, h, needle) in [
        (0, 1, "axes"),
        (1, 0, "axes"),
        (4097, 1, "axes"),
        (1, 4097, "axes"),
        (2049, 2048, "pixels"),
    ] {
        assert!(
            sky_png_budget(w, h, 1, SKY_PNG_WORK_CAP)
                .unwrap_err()
                .contains(needle)
        );
    }
    // Independent checked arithmetic helpers use synthetic numeric inputs, not decoder success claims.
    assert!(checked_png_rgba(usize::MAX, 2, usize::MAX).is_err());
    assert!(checked_png_rgba(2, 1, 7).is_err());
    assert_eq!(checked_png_rgba(2, 1, 8).unwrap(), 8);
    let charge = SKY_PNG_SOURCE_ALLOWANCE
        + 2 * 8
        + PNG_DECODER_ALLOWANCE
        + SKY_PNG_OUTPUT_ALLOWANCE
        + SKY_PNG_SCRATCH_ALLOWANCE;
    assert_eq!(sky_png_budget(2, 1, 1, charge).unwrap(), 8);
    assert!(
        sky_png_budget(2, 1, 1, charge - 1)
            .unwrap_err()
            .contains("work")
    );
    assert!(
        sky_png_budget(2, 1, 4_000_001, SKY_PNG_WORK_CAP)
            .unwrap_err()
            .contains("encoded")
    );
    assert!(sky_png_budget(u32::MAX, u32::MAX, 1, SKY_PNG_WORK_CAP).is_err());
    let source = control();
    for (w, h) in [
        (0, 1),
        (4097, 1),
        (1, 4097),
        (2049, 2048),
        (u32::MAX, u32::MAX),
    ] {
        assert!(decode_sky_png(&dimensions(&source, w, h)).is_err());
    }
}

#[test]
fn sky_profile_preserves_straight_samples_and_local_default_contract() {
    let source = control();
    let local = decode_local_png(&source).unwrap();
    let sky = decode_sky_png(&source).unwrap();
    assert_eq!(local.rgba, [19, 37, 83, 0, 201, 7, 129, 254]);
    assert_eq!(local.rgba, sky.rgba);
    let large = encode(
        1025,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[1, 2, 3, 255].repeat(1025),
        false,
    );
    assert!(decode_sky_png(&large).is_ok());
    assert!(decode_local_png(&large).unwrap_err().contains("1..1024"));
    let maximum = encode(
        1024,
        1024,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[1, 2, 3, 255].repeat(1024 * 1024),
        false,
    );
    assert_eq!(
        decode_local_png(&maximum).unwrap().rgba.len(),
        4 * 1024 * 1024
    );
    for (color, pixels) in [
        (png::ColorType::Grayscale, vec![37]),
        (png::ColorType::GrayscaleAlpha, vec![37, 128]),
        (png::ColorType::Rgb, vec![19, 37, 83]),
    ] {
        let bytes = encode(1, 1, color, png::BitDepth::Eight, &pixels, false);
        assert_eq!(
            decode_local_png(&bytes).unwrap().rgba,
            decode_sky_png(&bytes).unwrap().rgba
        );
    }
    let rgb = encode(
        2,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &[19, 37, 83, 201, 7, 129],
        false,
    );
    let trns = insert(&rgb, *b"tRNS", &[0, 19, 0, 37, 0, 83]);
    let image = decode_sky_png(&trns).unwrap();
    assert_eq!(image.rgba, [19, 37, 83, 0, 201, 7, 129, 255]);
    assert_eq!(image.rgba, decode_local_png(&trns).unwrap().rgba);
}

#[test]
fn sky_and_default_retain_profile_structure_crc_adler_finish_negatives() {
    let source = control();
    for (kind, payload) in [
        (*b"acTL", vec![0, 0, 0, 1, 0, 0, 0, 0]),
        (*b"iCCP", vec![0]),
        (*b"eXIf", vec![0]),
        (*b"cHRM", vec![0; 32]),
        (*b"cICP", vec![0; 4]),
        (*b"mDCV", vec![0; 24]),
        (*b"cLLI", vec![0; 8]),
        (*b"gAMA", 100000_u32.to_be_bytes().to_vec()),
    ] {
        let bytes = insert(&source, kind, &payload);
        assert!(decode_sky_png(&bytes).is_err());
        assert!(decode_local_png(&bytes).is_err());
    }
    for (case, bytes) in [
        encode(
            1,
            1,
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            &[0],
            false,
        ),
        encode(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            &[0; 8],
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if case == 0 {
            // Only the approved Sky8-bit indexed row changes. Local indexed
            // and BOTH nonindexed16-bit refusals remain in this same table.
            let image = decode_sky_png(&bytes).unwrap();
            assert_eq!((image.width, image.height), (1, 1));
            assert_eq!(image.rgba, [1, 2, 3, 255]);
        } else {
            assert!(decode_sky_png(&bytes).is_err());
        }
        assert!(decode_local_png(&bytes).is_err());
    }
    let mut mutations = vec![
        source[..source.len() - 12].to_vec(),
        source[..source.len() - 1].to_vec(),
        b"not png".to_vec(),
    ];
    let mut trailer = source.clone();
    trailer.push(0);
    mutations.push(trailer);
    let mut bad_crc = source.clone();
    bad_crc[29] ^= 1;
    mutations.push(bad_crc);
    let ancillary = insert(&source, *b"ruSt", &[1]);
    let mut ancillary_crc = ancillary;
    ancillary_crc[42] ^= 1;
    mutations.push(ancillary_crc);
    // Rebuild IDAT CRC after damaging Adler/deflate, so checksum refusal is not just chunk CRC.
    let len = u32::from_be_bytes(source[33..37].try_into().unwrap()) as usize;
    assert_eq!(&source[37..41], b"IDAT");
    let idat = source[41..41 + len].to_vec();
    for offset in [0, idat.len() - 1] {
        let mut payload = idat.clone();
        payload[offset] ^= 0x80;
        let mut bytes = source[..33].to_vec();
        bytes.extend(chunk(*b"IDAT", &payload));
        bytes.extend_from_slice(&source[45 + len..]);
        mutations.push(bytes);
    }
    for bytes in mutations {
        assert!(decode_sky_png(&bytes).is_err());
        assert!(decode_local_png(&bytes).is_err());
    }
    let metadata = insert(&source, *b"ruSt", &vec![0; 256 * 1024]);
    assert!(decode_sky_png(&metadata).unwrap_err().contains("metadata"));
    assert!(
        decode_local_png(&metadata)
            .unwrap_err()
            .contains("metadata")
    );
    let mut many = source[..33].to_vec();
    let empty = chunk(*b"ruSt", &[]);
    for _ in 0..4096 {
        many.extend_from_slice(&empty);
    }
    many.extend_from_slice(&source[33..]);
    assert!(decode_sky_png(&many).unwrap_err().contains("chunk count"));
    assert!(decode_local_png(&many).unwrap_err().contains("chunk count"));
}

#[test]
fn valid_decimal_encoded_boundary_is_independent_of_structure_and_local_binary_cap() {
    // Stored real pixels produce a nearly4MB valid zlib stream; one bounded unknown
    // ancillary chunk pads the exact decimal boundary without corrupting PNG/entropy.
    let source = encode(
        1000,
        990,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[19, 37, 83, 255].repeat(1000 * 990),
        true,
    );
    assert!(source.len() + 12 < 4_000_000);
    let pad = 4_000_000 - source.len() - 12;
    assert!(pad + 13 < 256 * 1024);
    let exact = insert(&source, *b"ruSt", &vec![0; pad]);
    assert_eq!(exact.len(), 4_000_000);
    assert!(decode_sky_png(&exact).is_ok());
    assert!(decode_local_png(&exact).is_ok());
    let above = insert(&source, *b"ruSt", &vec![0; pad + 1]);
    assert_eq!(above.len(), 4_000_001);
    assert!(decode_local_png(&above).is_ok());
    assert!(decode_sky_png(&above).unwrap_err().contains("encoded"));
    let binary_pad = 4 * 1024 * 1024 - source.len() - 12;
    assert!(binary_pad + 14 <= 256 * 1024);
    let local_exact = insert(&source, *b"ruSt", &vec![0; binary_pad]);
    assert_eq!(local_exact.len(), 4 * 1024 * 1024);
    assert!(decode_local_png(&local_exact).is_ok());
    let local_above = insert(&source, *b"ruSt", &vec![0; binary_pad + 1]);
    assert_eq!(local_above.len(), 4 * 1024 * 1024 + 1);
    assert!(
        decode_local_png(&local_above)
            .unwrap_err()
            .contains("4 MiB encoded")
    );
    let mut trailing = control();
    trailing.push(0);
    assert!(decode_sky_png(&trailing).unwrap_err().contains("trailing"));
}
