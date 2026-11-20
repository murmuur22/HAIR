//! Qualification of the public local PNG profile, not Site/pack policy replacement.
use hair::assets::decode_local_png;
use png::{BitDepth, ColorType};

fn encode(w: u32, h: u32, color: ColorType, depth: BitDepth, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if color == ColorType::Indexed {
            encoder.set_palette(vec![19, 37, 83]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    out
}
fn control() -> Vec<u8> {
    encode(1, 1, ColorType::Rgba, BitDepth::Eight, &[19, 37, 83, 109])
}
fn exact(bytes: &[u8], w: u32, h: u32, rgba: &[u8]) {
    let image = decode_local_png(bytes).expect("supported owned encoded control");
    assert_eq!((image.width, image.height), (w, h));
    assert_eq!(image.rgba, rgba);
}
fn valid_control() {
    exact(&control(), 1, 1, &[19, 37, 83, 109]);
}
// Use the existing encoder's checksum implementation, not an unchecked CRC literal.
// Extract exactly the requested chunk from a complete encoder-built PNG.
fn chunk(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 1, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_chunk(png::chunk::ChunkType(kind), payload)
            .unwrap();
        writer.write_image_data(&[19, 37, 83, 109]).unwrap();
        writer.finish().unwrap();
    }
    out[33..33 + 12 + payload.len()].to_vec()
}
fn with_chunk(bytes: &[u8], at: usize, kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = bytes[..at].to_vec();
    out.extend(chunk(kind, payload));
    out.extend_from_slice(&bytes[at..]);
    out
}
fn refused(bytes: &[u8], diagnostic: &str) {
    let error = decode_local_png(bytes).expect_err("policy must refuse encoded source");
    assert!(
        error.contains(diagnostic),
        "expected {diagnostic:?}, got {error:?}"
    );
}

#[test]
fn png_qual_admitted_samples_preserve_dimensions_and_straight_rgba() {
    // Includes transparent colored RGB and nontrivial alpha: no premultiplication,
    // gamma conversion or compositing belongs to this byte-to-texture assertion.
    for (color, source, rgba) in [
        (
            ColorType::Rgb,
            vec![19, 37, 83, 201, 7, 129],
            vec![19, 37, 83, 255, 201, 7, 129, 255],
        ),
        (
            ColorType::Grayscale,
            vec![19, 201],
            vec![19, 19, 19, 255, 201, 201, 201, 255],
        ),
        (
            ColorType::GrayscaleAlpha,
            vec![19, 0, 201, 109],
            vec![19, 19, 19, 0, 201, 201, 201, 109],
        ),
        (
            ColorType::Rgba,
            vec![19, 37, 83, 0, 201, 7, 129, 109],
            vec![19, 37, 83, 0, 201, 7, 129, 109],
        ),
    ] {
        exact(&encode(2, 1, color, BitDepth::Eight, &source), 2, 1, &rgba);
        exact(&encode(1, 2, color, BitDepth::Eight, &source), 1, 2, &rgba);
    }
}
#[test]
fn png_qual_trns_gray_and_rgb_expand_only_matching_samples() {
    valid_control();
    let gray = encode(2, 1, ColorType::Grayscale, BitDepth::Eight, &[19, 20]);
    exact(
        &with_chunk(&gray, 33, *b"tRNS", &[0, 19]),
        2,
        1,
        &[19, 19, 19, 0, 20, 20, 20, 255],
    );
    let rgb = encode(
        2,
        1,
        ColorType::Rgb,
        BitDepth::Eight,
        &[19, 37, 83, 19, 37, 84],
    );
    exact(
        &with_chunk(&rgb, 33, *b"tRNS", &[0, 19, 0, 37, 0, 83]),
        2,
        1,
        &[19, 37, 83, 0, 19, 37, 84, 255],
    );
}
#[test]
fn png_qual_single_pixel_adam7_has_exact_samples() {
    // For 1x1, Adam7 has exactly one nonempty pass with the same single filtered
    // row. Reframe IHDR with encoder-calculated CRC; compressed pixels unchanged.
    // This does NOT claim multipass/interlaced row reconstruction coverage.
    let mut bytes = control();
    let mut ihdr = bytes[16..29].to_vec();
    ihdr[12] = 1;
    bytes.splice(8..33, chunk(*b"IHDR", &ihdr));
    exact(&bytes, 1, 1, &[19, 37, 83, 109]);
}
#[test]
fn png_qual_truncation_crc_and_terminal_framing_refuse() {
    valid_control();
    let good = control();
    for end in [0, 8, 24, 33, good.len() - 12, good.len() - 1] {
        assert!(decode_local_png(&good[..end]).is_err(), "truncation {end}");
    }
    let mut header_crc = good.clone();
    header_crc[29] ^= 1;
    assert!(decode_local_png(&header_crc).is_err());
    let mut idat_crc = good.clone();
    let idat_len = u32::from_be_bytes(idat_crc[33..37].try_into().unwrap()) as usize;
    assert_eq!(&idat_crc[37..41], b"IDAT");
    idat_crc[41 + idat_len] ^= 1;
    assert!(decode_local_png(&idat_crc).is_err());
    let mut ancillary_crc = with_chunk(&good, 33, *b"vpAg", &[7]);
    ancillary_crc[42] ^= 1;
    assert!(decode_local_png(&ancillary_crc).is_err());
    let mut trailing = good.clone();
    trailing.push(0);
    refused(&trailing, "IEND/trailing");
    let mut bad_length = good;
    bad_length[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    refused(&bad_length, "chunk exceeds encoded bytes");
}
#[test]
fn png_qual_axis_limits_refuse_before_pixel_allocation() {
    valid_control();
    for (w, h) in [(1024, 1), (1, 1024)] {
        let source = vec![37; 1024];
        let expected = [37, 37, 37, 255].repeat(1024);
        exact(
            &encode(w, h, ColorType::Grayscale, BitDepth::Eight, &source),
            w,
            h,
            &expected,
        );
    }
    for (w, h) in [(1025_u32, 1_u32), (1, 1025), (u32::MAX, u32::MAX)] {
        let mut bytes = control();
        let mut ihdr = bytes[16..29].to_vec();
        ihdr[..4].copy_from_slice(&w.to_be_bytes());
        ihdr[4..8].copy_from_slice(&h.to_be_bytes());
        bytes.splice(8..33, chunk(*b"IHDR", &ihdr));
        // The huge header intentionally has tiny compressed data. No huge source
        // or output allocation is used to exercise header admission.
        assert!(decode_local_png(&bytes).is_err(), "dimensions {w}x{h}");
    }
}
#[test]
fn png_qual_unsupported_depth_and_palette_are_not_lossily_normalized() {
    valid_control();
    for color in [
        ColorType::Rgb,
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgba,
    ] {
        let bytes = encode(
            1,
            1,
            color,
            BitDepth::Sixteen,
            &vec![37; color.samples() * 2],
        );
        refused(&bytes, "no indexed/16-bit conversion");
    }
    refused(
        &encode(1, 1, ColorType::Indexed, BitDepth::Eight, &[0]),
        "no indexed/16-bit conversion",
    );
    refused(
        &encode(1, 1, ColorType::Grayscale, BitDepth::One, &[0]),
        "no indexed/16-bit conversion",
    );
}
#[test]
fn png_qual_apng_chunks_refuse_before_and_after_idat() {
    valid_control();
    let good = control();
    let mut actl = Vec::new();
    actl.extend(1_u32.to_be_bytes());
    actl.extend(0_u32.to_be_bytes());
    let mut fctl = Vec::new();
    for n in [0_u32, 1, 1, 0, 0] {
        fctl.extend(n.to_be_bytes());
    }
    fctl.extend([0, 1, 0, 10, 0, 0]);
    for (kind, payload) in [
        (*b"acTL", actl),
        (*b"fcTL", fctl),
        (*b"fdAT", vec![0, 0, 0, 0]),
    ] {
        for at in [33, good.len() - 12] {
            refused(
                &with_chunk(&good, at, kind, &payload),
                "animated PNG refused",
            );
        }
    }
}
#[test]
fn png_qual_color_orientation_profile_refusals_keep_srgb_controls() {
    valid_control();
    let good = control();
    exact(
        &with_chunk(&good, 33, *b"gAMA", &45455_u32.to_be_bytes()),
        1,
        1,
        &[19, 37, 83, 109],
    );
    exact(
        &with_chunk(&good, 33, *b"sRGB", &[0]),
        1,
        1,
        &[19, 37, 83, 109],
    );
    for gamma in [0_u32, 100000, 45454] {
        refused(
            &with_chunk(&good, 33, *b"gAMA", &gamma.to_be_bytes()),
            "non-sRGB gamma",
        );
    }
    for kind in [*b"iCCP", *b"eXIf", *b"cHRM", *b"cICP", *b"mDCV", *b"cLLI"] {
        // Policy rejects chunk presence before parsing/expanding its payload.
        // These are framed/CRC-correct policy probes, not valid ICC/EXIF files.
        for at in [33, good.len() - 12] {
            refused(
                &with_chunk(&good, at, kind, &[0; 8]),
                "custom color profile/orientation",
            );
        }
    }
}
#[test]
fn png_qual_chunk_count_boundary_has_small_valid_control() {
    valid_control();
    let good = control();
    let empty = chunk(*b"vpAg", &[]);
    let mut bytes = good[..33].to_vec();
    for _ in 0..4093 {
        bytes.extend_from_slice(&empty);
    }
    bytes.extend_from_slice(&good[33..]);
    exact(&bytes, 1, 1, &[19, 37, 83, 109]); // IHDR + 4093 ancillary + IDAT + IEND
    refused(
        &with_chunk(&bytes, 33, *b"vpAg", &[]),
        "chunk count exceeds 4096",
    );
}
#[test]
fn png_qual_aggregate_ancillary_boundary_counts_ihdr_and_post_idat() {
    valid_control();
    let good = control();
    exact(
        &with_chunk(&good, 33, *b"tEXt", b"Title\0literal source"),
        1,
        1,
        &[19, 37, 83, 109],
    );
    // Actual policy counts all non-IDAT payload, including the 13-byte IHDR.
    let payload = vec![7; 256 * 1024 - 13];
    exact(
        &with_chunk(&good, 33, *b"vpAg", &payload),
        1,
        1,
        &[19, 37, 83, 109],
    );
    let before = with_chunk(&good, 33, *b"vpAg", &payload[..payload.len() / 2]);
    let over = with_chunk(
        &before,
        before.len() - 12,
        *b"vpAg",
        &payload[payload.len() / 2..],
    );
    exact(&over, 1, 1, &[19, 37, 83, 109]);
    refused(
        &with_chunk(&over, over.len() - 12, *b"vpAg", &[1]),
        "aggregate metadata exceeds 256 KiB",
    );
}
#[test]
fn png_qual_encoded_cap_refuses_bounded_input_before_chunk_parsing() {
    valid_control();
    // Only cap+1 bytes, not a decompression bomb or multi-gigabyte fixture.
    let mut bytes = vec![0; 4 * 1024 * 1024 + 1];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    refused(&bytes, "4 MiB encoded budget");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod acquisition {
    use super::*;
    use hair::local_explorer::{Explorer, MAX_BODY_BYTES, READER_CHUNK_BYTES};
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let temp = std::env::var_os("TMPDIR").expect("explicit owned TMPDIR required");
            let base = PathBuf::from(temp).canonicalize().unwrap();
            let owned = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap();
            assert!(base.starts_with(owned.parent().unwrap()) && !base.starts_with(&owned));
            let path = base.join(format!(
                "hair-png-acquisition-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("exclusive fixture directory");
            Self(path)
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(self.0.join(name))
                .unwrap();
            file.write_all(bytes).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn png_qual_held_description_survives_path_replacement_after_explicit_rebaseline() {
        let fixture = Fixture::new();
        let name = "source\né.bin";
        let good = control();
        fixture.write(name, &good);
        let explorer = Explorer::new(&fixture.0).unwrap();
        assert_eq!(explorer.current().entries[0].name, name);
        let mut reader = explorer.open_reader(0).unwrap();
        assert_eq!(
            reader.read_image_bytes(&AtomicBool::new(false)).unwrap(),
            good
        );
        let before_rename = reader.try_clone().unwrap();
        fs::rename(fixture.0.join(name), fixture.0.join("held")).unwrap();
        fixture.write(name, b"PATH IMPOSTOR");
        // Rename can change ctime. Do NOT assume old baseline survives rename.
        assert_eq!(reader.retry_from_zero().unwrap().body(), good);
        let staged = reader.try_clone().unwrap();
        let bytes = staged.read_image_bytes(&AtomicBool::new(false)).unwrap();
        assert_eq!(bytes, good);
        exact(&bytes, 1, 1, &[19, 37, 83, 109]);
        assert_eq!(reader.read_chunk(0).unwrap().body(), good);
        // Old clone remains usable only if its original stamp still matches;
        // it must never reset baseline or silently follow the impostor.
        if let Ok(bytes) = before_rename.read_image_bytes(&AtomicBool::new(false)) {
            assert_eq!(bytes, good);
        }
        assert_eq!(fs::read(fixture.0.join(name)).unwrap(), b"PATH IMPOSTOR");
        assert_eq!(fs::read(fixture.0.join("held")).unwrap(), good);
    }
    #[test]
    fn png_qual_mutation_after_open_or_clone_refuses_until_same_handle_retry_and_cancellation_is_separate()
     {
        let fixture = Fixture::new();
        let good = control();
        fixture.write("source", &good);
        let explorer = Explorer::new(&fixture.0).unwrap();
        let mut reader = explorer.open_reader(0).unwrap();
        let old = reader.try_clone().unwrap();
        assert_eq!(
            reader.read_image_bytes(&AtomicBool::new(false)).unwrap(),
            good
        );
        assert!(
            reader
                .read_image_bytes(&AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
        // Length changes deterministically: no filesystem timestamp sleeps.
        let changed = encode(2, 1, ColorType::Rgb, BitDepth::Eight, &[1, 2, 3, 4, 5, 6]);
        let changed = with_chunk(&changed, 33, *b"vpAg", &[0; 101]);
        assert_ne!(changed.len(), good.len());
        fs::write(fixture.0.join("source"), &changed).unwrap();
        assert!(reader.read_image_bytes(&AtomicBool::new(false)).is_err());
        assert!(old.read_image_bytes(&AtomicBool::new(false)).is_err());
        assert!(reader.read_chunk(0).is_err());
        assert_eq!(reader.retry_from_zero().unwrap().body(), changed);
        assert!(
            old.read_image_bytes(&AtomicBool::new(false)).is_err(),
            "clone baseline is copied, not shared/reset"
        );
        let bytes = reader
            .try_clone()
            .unwrap()
            .read_image_bytes(&AtomicBool::new(false))
            .unwrap();
        assert_eq!(bytes, changed);
        exact(&bytes, 2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(fs::read(fixture.0.join("source")).unwrap(), changed);
    }
    #[test]
    fn png_qual_sparse_image_cap_is_distinct_from_raw4096_and_old_whole_body_guard() {
        let fixture = Fixture::new();
        fixture.write("source", b"ORIGINAL");
        let path = fixture.0.join("source");
        let writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
        writer.set_len(MAX_BODY_BYTES + 1).unwrap(); // sparse, no huge buffer
        let explorer = Explorer::new(&fixture.0).unwrap();
        assert!(explorer.read_entry(0, MAX_BODY_BYTES + 1).is_err());
        assert!(explorer.read_entry(0, MAX_BODY_BYTES).is_err());
        let mut reader = explorer.open_reader(0).unwrap();
        assert!(
            reader
                .read_image_bytes(&AtomicBool::new(false))
                .unwrap_err()
                .contains("4 MiB")
        );
        let first = reader.read_chunk(0).unwrap();
        assert_eq!(first.used, READER_CHUNK_BYTES);
        assert_eq!(&first.body()[..8], b"ORIGINAL");
        let tail = reader.read_chunk(MAX_BODY_BYTES).unwrap();
        assert_eq!(tail.body(), &[0]);
        assert!(tail.eof);
        let good = control();
        writer.set_len(0).unwrap();
        fs::write(&path, &good).unwrap();
        assert!(reader.read_image_bytes(&AtomicBool::new(false)).is_err());
        assert_eq!(reader.retry_from_zero().unwrap().body(), good);
        assert_eq!(
            reader.read_image_bytes(&AtomicBool::new(false)).unwrap(),
            good
        );
        assert_eq!(explorer.read_entry(0, MAX_BODY_BYTES).unwrap(), good);
    }
}
