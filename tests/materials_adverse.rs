use hair::{authoring::Project, theme::Theme};
use std::fs;
#[test]
fn malformed_png_dimension_encoded_crc_and_warm_cache_declarations_refuse_last_good() {
    let d = std::env::temp_dir().join(format!("hair-png-adverse-{}", std::process::id()));
    fs::create_dir(&d).unwrap();
    let path = d.join("image.png");
    let png = |w, h| {
        let mut b = Vec::new();
        {
            let mut e = png::Encoder::new(&mut b, w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut out = e.write_header().unwrap();
            out.write_image_data(&vec![255; (w * h * 4) as usize])
                .unwrap();
            out.finish().unwrap();
        }
        b
    };
    let good = png(1, 1);
    fs::write(&path, &good).unwrap();
    let mut p = Project::blank();
    p.import_surface("wall", &path).unwrap();
    let before = p.surfaces();
    let preview = bytemuck::cast_slice::<_, u8>(&p.preview().unwrap().vertices).to_vec();
    let mut corrupt = good.clone();
    corrupt[45] ^= 1;
    for bad in [
        b"not PNG".to_vec(),
        good[..24].to_vec(),
        corrupt,
        png(1025, 1),
        png(1, 1025),
        vec![0; 4_000_001],
    ] {
        fs::write(&path, &bad).unwrap();
        assert!(p.import_surface("wall", &path).is_err());
        assert_eq!(p.surfaces(), before);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&p.preview().unwrap().vertices),
            preview
        );
    }
    fs::write(&path, &good).unwrap();
    let manifest = "version=3\nname='all declarations'\nroles=[]\n[[surfaces]]\nsurface='wall'\nfile='image.png'\ntint=[1,1,1]\nmeters_per_tile=1.0\n[[surfaces]]\nsurface='floor'\nfile='image.png'\ntint=[1,1,1]\nmeters_per_tile=1.0\n";
    let pack = d.join("pack.toml");
    fs::write(&pack, manifest).unwrap();
    let mut cache = Default::default();
    Theme::load(&pack, &mut cache).unwrap();
    fs::write(&path, b"bad").unwrap();
    assert!(Theme::load(&pack, &mut cache).is_err());
    fs::remove_dir_all(d).unwrap();
}
