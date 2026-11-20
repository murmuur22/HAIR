//! Prepared CPU tests; real png encoder and strict decoder, not injected textures.
use super::*;

fn png_bytes(w: u32, h: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    bytes
}
fn options(fit: PanoramaFit) -> PanoramaOptions {
    PanoramaOptions {
        fit,
        background_linear: [0.0; 3],
    }
}
fn pixel(image: &Panorama, x: usize, y: usize) -> [u8; 4] {
    let i = (y * image.image().width as usize + x) * 4;
    image.image().rgba[i..i + 4].try_into().unwrap()
}
fn bounded(image: &Panorama) {
    let image = image.image();
    assert!(image.width > 0 && image.width <= 1024);
    assert!(image.height > 0 && image.height <= 512);
    assert_eq!(image.width, image.height * 2);
    assert_eq!(
        image.rgba.len(),
        image.width as usize * image.height as usize * 4
    );
    assert!(image.rgba.len() <= 2_097_152);
    assert!(image.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}

#[test]
fn ordinary_non_two_to_one_png_adapts_without_mutating_encoded_source() {
    let source = png_bytes(6, 4, &[19, 37, 83, 255].repeat(24));
    let original = source.clone();
    let decoded = crate::assets::decode_local_png(&source).unwrap();
    assert_eq!((decoded.width, decoded.height), (6, 4));
    let image = adapt_png(&source, PanoramaOptions::default()).unwrap();
    bounded(&image);
    assert_eq!((image.image().width, image.image().height), (1024, 512));
    assert!(
        image
            .image()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [19, 37, 83, 255])
    );
    assert_eq!(source, original);
}

#[test]
fn sky_admits_1200_by_673_without_widening_local_png() {
    let source = png_bytes(1200, 673, &[70, 120, 190, 255].repeat(1200 * 673));
    assert!(crate::assets::decode_local_png(&source).is_err());
    let image = adapt_png(&source, PanoramaOptions::default()).unwrap();
    bounded(&image);
    assert_eq!(pixel(&image, 512, 256), [70, 120, 190, 255]);
}

#[test]
fn opaque_conforming_png_is_exact_pixel_identity_for_all_modes() {
    let pixels: Vec<u8> = (0..32)
        .flat_map(|i| [i * 7, 255 - i * 5, i * 3, 255])
        .collect();
    let source = png_bytes(8, 4, &pixels);
    for fit in [PanoramaFit::Fill, PanoramaFit::Fit, PanoramaFit::Stretch] {
        let image = adapt_png(&source, options(fit)).unwrap();
        assert_eq!((image.image().width, image.image().height), (8, 4));
        assert_eq!(image.image().rgba, pixels);
    }
    let source = png_bytes(1024, 512, &[1, 254, 83, 255].repeat(1024 * 512));
    let image = adapt_png(&source, PanoramaOptions::default()).unwrap();
    bounded(&image);
    assert_eq!(image.image().rgba, [1, 254, 83, 255].repeat(1024 * 512));
}

#[test]
fn fill_center_crops_while_fit_pads_and_stretch_keeps_extremes() {
    // 8x2: Fill keeps centered columns 2..6; Fit pads vertically; Stretch keeps all.
    let row: Vec<u8> = (0..8)
        .flat_map(|x| match x {
            0 | 1 => [255, 0, 0, 255],
            6 | 7 => [0, 0, 255, 255],
            _ => [0, 255, 0, 255],
        })
        .collect();
    let source = png_bytes(8, 2, &row.repeat(2));
    let fill = adapt_png(&source, options(PanoramaFit::Fill)).unwrap();
    let fit = adapt_png(&source, options(PanoramaFit::Fit)).unwrap();
    let stretch = adapt_png(&source, options(PanoramaFit::Stretch)).unwrap();
    for image in [&fill, &fit, &stretch] {
        bounded(image);
    }
    // Avoid the bilinear boundary ramps: these centers are strictly inside landmarks.
    assert_eq!(pixel(&fill, 256, 256), [0, 255, 0, 255]);
    assert_eq!(pixel(&fill, 640, 256), [0, 255, 0, 255]);
    assert_eq!(pixel(&fit, 128, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&fit, 128, 256), [255, 0, 0, 255]);
    assert_eq!(pixel(&fit, 896, 256), [0, 0, 255, 255]);
    assert_eq!(pixel(&stretch, 128, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&stretch, 896, 511), [0, 0, 255, 255]);
}

