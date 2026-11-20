//! Synthetic encoder-built indexed fixtures; prepared in a file-only lane.
use super::*;
use crate::sky::{PanoramaFit, PanoramaOptions, adapt_png};

const COLORS: [u8; 6] = [19, 37, 83, 201, 7, 129];

fn indexed(w: u32, h: u32, palette: &[u8], alpha: Option<&[u8]>, indices: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(palette.to_vec());
        if let Some(alpha) = alpha {
            encoder.set_trns(alpha.to_vec());
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(indices).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

// Independent CRC constructor permits malformed structure with genuinely correct CRCs.
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = !0_u32;
    for &byte in &out[4..] {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
    out
}
fn chunks(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut at = 8;
    let mut out = Vec::new();
    while at < bytes.len() {
        let n = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        out.push((
            bytes[at + 4..at + 8].try_into().unwrap(),
            bytes[at + 8..at + 8 + n].to_vec(),
        ));
        at += n + 12;
    }
    assert_eq!(at, bytes.len());
    out
}
fn rebuild(parts: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in parts {
        out.extend(chunk(kind, data));
    }
    out
}
fn change(bytes: &[u8], kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut parts = chunks(bytes);
    parts.iter_mut().find(|p| p.0 == kind).unwrap().1 = data.to_vec();
    rebuild(&parts)
}
fn raw_control(bytes: &[u8], expected: &[u8]) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    reader.finish().unwrap();
    assert_eq!(frame.color_type, png::ColorType::Indexed);
    assert_eq!(frame.bit_depth, png::BitDepth::Eight);
    assert_eq!(&pixels[..frame.buffer_size()], expected);
}
fn refuses(bytes: &[u8], stage: &str) {
    let error = decode_sky_png(bytes).unwrap_err();
    assert!(error.contains(stage), "wanted {stage}, got {error}");
    assert!(adapt_png(bytes, PanoramaOptions::default()).is_err());
}

// Test-local IEC sRGB reference, not production transfer/composition helpers.
fn linear(c: u8) -> f64 {
    let s = f64::from(c) / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(v: f64) -> u8 {
    let s = if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (255.0 * s).round().clamp(0.0, 255.0) as u8
}
fn composite(rgb: &[u8], alpha: u8) -> [u8; 4] {
    let a = f64::from(alpha) / 255.0;
    let bg = [0.04, 0.06, 0.05];
    [
        encoded(linear(rgb[0]) * a + bg[0] * (1.0 - a)),
        encoded(linear(rgb[1]) * a + bg[1] * (1.0 - a)),
        encoded(linear(rgb[2]) * a + bg[2] * (1.0 - a)),
        255,
    ]
}

#[test]
fn sky_palette_distinguishable_straight_rgba_and_exact_opaque_passthrough() {
    let source = indexed(2, 1, &COLORS, None, &[0, 1]);
    let before = source.clone();
    raw_control(&source, &[0, 1]);
    let expected = [19, 37, 83, 255, 201, 7, 129, 255];
    let straight = decode_sky_png(&source).unwrap();
    assert_eq!((straight.width, straight.height), (2, 1));
    assert_eq!(straight.rgba, expected);
    assert_eq!(straight.rgba.capacity(), 8);
    for fit in [PanoramaFit::Fill, PanoramaFit::Fit, PanoramaFit::Stretch] {
        let result = adapt_png(
            &source,
            PanoramaOptions {
                fit,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.image().rgba, expected);
        assert_eq!((result.image().width, result.image().height), (2, 1));
    }
    assert_eq!(source, before);
    assert!(
        decode_local_png(&source)
            .unwrap_err()
            .contains("no indexed/16-bit conversion")
    );
}

#[test]
fn sky_palette_real_raw_index_cardinality_counterexamples() {
    let one = indexed(1, 1, &COLORS[..3], None, &[0]);
    raw_control(&one, &[0]);
    assert_eq!(decode_sky_png(&one).unwrap().rgba, [19, 37, 83, 255]);
    for index in [1, 255] {
        let invalid = indexed(1, 1, &COLORS[..3], None, &[index]);
        raw_control(&invalid, &[index]); // genuine decodable invalid index, not bad entropy/CRC
        refuses(&invalid, "index outside PLTE");
    }
    let full: Vec<u8> = (0..=255_u8).flat_map(|i| [i, 255 - i, i / 2]).collect();
    let valid = indexed(2, 1, &full, None, &[0, 255]);
    raw_control(&valid, &[0, 255]);
    assert_eq!(
        decode_sky_png(&valid).unwrap().rgba,
        [0, 255, 0, 255, 255, 0, 127, 255]
    );
}

#[test]
fn sky_palette_every_row_and_last_real_index_are_checked() {
    let valid = indexed(3, 2, &COLORS, None, &[0, 1, 0, 1, 0, 1]);
    raw_control(&valid, &[0, 1, 0, 1, 0, 1]);
    assert_eq!(decode_sky_png(&valid).unwrap().rgba.len(), 24);
    for offset in 0..6 {
        let mut indices = [0, 1, 0, 1, 0, 1];
        indices[offset] = 2;
        let invalid = indexed(3, 2, &COLORS, None, &indices);
        raw_control(&invalid, &indices);
        refuses(&invalid, "index outside PLTE");
    }
}

#[test]
fn sky_palette_maximum_input_and_full_alpha_table_use_existing_charge() {
    let full: Vec<u8> = (0..=255_u8).flat_map(|i| [i, 255 - i, i / 2]).collect();
    let mut alpha = vec![255; 256];
    alpha[255] = 128;
    let source = indexed(4096, 1024, &full, Some(&alpha), &vec![255; 4_194_304]);
    assert!(source.len() <= 4_000_000);
    let image = decode_sky_png(&source).unwrap();
    assert_eq!((image.width, image.height), (4096, 1024));
    assert_eq!(image.rgba.len(), 16_777_216);
    assert_eq!(image.rgba.capacity(), 16_777_216);
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [255, 0, 127, 128])
    );
    assert!(decode_local_png(&source).is_err());
    // Same conservative shared admission, not an extra raster allowance or RSS measurement.
    assert_eq!(
        sky_png_budget(4096, 1024, source.len(), SKY_PNG_WORK_CAP).unwrap(),
        image.rgba.len()
    );
    assert!(sky_png_reserve(9, 8).is_err());
    let exact = sky_png_reserve(8, 8).unwrap();
    assert_eq!(exact.capacity(), 8);
}

#[test]
fn sky_palette_short_full_alpha_and_independent_composition() {
    for alpha in [
        vec![0],
        vec![128],
        vec![0, 128],
        vec![255, 0],
        vec![0, 0],
        vec![255, 255],
    ] {
        let source = indexed(2, 1, &COLORS, Some(&alpha), &[0, 1]);
        let before = source.clone();
        let a0 = alpha[0];
        let a1 = alpha.get(1).copied().unwrap_or(255);
        let straight = decode_sky_png(&source).unwrap();
        assert_eq!(straight.rgba, [19, 37, 83, a0, 201, 7, 129, a1]);
        let mut expected = composite(&COLORS[..3], a0).to_vec();
        expected.extend(composite(&COLORS[3..], a1));
        let result = adapt_png(&source, PanoramaOptions::default()).unwrap();
        assert_eq!((result.image().width, result.image().height), (2, 1));
        assert_eq!(result.image().rgba, expected);
        assert_eq!(source, before);
        assert!(decode_local_png(&source).is_err());
    }
}

#[test]
fn sky_palette_nonconforming_adaptation_and_poisoned_rgb_filtering() {
    let source = indexed(4, 1, &COLORS[..3], Some(&[128]), &[0; 4]);
    let before = source.clone();
    let expected = composite(&COLORS[..3], 128);
    let result = adapt_png(&source, PanoramaOptions::default()).unwrap();
    assert_eq!((result.image().width, result.image().height), (1024, 512));
    assert_eq!(result.image().rgba.len(), 2_097_152);
    assert!(result.image().rgba.capacity() <= 2_097_152);
    assert!(
        result
            .image()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == expected)
    );
    assert_eq!(source, before);
    // Stretch a 2x2 raster of repeated horizontal opaque/transparent pairs.
    // Independent center-aligned two-tap expectation for EVERY output pixel.
    let mut held = None;
    for poison in [[255, 0, 255], [0, 255, 0]] {
        let palette = [19, 37, 83, poison[0], poison[1], poison[2]];
        let bytes = indexed(2, 2, &palette, Some(&[255, 0]), &[0, 1, 0, 1]);
        let unchanged = bytes.clone();
        let image = adapt_png(
            &bytes,
            PanoramaOptions {
                fit: PanoramaFit::Stretch,
                ..Default::default()
            },
        )
        .unwrap();
        let bg = [0.04, 0.06, 0.05];
        for (i, p) in image.image().rgba.as_chunks::<4>().0.iter().enumerate() {
            let u = (((i % 1024) as f64 + 0.5) / 512.0 - 0.5).clamp(0.0, 1.0);
            let expected = [
                encoded(linear(19) * (1.0 - u) + bg[0] * u),
                encoded(linear(37) * (1.0 - u) + bg[1] * u),
                encoded(linear(83) * (1.0 - u) + bg[2] * u),
                255,
            ];
            assert_eq!(*p, expected);
        }
        if let Some(previous) = &held {
            assert_eq!(&image.image().rgba, previous);
        }
        held = Some(image.image().rgba.clone());
        assert_eq!(bytes, unchanged);
    }
}

