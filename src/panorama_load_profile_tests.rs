//! Parent-run CPU load/profile regression copied from the executed external oracle.
//! Ignored: requires explicit exclusive sibling output; canonical verification pending.
use crate::{
    assets::decode_local_png,
    content::read_local,
    sky::{Panorama, SkyAppearance},
    theme::{AssetCache, Theme},
};
use png::{BitDepth, ColorType};
use std::{
    ffi::CString,
    fs,
    io::Write,
    os::unix::{ffi::OsStrExt, fs::FileTypeExt, fs::symlink},
    path::{Path, PathBuf},
    sync::Arc,
};

const TOKEN: &str = "OWNED_PNG_PAYLOAD_SENTINEL_NOT_USER_DATA";
const RGB: [u8; 6] = [19, 37, 83, 201, 7, 129];
const RGBA: [u8; 8] = [19, 37, 83, 255, 201, 7, 129, 255];

fn encode(w: u32, h: u32, color: ColorType, depth: BitDepth, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if color == ColorType::Indexed {
            encoder.set_palette(RGB.to_vec());
        }
        let mut writer = encoder.write_header().expect("SETUP encoder header");
        writer
            .write_image_data(pixels)
            .expect("SETUP encoder pixels");
        writer.finish().expect("SETUP encoder finish");
    }
    bytes
}

