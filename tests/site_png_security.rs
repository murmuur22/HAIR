use hair::content::Site;
use std::{fs, path::Path};
fn png(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(color);
        e.set_depth(depth);
        let mut writer = e.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    out
}
fn load(dir: &Path, bytes: &[u8]) -> hair::Result<Site> {
    fs::write(dir.join("pixel.png"), bytes).unwrap();
    Site::load(dir)
}
#[test]
fn site_png_strict_bounds_and_metadata_controls() {
    let dir = std::env::temp_dir().join(format!("hair-site-png-repair-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("index.md"), "# Pixels\n![pixel](pixel.png)").unwrap();
    let good = png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[0, 255, 0, 255],
    );
    assert_eq!(
        load(&dir, &good).unwrap().images["pixel.png"].rgba,
        [0, 255, 0, 255]
    );
    let maximum = png(
        256,
        256,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &vec![255; 256 * 256 * 4],
    );
    assert_eq!(
        load(&dir, &maximum).unwrap().images["pixel.png"].rgba.len(),
        256 * 256 * 4
    );
    for (color, depth, data, expected) in [
        (
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            vec![1, 2, 3],
            vec![1, 2, 3, 255],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            vec![20],
            vec![20, 20, 20, 255],
        ),
        (
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            vec![20, 90],
            vec![20, 20, 20, 90],
        ),
        (
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            vec![1, 0, 2, 0, 3, 0],
            vec![1, 2, 3, 255],
        ),
    ] {
        assert_eq!(
            load(&dir, &png(1, 1, color, depth, &data)).unwrap().images["pixel.png"].rgba,
            expected
        );
    }
    // Encoder-built text metadata remains pixel-compatible; its content is not used.
    let mut text = Vec::new();
    {
        let mut e = png::Encoder::new(&mut text, 1, 1);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.add_text_chunk("Title".into(), "Normal".into()).unwrap();
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[0, 255, 0, 255]).unwrap();
        w.finish().unwrap();
    }
    assert_eq!(
        load(&dir, &text).unwrap().images["pixel.png"].rgba,
        [0, 255, 0, 255]
    );
    for end in [0, 8, 24, 33, good.len() - 12, good.len() - 1] {
        assert!(load(&dir, &good[..end]).is_err(), "truncation at {end}");
    }
    let mut bad_crc = good.clone();
    bad_crc[29] ^= 1;
    let mut bad_length = good.clone();
    bad_length[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    for bytes in [
        bad_crc,
        bad_length,
        vec![0; 262145],
        png(
            257,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &vec![255; 257 * 4],
        ),
        png(
            1,
            257,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &vec![255; 257 * 4],
        ),
    ] {
        assert!(load(&dir, &bytes).is_err());
    }
    fs::remove_dir_all(dir).unwrap();
}