#[test]
fn sky_palette_crc_correct_plte_and_trns_structure_negatives() {
    let source = indexed(2, 1, &COLORS, Some(&[128]), &[0, 1]);
    assert_eq!(rebuild(&chunks(&source)), source); // independent CRC builder matches real encoder bytes
    assert!(decode_sky_png(&rebuild(&chunks(&source))).is_ok());
    assert!(decode_sky_png(&source).is_ok());
    for payload in [
        vec![],
        vec![1],
        vec![1, 2],
        vec![1; 4],
        vec![1; 769],
        vec![1; 771],
    ] {
        refuses(&change(&source, *b"PLTE", &payload), "PLTE");
    }
    for payload in [vec![], vec![0, 128, 255]] {
        refuses(&change(&source, *b"tRNS", &payload), "tRNS");
    }
    let parts = chunks(&source);
    let header = parts.iter().find(|p| p.0 == *b"IHDR").unwrap().clone();
    let plte = parts.iter().find(|p| p.0 == *b"PLTE").unwrap().clone();
    let trns = parts.iter().find(|p| p.0 == *b"tRNS").unwrap().clone();
    let data: Vec<_> = parts.iter().filter(|p| p.0 == *b"IDAT").cloned().collect();
    let end = parts.last().unwrap().clone();
    let mut cases = Vec::new();
    for prefix in [
        vec![header.clone()],
        vec![header.clone(), trns.clone()],
        vec![header.clone(), plte.clone(), plte.clone()],
        vec![header.clone(), trns.clone(), plte.clone()],
        vec![header.clone(), plte.clone(), trns.clone(), trns.clone()],
    ] {
        let mut p = prefix;
        p.extend(data.clone());
        p.push(end.clone());
        cases.push(p);
    }
    for late in [vec![plte.clone()], vec![plte.clone(), trns.clone()]] {
        let mut p = vec![header.clone()];
        p.extend(data.clone());
        p.extend(late);
        p.push(end.clone());
        cases.push(p);
    }
    let mut late_plte = vec![header.clone(), plte.clone()];
    late_plte.extend(data.clone());
    late_plte.push(plte.clone());
    late_plte.push(end.clone());
    cases.push(late_plte);
    let mut late_alpha = vec![header, plte];
    late_alpha.extend(data);
    late_alpha.push(trns);
    late_alpha.push(end);
    cases.push(late_alpha);
    for p in cases {
        refuses(&rebuild(&p), "palette");
    }
    // Overlong alpha is not merely ignored, unlike the library's EXPAND path.
    let one = indexed(1, 1, &COLORS[..3], Some(&[0]), &[0]);
    assert_eq!(decode_sky_png(&one).unwrap().rgba, [19, 37, 83, 0]);
    refuses(&change(&one, *b"tRNS", &[0, 255]), "tRNS");
}