// Use the real encoder's CRC implementation for deliberate structural mutations.
// These are explicitly malformed/profile probes, never claimed successful images.
fn chunk(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().expect("SETUP chunk header");
        writer
            .write_chunk(png::chunk::ChunkType(kind), payload)
            .expect("SETUP chunk CRC");
        writer.write_image_data(&RGB).expect("SETUP chunk pixels");
        writer.finish().expect("SETUP chunk finish");
    }
    assert_eq!(&bytes[37..41], &kind, "SETUP chunk offset");
    bytes[33..45 + payload.len()].to_vec()
}
fn inserted(bytes: &[u8], kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = bytes[..33].to_vec();
    out.extend(chunk(kind, payload));
    out.extend_from_slice(&bytes[33..]);
    out
}
fn header_dimensions(bytes: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut ihdr = bytes[16..29].to_vec();
    ihdr[..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    let mut out = bytes[..8].to_vec();
    out.extend(chunk(*b"IHDR", &ihdr));
    out.extend_from_slice(&bytes[33..]);
    out
}
fn decoded(bytes: &[u8], w: u32, h: u32, rgba: &[u8]) {
    let image = decode_local_png(bytes).expect("SETUP strict decoder positive control");
    assert_eq!(
        (image.width, image.height),
        (w, h),
        "SETUP decoded dimensions"
    );
    assert!(image.rgba == rgba, "SETUP decoded exact source samples");
}

struct Fixture {
    root: PathBuf,
    pack: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        assert_eq!(
            std::env::consts::OS,
            "macos",
            "SETUP Darwin reader contract"
        );
        let out = PathBuf::from(
            std::env::var_os("HAIR_PANORAMA_PROFILE_OUTPUT")
                .expect("SETUP explicit isolated output required"),
        );
        let out = out.canonicalize().expect("SETUP output exists");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert_eq!(out.parent(), source.parent(), "SETUP sibling output");
        assert_eq!(out.file_name().and_then(|s| s.to_str()), Some("output"));
        assert!(
            fs::read_dir(&out).unwrap().next().is_none(),
            "SETUP output must be empty"
        );
        let root = out.join("fixture");
        let pack = root.join("pack");
        fs::create_dir(&root).expect("SETUP exclusive fixture");
        fs::create_dir(&pack).expect("SETUP exclusive pack");
        Self { root, pack }
    }
    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.pack.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("SETUP owned parents");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("SETUP exclusive owned file");
        file.write_all(bytes).expect("SETUP write fixture");
    }
    fn manifest(&self, version: u32, sky: &str) -> PathBuf {
        let path = self.pack.join("profile.toml");
        // Only this test-created manifest is replaced; all source is immutable.
        let text = format!("version = {version}\nname = 'Owned profile'\nroles = []\n{sky}\n");
        fs::write(&path, text).expect("SETUP owned manifest write");
        path
    }
    fn panorama_manifest(&self, file: &str) -> PathBuf {
        // TOML string encoding is real serializer output, including backslash/NUL probes.
        let literal = toml::Value::String(file.into()).to_string();
        self.manifest(3, &format!("[sky]\nkind = 'panorama'\nfile = {literal}"))
    }
    fn load(&self, path: &Path, cache: &mut AssetCache, case: &str) -> Theme {
        eprintln!("PANORAMA_PROFILE_CASE_BEGIN {case}");
        match Theme::load(path, cache) {
            Ok(theme) => theme,
            Err(_) => panic!("PANORAMA_PROFILE_RED admission {case}; raw input/error withheld"),
        }
    }
    fn refuse(&self, path: &Path, cache: &mut AssetCache, case: &str, needle: &str) {
        eprintln!("PANORAMA_PROFILE_CASE_BEGIN {case}");
        match Theme::load(path, cache) {
            Ok(_) => panic!("PANORAMA_PROFILE_RED accepted forbidden {case}"),
            Err(error) => {
                if needle.starts_with("sky PNG ") {
                    assert!(error.contains("SKY_REFUSED"));
                }
                assert!(
                    error.contains(needle),
                    "PANORAMA_PROFILE_RED refusal stage {case}; raw input/error withheld"
                );
                // Do not emit parser excerpts, paths, URLs or encoded payloads to receipts.
            }
        }
        eprintln!("PANORAMA_PROFILE_CASE_OK {case}");
    }
    fn refuse_file(&self, file: &str, cache: &mut AssetCache, case: &str, needle: &str) {
        self.refuse(&self.panorama_manifest(file), cache, case, needle);
    }
}
fn held(theme: &Theme) -> Arc<Panorama> {
    match &theme.sky {
        SkyAppearance::Panorama(image) => Arc::clone(image),
        _ => panic!("PANORAMA_PROFILE_RED panorama identity"),
    }
}
fn exact(theme: &Theme, w: u32, h: u32, rgba: &[u8]) {
    let image = held(theme);
    assert_eq!(
        (image.image().width, image.image().height),
        (w, h),
        "PANORAMA_PROFILE_RED shape"
    );
    assert!(
        image.image().rgba == rgba,
        "PANORAMA_PROFILE_RED source samples"
    );
    assert_eq!(image.image().rgba.len(), w as usize * h as usize * 4);
    assert!(image.image().rgba.capacity() <= 2_097_152);
    assert_eq!(w, h * 2);
    assert!(
        image
            .image()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 255)
    );
    assert!(
        theme.textures.is_empty(),
        "PANORAMA_PROFILE_RED sky spent world material slots"
    );
    assert!(
        !theme.load_failed,
        "PANORAMA_PROFILE_RED successful load flagged failed"
    );
    assert_eq!(
        theme.sky_feedback(),
        Some("SKY_STATUS LOADED SKY PANORAMA (static pack-local PNG)")
    );
    let feedback = theme.diagnostics.join("\n");
    assert!(
        !feedback.contains(TOKEN),
        "PANORAMA_PROFILE_RED source payload leaked to diagnostics"
    );
    assert!(
        !feedback.contains("profile.toml") && !feedback.contains(".png"),
        "PANORAMA_PROFILE_RED success diagnostics expose source path"
    );
}
fn failed_bytes(f: &Fixture, cache: &mut AssetCache, name: &str, bytes: &[u8], needle: &str) {
    f.write(name, bytes);
    f.refuse_file(name, cache, name, needle);
}