#[test]
fn tall_fill_crops_vertical_center_and_fit_pads_horizontally() {
    let pixels: Vec<u8> = (0..8)
        .flat_map(|y| {
            let p = if (2..6).contains(&y) {
                [0, 255, 0, 255]
            } else {
                [255, 0, 0, 255]
            };
            p.repeat(2)
        })
        .collect();
    let source = png_bytes(2, 8, &pixels);
    let fill = adapt_png(&source, options(PanoramaFit::Fill)).unwrap();
    assert_eq!(pixel(&fill, 512, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&fill, 512, 511), [0, 255, 0, 255]);
    let fit = adapt_png(&source, options(PanoramaFit::Fit)).unwrap();
    assert_eq!(pixel(&fit, 0, 256), [0, 0, 0, 255]);
    assert_eq!(pixel(&fit, 512, 256), [0, 255, 0, 255]);
    let stretch = adapt_png(&source, options(PanoramaFit::Stretch)).unwrap();
    assert_eq!(pixel(&stretch, 512, 0), [255, 0, 0, 255]);
}

#[test]
fn poisoned_transparent_rgb_has_no_fringe_and_composition_is_linear() {
    let poisoned = png_bytes(2, 2, &[255, 0, 255, 0, 0, 255, 0, 255].repeat(2));
    let clean = png_bytes(2, 2, &[0, 0, 0, 0, 0, 255, 0, 255].repeat(2));
    for fit in [PanoramaFit::Fill, PanoramaFit::Fit, PanoramaFit::Stretch] {
        let a = adapt_png(&poisoned, options(fit)).unwrap();
        let b = adapt_png(&clean, options(fit)).unwrap();
        assert_eq!(a, b);
        assert!(
            a.image()
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == 0 && p[2] == 0 && p[3] == 255)
        );
    }
    let source = png_bytes(2, 1, &[255, 255, 255, 128, 255, 0, 255, 0]);
    let image = adapt_png(&source, options(PanoramaFit::Fill)).unwrap();
    assert_eq!(pixel(&image, 0, 0), [188, 188, 188, 255]);
    assert_eq!(pixel(&image, 1, 0), [0, 0, 0, 255]);
    let background = PanoramaOptions {
        fit: PanoramaFit::Fit,
        background_linear: [0.25, 0.0, 1.0],
    };
    let transparent = png_bytes(1, 1, &[255, 255, 0, 0]);
    let image = adapt_png(&transparent, background).unwrap();
    assert!(
        image
            .image()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [137, 0, 255, 255])
    );
}

#[test]
fn bilinear_interpolation_is_in_linear_light_not_srgb_bytes() {
    let source = png_bytes(2, 2, &[0, 0, 0, 255, 255, 255, 255, 255].repeat(2));
    let image = adapt_png(&source, options(PanoramaFit::Stretch)).unwrap();
    let mid = pixel(&image, 511, 256);
    assert!(
        (187..=188).contains(&mid[0]),
        "linear-light half black/white: {mid:?}"
    );
    assert_eq!(mid[0], mid[1]);
    assert_eq!(mid[0], mid[2]);
}

#[test]
fn singleton_narrow_tall_and_maximum_pixel_inputs_have_bounded_output() {
    for (w, h) in [(1, 1), (1, 4096), (4096, 1), (2048, 2048), (4096, 1024)] {
        let source = png_bytes(w, h, &[17, 31, 71, 255].repeat(w as usize * h as usize));
        for fit in [PanoramaFit::Fill, PanoramaFit::Fit, PanoramaFit::Stretch] {
            bounded(&adapt_png(&source, options(fit)).unwrap());
        }
    }
}

#[test]
fn invalid_linear_background_refuses_even_on_passthrough() {
    let source = png_bytes(2, 1, &[1, 2, 3, 255].repeat(2));
    for value in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(
            adapt_png(
                &source,
                PanoramaOptions {
                    background_linear: [value, 0.0, 0.0],
                    ..PanoramaOptions::default()
                }
            )
            .is_err()
        );
    }
    assert_eq!(PanoramaOptions::default().fit, PanoramaFit::Fill);
    assert_eq!(
        PanoramaOptions::default().background_linear,
        [0.04, 0.06, 0.05]
    );
}