#[test]
fn sky_palette_preserves_checksum_finish_profile_and_resource_guards() {
    let source = indexed(2, 1, &COLORS, Some(&[128]), &[0, 1]);
    assert!(decode_sky_png(&source).is_ok());
    // Corrupt each actual chunk CRC independently, including PLTE, alpha and IEND.
    let mut at = 8;
    while at < source.len() {
        let n = u32::from_be_bytes(source[at..at + 4].try_into().unwrap()) as usize;
        let mut bad = source.clone();
        bad[at + 8 + n] ^= 1;
        assert!(decode_sky_png(&bad).is_err());
        at += n + 12;
    }
    let parts = chunks(&source);
    let idat = &parts.iter().find(|p| p.0 == *b"IDAT").unwrap().1;
    for offset in [0, idat.len() - 1] {
        let mut payload = idat.clone();
        payload[offset] ^= 0x80;
        assert!(decode_sky_png(&change(&source, *b"IDAT", &payload)).is_err());
    }
    // CRC-correct incomplete zlib stream with intact IEND: exercise real decoder
    // completion rather than confusing a missing PNG chunk with entropy finish.
    for removed in [1, 4] {
        assert!(decode_sky_png(&change(&source, *b"IDAT", &idat[..idat.len() - removed])).is_err());
    }
    for short in [source.len() - 1, source.len() - 12] {
        assert!(decode_sky_png(&source[..short]).is_err());
    }
    let mut trailer = source.clone();
    trailer.push(0);
    refuses(&trailer, "trailing");
    for kind in [
        *b"acTL", *b"fcTL", *b"fdAT", *b"iCCP", *b"eXIf", *b"cHRM", *b"cICP", *b"mDCV", *b"cLLI",
        *b"gAMA",
    ] {
        let mut p = parts.clone();
        p.insert(1, (kind, vec![0; 8]));
        assert!(decode_sky_png(&rebuild(&p)).is_err());
    }
    let mut p = parts.clone();
    p.insert(1, (*b"ruSt", vec![0; 256 * 1024]));
    refuses(&rebuild(&p), "metadata");
    let mut p = parts.clone();
    drop(p.splice(1..1, (0..4096).map(|_| (*b"ruSt", vec![]))));
    refuses(&rebuild(&p), "chunk count");
    let mut too_large = source.clone();
    too_large.resize(4_000_001, 0);
    refuses(&too_large, "encoded");
    for (w, h, stage) in [
        (4097_u32, 1_u32, "axes"),
        (1, 4097, "axes"),
        (2049, 2048, "pixels"),
    ] {
        let mut header = parts[0].1.clone();
        header[..4].copy_from_slice(&w.to_be_bytes());
        header[4..8].copy_from_slice(&h.to_be_bytes());
        refuses(&change(&source, *b"IHDR", &header), stage);
    }
}

#[test]
fn sky_palette_lower_depth_sixteen_bit_and_indexed_interlace_stay_excluded() {
    let source = indexed(2, 1, &COLORS, None, &[0, 1]);
    assert!(decode_sky_png(&source).is_ok());
    for depth in [png::BitDepth::One, png::BitDepth::Two, png::BitDepth::Four] {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(depth);
            encoder.set_palette(COLORS.to_vec());
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[1 << (8 - depth as u8 * 2)])
                .unwrap();
            writer.finish().unwrap();
        }
        assert!(decode_sky_png(&bytes).is_err());
        assert!(decode_local_png(&bytes).is_err());
    }
    let header = chunks(&source)[0].1.clone();
    let mut sixteen = header.clone();
    sixteen[8] = 16; // malformed indexed IHDR, not valid 16-bit fixture
    assert!(decode_sky_png(&change(&source, *b"IHDR", &sixteen)).is_err());
    assert!(decode_local_png(&change(&source, *b"IHDR", &sixteen)).is_err());
    let mut interlace = header;
    interlace[12] = 1;
    // Header-stage explicit refusal, NOT a genuine interlaced decoder success claim.
    refuses(&change(&source, *b"IHDR", &interlace), "indexed interlace");
}