// Independent test-local IEC sRGB reference; never calls the production adapter.
fn composite_reference(samples: &[u8]) -> Vec<u8> {
    samples
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let alpha = f64::from(p[3]) / 255.0;
            let background = [0.04, 0.06, 0.05];
            let mut out = [0_u8; 4];
            for c in 0..3 {
                let source = f64::from(p[c]) / 255.0;
                let linear = if source <= 0.04045 {
                    source / 12.92
                } else {
                    ((source + 0.055) / 1.055).powf(2.4)
                };
                let value = linear * alpha + background[c] * (1.0 - alpha);
                let encoded = if value <= 0.0031308 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                };
                out[c] = (encoded * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            out[3] = 255;
            out
        })
        .collect()
}
fn admitted_adaptation(
    f: &Fixture,
    cache: &mut AssetCache,
    name: &str,
    bytes: &[u8],
    shape: (u32, u32),
    expected: &[u8],
    baseline: &Theme,
) -> Arc<Panorama> {
    let (w, h) = shape;
    f.write(name, bytes);
    let previous = held(baseline);
    let loaded = f.load(&f.panorama_manifest(name), cache, name);
    exact(&loaded, w, h, expected);
    assert_eq!(fs::read(f.pack.join(name)).unwrap(), bytes);
    cache.assert_sky_did_not_spend_model_cache();
    exact(baseline, 2, 1, &RGBA);
    assert!(Arc::ptr_eq(&previous, &held(baseline)));
    held(&loaded)
}