#[test]
fn private_panorama_constructor_retains_output_shape_length_and_alpha_invariant() {
    use crate::assets::Texture;
    // Constructor refusal controls only; these are not claimed decoder outputs.
    for image in [
        Texture {
            width: 0,
            height: 1,
            rgba: Vec::new(),
        },
        Texture {
            width: 6,
            height: 4,
            rgba: vec![255; 6 * 4 * 4],
        },
        Texture {
            width: 1026,
            height: 513,
            rgba: Vec::new(),
        },
        Texture {
            width: 2,
            height: 1,
            rgba: vec![255; 7],
        },
        Texture {
            width: 2,
            height: 1,
            rgba: vec![0; 8],
        },
        Texture {
            width: u32::MAX,
            height: u32::MAX,
            rgba: Vec::new(),
        },
    ] {
        assert!(Panorama::from_image(image).is_err());
    }
}

#[test]
#[ignore = "parent-only descriptor/source proof; explicit empty external HAIR_PANORAMA_ADAPT_CPU_OUTPUT"]
fn runtime_load_preserves_source_and_path_file_guards() {
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(
        std::env::var_os("HAIR_PANORAMA_ADAPT_CPU_OUTPUT").expect("explicit owned output"),
    )
    .canonicalize()
    .unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    assert!(!root.starts_with(&source) && !source.starts_with(&root));
    assert!(fs::read_dir(&root).unwrap().next().is_none());
    let bytes = png_bytes(6, 4, &[19, 37, 83, 128].repeat(24));
    fs::write(root.join("ordinary.png"), &bytes).unwrap();
    let loaded = SkySpec::Panorama {
        file: "ordinary.png".into(),
    }
    .load(&root)
    .unwrap();
    let SkyAppearance::Panorama(held) = loaded else {
        panic!("not panorama")
    };
    bounded(&held);
    assert_eq!(fs::read(root.join("ordinary.png")).unwrap(), bytes);
    let expected = adapt_png(&bytes, PanoramaOptions::default()).unwrap();
    assert_eq!(*held, expected);
    let cloned = Arc::clone(&held);
    fs::rename(
        root.join("ordinary.png"),
        root.join("retained-original.png"),
    )
    .unwrap();
    fs::write(root.join("ordinary.png"), b"not png").unwrap();
    assert!(Arc::ptr_eq(&held, &cloned));
    assert_eq!(*cloned, expected);
    assert_eq!(fs::read(root.join("retained-original.png")).unwrap(), bytes);
    for file in [
        "../retained-original.png",
        "/ordinary.png",
        "a\\b.png",
        "a:b.png",
        "ordinary.PNG",
        "a/b/c/d/e/f/g/h/i.png",
        "missing.png",
        "ordinary.png",
    ] {
        assert!(
            SkySpec::Panorama { file: file.into() }.load(&root).is_err(),
            "accepted {file}"
        );
    }
    fs::create_dir(root.join("directory.png")).unwrap();
    assert!(
        SkySpec::Panorama {
            file: "directory.png".into()
        }
        .load(&root)
        .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(root.join("retained-original.png"), root.join("link.png")).unwrap();
        assert!(
            SkySpec::Panorama {
                file: "link.png".into()
            }
            .load(&root)
            .is_err()
        );
        symlink(&root, root.join("linked-directory")).unwrap();
        assert!(
            SkySpec::Panorama {
                file: "linked-directory/retained-original.png".into()
            }
            .load(&root)
            .is_err()
        );
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let fifo = CString::new(root.join("fifo.png").as_os_str().as_bytes()).unwrap();
        // SAFETY: owned path is NUL-terminated and only this explicit fixture is created.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(
            SkySpec::Panorama {
                file: "fifo.png".into()
            }
            .load(&root)
            .is_err()
        );
    }
    let oversized = fs::File::create(root.join("oversized.png")).unwrap();
    oversized.set_len(4_000_001).unwrap();
    assert!(
        SkySpec::Panorama {
            file: "oversized.png".into()
        }
        .load(&root)
        .is_err()
    );
}