#[test]
#[ignore = "parent-only bounded CPU Theme load/profile gate; requires isolated output"]
fn theme_load_panorama_profile_and_held_source() {
    let f = Fixture::new();
    let mut cache = AssetCache::default();
    let rgb = encode(2, 1, ColorType::Rgb, BitDepth::Eight, &RGB);
    let rgba = encode(2, 1, ColorType::Rgba, BitDepth::Eight, &RGBA);
    decoded(&rgb, 2, 1, &RGBA);
    decoded(&rgba, 2, 1, &RGBA);
    f.write("sky/rgb.png", &rgb);
    f.write("sky/rgba.png", &rgba);
    assert!(
        read_local(&f.pack, "sky/rgb.png", 4_000_000).unwrap() == rgb,
        "SETUP actual filesystem reader"
    );

    // Real bundled v2 geometry/control, not just parsing a stripped manifest.
    let office = Theme::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/office.toml"),
        &mut AssetCache::default(),
    )
    .expect("SETUP inherited office load control");
    assert!(
        !office.load_failed && !office.vertices.is_empty(),
        "SETUP inherited office geometry"
    );
    assert_eq!(office.loaded_roles.len(), 3, "SETUP inherited roles");
    assert!(matches!(office.sky, SkyAppearance::Legacy));
    assert!(office.sky_feedback().is_none());
    for version in [2, 3] {
        let path = f.manifest(version, "");
        let theme = Theme::load(&path, &mut cache).expect("SETUP no-sky load control");
        assert!(matches!(theme.sky, SkyAppearance::Legacy));
        assert!(theme.sky_feedback().is_none() && !theme.load_failed);
        assert!(!theme.vertices.is_empty() && theme.textures.is_empty());
    }
    let colour = Theme::load(
        &f.manifest(3, "[sky]\nkind = 'colour'\nrgb = [0.0, 0.25, 1.0]"),
        &mut cache,
    )
    .expect("SETUP colour load control");
    assert_eq!(colour.sky, SkyAppearance::Colour([0.0, 0.25, 1.0]));
    assert_eq!(
        colour.sky_feedback(),
        Some("SKY_STATUS LOADED SKY COLOUR (static linear RGB)")
    );
    eprintln!("PANORAMA_PROFILE_PRECONTROLS_OK genuine_png_reader_v2_v3_colour");

    for name in ["sky/rgb.png", "sky/rgba.png"] {
        let theme = f.load(&f.panorama_manifest(name), &mut cache, name);
        exact(&theme, 2, 1, &RGBA);
        assert_eq!(
            cache.loads, 0,
            "PANORAMA_PROFILE_RED sky touched model cache loads"
        );
    }
    // Existing strict decoder admits opaque gray as well; do not invent RGB-only policy.
    for (name, color, pixels, expected) in [
        (
            "gray.png",
            ColorType::Grayscale,
            vec![19, 201],
            vec![19, 19, 19, 255, 201, 201, 201, 255],
        ),
        (
            "gray-alpha.png",
            ColorType::GrayscaleAlpha,
            vec![19, 255, 201, 255],
            vec![19, 19, 19, 255, 201, 201, 201, 255],
        ),
    ] {
        let bytes = encode(2, 1, color, BitDepth::Eight, &pixels);
        decoded(&bytes, 2, 1, &expected);
        f.write(name, &bytes);
        exact(
            &f.load(&f.panorama_manifest(name), &mut cache, name),
            2,
            1,
            &expected,
        );
    }
    let tagged = inserted(&rgb, *b"tEXt", format!("Note\0{TOKEN}").as_bytes());
    decoded(&tagged, 2, 1, &RGBA);
    f.write("tagged.png", &tagged);
    exact(
        &f.load(
            &f.panorama_manifest("tagged.png"),
            &mut cache,
            "metadata-private",
        ),
        2,
        1,
        &RGBA,
    );

    for (case, version, sky, needle) in [
        (
            "v2-panorama",
            2,
            "[sky]\nkind = 'panorama'\nfile = 'sky/rgb.png'",
            "version 3",
        ),
        (
            "v2-colour",
            2,
            "[sky]\nkind = 'colour'\nrgb = [0.0, 0.25, 1.0]",
            "version 3",
        ),
        (
            "v2-version-before-fields",
            2,
            "[sky]\nfile = 'missing.png'",
            "version 3",
        ),
        ("v1-not-upgraded", 1, "", "version 2 or 3"),
        ("v4-not-upgraded", 4, "", "version 2 or 3"),
        (
            "unknown-kind",
            3,
            "[sky]\nkind = 'hdr'\nfile = 'sky/rgb.png'",
            "SKY_REFUSED",
        ),
        (
            "kind-case-sensitive",
            3,
            "[sky]\nkind = 'Panorama'\nfile = 'sky/rgb.png'",
            "SKY_REFUSED",
        ),
        (
            "missing-kind",
            3,
            "[sky]\nfile = 'sky/rgb.png'",
            "SKY_REFUSED",
        ),
        ("missing-file", 3, "[sky]\nkind = 'panorama'", "SKY_REFUSED"),
        (
            "file-type",
            3,
            "[sky]\nkind = 'panorama'\nfile = 42",
            "SKY_REFUSED",
        ),
        (
            "extra-field",
            3,
            "[sky]\nkind = 'panorama'\nfile = 'sky/rgb.png'\nyaw = 1",
            "SKY_REFUSED",
        ),
        (
            "mixed-kind-fields",
            3,
            "[sky]\nkind = 'panorama'\nfile = 'sky/rgb.png'\nrgb = [0, 0, 0]",
            "SKY_REFUSED",
        ),
        (
            "colour-extra-file",
            3,
            "[sky]\nkind = 'colour'\nrgb = [0.0, 0.25, 1.0]\nfile = 'sky/rgb.png'",
            "SKY_REFUSED",
        ),
        (
            "colour-too-short",
            3,
            "[sky]\nkind = 'colour'\nrgb = [0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "colour-too-long",
            3,
            "[sky]\nkind = 'colour'\nrgb = [0.0, 0.0, 0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "colour-negative",
            3,
            "[sky]\nkind = 'colour'\nrgb = [-0.1, 0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "colour-above-one",
            3,
            "[sky]\nkind = 'colour'\nrgb = [1.1, 0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "colour-nan",
            3,
            "[sky]\nkind = 'colour'\nrgb = [nan, 0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "colour-infinite",
            3,
            "[sky]\nkind = 'colour'\nrgb = [inf, 0.0, 1.0]",
            "SKY_REFUSED",
        ),
        (
            "strict-top-field",
            3,
            "unexpected = 1\n[sky]\nkind = 'panorama'\nfile = 'sky/rgb.png'",
            "SKY_REFUSED",
        ),
    ] {
        f.refuse(&f.manifest(version, sky), &mut cache, case, needle);
    }
    eprintln!("PANORAMA_PROFILE_SCHEMA_OK");

    // Input now has independent axes/pixel caps; held output still enforces
    // exact 2:1 and <=1024x512. Preserve conforming passthrough identity.
    let baseline = f.load(
        &f.panorama_manifest("sky/rgb.png"),
        &mut cache,
        "adaptation-baseline",
    );
    let mut adapted_held = Vec::new();
    for (w, h) in [(1022_u32, 511_u32), (1024, 512)] {
        let pixels = [19, 37, 83, 255].repeat((w * h) as usize);
        let bytes = encode(w, h, ColorType::Rgba, BitDepth::Eight, &pixels);
        decoded(&bytes, w, h, &pixels);
        let name = format!("limit-{w}.png");
        f.write(&name, &bytes);
        let theme = f.load(
            &f.panorama_manifest(&name),
            &mut cache,
            "decoded-limit-positive",
        );
        exact(&theme, w, h, &pixels);
        assert_eq!(held(&theme).image().rgba.len(), (w * h * 4) as usize);
    }
    for (w, h) in [
        (4_u32, 1_u32),
        (1, 2),
        (1025, 512),
        (1024, 513),
        (1026, 513),
    ] {
        let pixels = [19, 37, 83, 255].repeat((w * h) as usize);
        let bytes = encode(w, h, ColorType::Rgba, BitDepth::Eight, &pixels);
        if w <= 1024 && h <= 1024 {
            decoded(&bytes, w, h, &pixels);
        } else {
            assert!(decode_local_png(&bytes).is_err());
        }
        let name = format!("dimensions-{w}-{h}.png");
        let expected = [19, 37, 83, 255].repeat(1024 * 512);
        let held = admitted_adaptation(
            &f,
            &mut cache,
            &name,
            &bytes,
            (1024, 512),
            &expected,
            &baseline,
        );
        adapted_held.push((held, expected, (1024, 512)));
    }
    for (w, h, needle) in [
        (0, 1, "sky PNG axes"),
        (2, 0, "sky PNG axes"),
        (u32::MAX, u32::MAX, "sky PNG axes"),
        (4097, 1, "sky PNG axes"),
        (1, 4097, "sky PNG axes"),
        (2049, 2048, "sky PNG pixels"),
    ] {
        failed_bytes(
            &f,
            &mut cache,
            &format!("header-{w}-{h}.png"),
            &header_dimensions(&rgb, w, h),
            needle,
        );
    }
    eprintln!("PANORAMA_PROFILE_DIMENSIONS_OK");

    let mut transparent = RGBA;
    transparent[7] = 254;
    let alpha = encode(2, 1, ColorType::Rgba, BitDepth::Eight, &transparent);
    decoded(&alpha, 2, 1, &transparent);
    let alpha_expected = composite_reference(&transparent);
    let alpha_held = admitted_adaptation(
        &f,
        &mut cache,
        "alpha.png",
        &alpha,
        (2, 1),
        &alpha_expected,
        &baseline,
    );
    adapted_held.push((alpha_held, alpha_expected, (2, 1)));
    let trns = inserted(&rgb, *b"tRNS", &[0, 19, 0, 37, 0, 83]);
    let trns_straight = [19, 37, 83, 0, 201, 7, 129, 255];
    decoded(&trns, 2, 1, &trns_straight);
    let trns_expected = composite_reference(&trns_straight);
    let trns_held = admitted_adaptation(
        &f,
        &mut cache,
        "trns.png",
        &trns,
        (2, 1),
        &trns_expected,
        &baseline,
    );
    adapted_held.push((trns_held, trns_expected, (2, 1)));
    let nonmatching = inserted(&rgb, *b"tRNS", &[0, 1, 0, 2, 0, 3]);
    decoded(&nonmatching, 2, 1, &RGBA);
    f.write("nonmatching-trns.png", &nonmatching);
    exact(
        &f.load(
            &f.panorama_manifest("nonmatching-trns.png"),
            &mut cache,
            "opaque-trns",
        ),
        2,
        1,
        &RGBA,
    );
    let indexed = encode(2, 1, ColorType::Indexed, BitDepth::Eight, &[0, 1]);
    assert!(decode_local_png(&indexed).is_err());
    let indexed_held = admitted_adaptation(
        &f,
        &mut cache,
        "indexed.png",
        &indexed,
        (2, 1),
        &RGBA,
        &baseline,
    );
    adapted_held.push((indexed_held, RGBA.to_vec(), (2, 1)));
    // Insert after the actual PLTE, not before it: palette tRNS must be ordered.
    let plte_len = u32::from_be_bytes(indexed[33..37].try_into().unwrap()) as usize;
    assert_eq!(&indexed[37..41], b"PLTE");
    let plte_end = 45 + plte_len;
    let mut indexed_alpha = indexed[..plte_end].to_vec();
    indexed_alpha.extend(chunk(*b"tRNS", &[0, 128]));
    indexed_alpha.extend_from_slice(&indexed[plte_end..]);
    assert!(decode_local_png(&indexed_alpha).is_err());
    let indexed_expected = composite_reference(&[19, 37, 83, 0, 201, 7, 129, 128]);
    let indexed_alpha_held = admitted_adaptation(
        &f,
        &mut cache,
        "indexed-alpha.png",
        &indexed_alpha,
        (2, 1),
        &indexed_expected,
        &baseline,
    );
    adapted_held.push((indexed_alpha_held, indexed_expected, (2, 1)));
    failed_bytes(
        &f,
        &mut cache,
        "sixteen.png",
        &encode(2, 1, ColorType::Rgb, BitDepth::Sixteen, &[37; 12]),
        "no indexed/16-bit conversion",
    );
    for (name, kind, payload, needle) in [
        (
            "apng.png",
            *b"acTL",
            vec![0, 0, 0, 1, 0, 0, 0, 0],
            "animated PNG",
        ),
        (
            "gamma.png",
            *b"gAMA",
            100000_u32.to_be_bytes().to_vec(),
            "non-sRGB gamma",
        ),
        (
            "orientation.png",
            *b"eXIf",
            vec![0; 8],
            "profile/orientation",
        ),
        ("icc.png", *b"iCCP", vec![0; 8], "profile/orientation"),
    ] {
        failed_bytes(
            &f,
            &mut cache,
            name,
            &inserted(&rgb, kind, &payload),
            needle,
        );
    }
    for (name, bytes) in [
        ("not-png.png", b"not a PNG".to_vec()),
        ("truncated.png", rgb[..rgb.len() - 1].to_vec()),
        ("missing-end.png", rgb[..rgb.len() - 12].to_vec()),
        ("header-crc.png", {
            let mut b = rgb.clone();
            b[29] ^= 1;
            b
        }),
        ("idat-crc.png", {
            let mut b = rgb.clone();
            assert_eq!(&b[37..41], b"IDAT", "SETUP IDAT offset");
            let len = u32::from_be_bytes(b[33..37].try_into().unwrap()) as usize;
            b[41 + len] ^= 1;
            b
        }),
        ("ancillary-crc.png", {
            let mut b = inserted(&rgb, *b"vpAg", &[7]);
            b[42] ^= 1;
            b
        }),
        ("corrupt-deflate.png", {
            let len = u32::from_be_bytes(rgb[33..37].try_into().unwrap()) as usize;
            let mut payload = rgb[41..41 + len].to_vec();
            payload[0] = 0; // invalid zlib method with recalculated chunk CRC
            let mut b = rgb[..33].to_vec();
            b.extend(chunk(*b"IDAT", &payload));
            b.extend_from_slice(&rgb[45 + len..]);
            b
        }),
    ] {
        assert!(
            decode_local_png(&bytes).is_err(),
            "SETUP malformed probe must really be invalid"
        );
        failed_bytes(&f, &mut cache, name, &bytes, "SKY_REFUSED");
    }
    eprintln!("PANORAMA_PROFILE_STRICT_PNG_OK");

    let outside = f.root.join("outside.png");
    fs::write(&outside, &rgb).expect("SETUP owned escape target");
    for (case, file) in [
        ("url", "https://example.invalid/source.png"),
        ("file-url", "file:///owned.png"),
        ("absolute", outside.to_str().unwrap()),
        ("traversal", "../outside.png"),
        ("internal-traversal", "sky/../sky/rgb.png"),
        ("dot-prefix", "./sky/rgb.png"),
        ("backslash", "sky\\rgb.png"),
        ("colon", "sky:rgb.png"),
        ("empty", ""),
        ("wrong-extension", "sky/rgb.jpg"),
        ("extension-case", "sky/rgb.PNG"),
        ("nul-component", "sky/nu\0l.png"),
        ("missing-file", "missing.png"),
    ] {
        f.refuse_file(file, &mut cache, case, "SKY_REFUSED");
    }
    let eight = "a/b/c/d/e/f/g/eight.png";
    f.write(eight, &rgb);
    exact(
        &f.load(&f.panorama_manifest(eight), &mut cache, "eight-components"),
        2,
        1,
        &RGBA,
    );
    let nine = "a/b/c/d/e/f/g/h/nine.png";
    f.write(nine, &rgb);
    f.refuse_file(nine, &mut cache, "nine-components", "bounded pack-local");
    symlink("sky/rgb.png", f.pack.join("inside-link.png")).expect("SETUP inside symlink");
    symlink(&outside, f.pack.join("outside-link.png")).expect("SETUP outside symlink");
    symlink("sky", f.pack.join("inside-dir")).expect("SETUP inside dir symlink");
    symlink(&f.root, f.pack.join("outside-dir")).expect("SETUP outside dir symlink");
    for file in [
        "inside-link.png",
        "outside-link.png",
        "inside-dir/rgb.png",
        "outside-dir/outside.png",
    ] {
        f.refuse_file(file, &mut cache, "nofollow-chain", "SKY_REFUSED");
    }
    fs::create_dir(f.pack.join("directory.png")).expect("SETUP nonregular directory");
    f.refuse_file(
        "directory.png",
        &mut cache,
        "directory-not-file",
        "must be regular",
    );
    let fifo = f.pack.join("fifo.png");
    let cpath = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // Actual owned FIFO, not a parser/read double. Reader must refuse without blocking.
    assert_eq!(
        unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) },
        0,
        "SETUP mkfifo"
    );
    assert!(
        fs::symlink_metadata(&fifo).unwrap().file_type().is_fifo(),
        "SETUP actual FIFO"
    );
    f.refuse_file("fifo.png", &mut cache, "fifo-not-file", "must be regular");

    // Ordinary encoder-built bounded pixel fixtures here do not approach 4 MB.
    // Probe the actual acquisition threshold separately from strict PNG validity:
    // at/below read succeeds but trailing data refuses; above refuses at file cap.
    for len in [3_999_999_usize, 4_000_000, 4_000_001] {
        let name = format!("encoded-{len}.png");
        let mut bytes = rgb.clone();
        bytes.resize(len, 0);
        f.write(&name, &bytes);
        let read = read_local(&f.pack, &name, 4_000_000);
        if len <= 4_000_000 {
            assert!(
                read.as_ref()
                    .is_ok_and(|b| b.len() == len && b.as_slice() == bytes.as_slice()),
                "PANORAMA_PROFILE_RED encoded reader boundary"
            );
            f.refuse_file(
                &name,
                &mut cache,
                "encoded-at-or-below",
                "IEND/trailing bytes",
            );
        } else {
            assert!(read.is_err(), "PANORAMA_PROFILE_RED encoded reader cap");
            f.refuse_file(&name, &mut cache, "encoded-over", "<= 4000000 bytes");
        }
    }
    eprintln!("PANORAMA_PROFILE_FILESYSTEM_OK");

    let path = f.panorama_manifest("sky/rgb.png");
    let original = f.load(&path, &mut cache, "held-original");
    let image = held(&original);
    let clone = original.sky.clone();
    assert!(matches!(&clone, SkyAppearance::Panorama(p) if Arc::ptr_eq(p, &image)));
    let changed_samples = [201, 7, 129, 255, 19, 37, 83, 255];
    let changed = encode(2, 1, ColorType::Rgba, BitDepth::Eight, &changed_samples);
    decoded(&changed, 2, 1, &changed_samples);
    fs::write(f.pack.join("sky/rgb.png"), &changed).expect("SETUP replace source bytes");
    exact(&original, 2, 1, &RGBA);
    let fresh = f.load(&path, &mut cache, "fresh-load-acquires-new-bytes");
    exact(&fresh, 2, 1, &changed_samples);
    assert!(!Arc::ptr_eq(&held(&fresh), &image));
    fs::write(f.pack.join("sky/rgb.png"), b"owned corrupt replacement")
        .expect("SETUP corrupt source");
    f.refuse(
        &path,
        &mut cache,
        "fresh-load-corrupt-refuses",
        "SKY_REFUSED",
    );
    exact(&original, 2, 1, &RGBA);
    exact(&fresh, 2, 1, &changed_samples);
    fs::remove_file(f.pack.join("sky/rgb.png")).expect("SETUP delete source");
    f.refuse(
        &path,
        &mut cache,
        "fresh-load-missing-refuses",
        "SKY_REFUSED",
    );
    symlink(&outside, f.pack.join("sky/rgb.png")).expect("SETUP source replaced by symlink");
    f.refuse(
        &path,
        &mut cache,
        "fresh-load-symlink-refuses",
        "SKY_REFUSED",
    );
    for _ in 0..4 {
        original
            .sky
            .validate()
            .expect("PANORAMA_PROFILE_RED held validation reopened source");
        clone
            .validate()
            .expect("PANORAMA_PROFILE_RED clone lost held validity");
        exact(&original, 2, 1, &RGBA);
        exact(&fresh, 2, 1, &changed_samples);
    }
    drop(original);
    assert!(
        image.image().rgba == RGBA,
        "PANORAMA_PROFILE_RED held resource lost after owner drop"
    );
    assert_eq!(cache.loads, 0, "PANORAMA_PROFILE_RED model cache polluted");
    for ((resource, expected, shape), name) in adapted_held.iter().zip([
        "dimensions-4-1.png",
        "dimensions-1-2.png",
        "dimensions-1025-512.png",
        "dimensions-1024-513.png",
        "dimensions-1026-513.png",
        "alpha.png",
        "trns.png",
        "indexed.png",
        "indexed-alpha.png",
    ]) {
        fs::rename(f.pack.join(name), f.pack.join(format!("retained-{name}"))).unwrap();
        fs::write(f.pack.join(name), b"owned corrupt adapted replacement").unwrap();
        assert!(Theme::load(&f.panorama_manifest(name), &mut cache).is_err());
        let cloned = Arc::clone(resource);
        assert!(Arc::ptr_eq(resource, &cloned));
        SkyAppearance::Panorama(cloned).validate().unwrap();
        assert_eq!((resource.image().width, resource.image().height), *shape);
        assert_eq!(&resource.image().rgba, expected);
    }
    assert_eq!(adapted_held.len(), 9);
    cache.assert_sky_did_not_spend_model_cache();
    eprintln!("PANORAMA_PROFILE_HELD_SOURCE_OK");
    // Fixture retained, including source replacements and nonregular probes.
    eprintln!("PANORAMA_PROFILE_PASS CPU_THEME_LOAD_ONLY");
}
