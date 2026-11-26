// Disposable owned fixtures only; checked resize is distinct from relocation.
include!("r2b_qualification_tests.rs");
include!("r2c_tests.rs");
#[test]
fn r2b_scale_schema_legacy_strict_values_and_original_reference() {
    use crate::placement::scaling::Scaling;
    let legacy = pos();
    assert_eq!(legacy.properties.scale_percent(),100);
    let text = toml::to_string(&legacy).unwrap(); assert!(!text.contains("scaling"));
    assert_eq!(toml::from_str::<Position>(&text).unwrap(),legacy);
    for percent in crate::placement::scaling::STOPS {
        let s = Scaling::new(legacy.size,percent).unwrap();
        let mut p = legacy.clone(); p.size = s.size().unwrap(); p.properties.scaling = Some(s);
        p.validate().unwrap();
        assert_eq!(toml::from_str::<Position>(&toml::to_string(&p).unwrap()).unwrap(),p);
        p.size[0] += 0.1; assert!(p.validate().is_err());
    }
    for n in [0,49,51,74,76,99,101,149,151,u16::MAX] { assert!(Scaling::new(legacy.size,n).is_err()); }
    for n in [f32::NAN,f32::INFINITY,0.,-1.,17.] { assert!(Scaling::new([n,1.,1.],100).is_err()); }
    let old = slice5a_decoration([-2.,0.]);
    let mut proposed = old.clone(); proposed.set_scale(50).unwrap(); proposed.set_scale(150).unwrap();
    assert_eq!(proposed.size,old.size.map(|n|n*1.5));
    assert!(DecorationResize::new(&old,&proposed).is_ok());
    assert!(old.validate_relocation(&proposed).is_err());
    let mut wrong = proposed.clone(); wrong.tag = "hair.other".into(); assert!(DecorationResize::new(&old,&wrong).is_err());
    wrong = proposed.clone(); wrong.placement_id = "ffffffffffffffffffffffffffffffff".into(); assert!(DecorationResize::new(&old,&wrong).is_err());
    wrong = proposed.clone(); wrong.scaling.as_mut().unwrap().reference[0] = 1.; wrong.size = wrong.scaling.unwrap().size().unwrap();
    assert!(DecorationResize::new(&old,&wrong).is_err());
    proposed.set_scale(100).unwrap(); assert_eq!(proposed.size,old.size);
}
#[test]
fn r2b_resize_two_real_locked_stages_refusal_recovery_stale_and_listing() {
    for kind in ["success","stale","busy","first","late","listing","record","sync"] {
        let (f,e,old,_) = slice5a_saved();
        std::fs::write(f.0.join("original.txt"),b"never resized").unwrap();
        let originals = slice5a_snapshot(&f.0);
        let active = std::fs::read(slice5a_record(&f,&old)).unwrap();
        let mut proposed = old.clone(); proposed.set_scale(75).unwrap();
        let mut expected = old.clone(); if kind=="stale" { expected.yaw=0.2; }
        let intent = DecorationResize::new(&expected,&proposed).unwrap();
        let busy = std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
        use std::os::fd::AsRawFd;
        if kind=="busy" { assert_eq!(unsafe{libc::flock(busy.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0); }
        let mut calls=0;
        let out=mac::update_decoration(&e,&intent.expected,&intent.proposed,true,&mut |candidate| {
            calls+=1; assert!(read_layout(&e).unwrap_err().contains("busy"));
            crate::placement_scene::validate_decoration_candidate(&e,candidate,[0.,2.],0)?;
            if kind=="first" || kind=="late" && calls==2 { return Err("checked resize refused".into()); }
            if kind=="listing" && calls==2 { std::fs::write(f.0.join("new.txt"),b"late destination").unwrap(); }
            if kind=="record" && calls==2 { std::fs::write(slice5a_record(&f,&old),toml::to_string(&proposed).unwrap()).unwrap(); }
            Ok(())
        },|s| if kind=="sync" && s==2 {Err("sync seam".into())}else{Ok(())});
        if kind=="busy" { assert_eq!(unsafe{libc::flock(busy.as_raw_fd(),libc::LOCK_UN)},0); }
        let live=read_layout(&Explorer::new(&f.0).unwrap()).unwrap();
        if kind=="success" || kind=="sync" {
            assert_eq!(calls,2); assert_eq!(out,Outcome::Committed{synced:kind=="success"}); assert_eq!(live.decorations,vec![proposed]);
        } else {
            assert!(matches!(out,Outcome::NotCommitted(_)),"{kind}: {out:?}");
            if kind!="record" { assert_eq!(std::fs::read(slice5a_record(&f,&old)).unwrap(),active); assert_eq!(live.decorations,vec![old.clone()]); }
            if ["late","listing","record"].contains(&kind) {
                assert_eq!(live.incomplete_records,1);
                assert!(inspect_placements(&e).unwrap().select_decoration_resume(0).is_err(),"resize pending never silently becomes relocation authority");
                let snapshot=slice5a_snapshot(&f.0);
                assert!(matches!(resize_decoration_checked(&e,&intent,|_|panic!("recovery bypass")),Outcome::NotCommitted(_)));
                assert_eq!(slice5a_snapshot(&f.0),snapshot);
            } else { assert_eq!(slice5a_snapshot(&f.0),originals); }
        }
        assert_eq!(std::fs::read(f.0.join("original.txt")).unwrap(),b"never resized");
    }
}
#[test]
fn r2b_resize_rejects_byte_identical_pin_rebinding_between_locked_stages() {
    use std::os::unix::fs::PermissionsExt;
    for stage in [1, 2] {
        let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
        let (_, theme) = slice6a_pack(&f);
        let source = select_pin_asset(&theme, "chair").unwrap();
        let follow = slice5a_decoration([-2.,0.]);
        assert_eq!(theme.publish_pin_checked(&e, &source, &follow, |_|Ok(())), Outcome::Committed { synced:true });
        let old = read_layout(&e).unwrap().decorations[0].clone();
        let mut proposed = old.clone(); proposed.set_scale(75).unwrap();
        let path = f.0.join(format!(".hair-layout/ac-{}/asset.glb", old.closure_identity().unwrap()));
        let active = std::fs::read(slice5a_record(&f,&old)).unwrap();
        let rebind = || {
            let bytes = std::fs::read(&path).unwrap();
            std::fs::rename(&path,f.0.join("displaced-asset")).unwrap();
            std::fs::write(&path,bytes).unwrap();
            std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        let mut calls = 0;
        let out = mac::update_decoration(&e,&old,&proposed,true,&mut |candidate| {
            calls+=1;
            assert!(read_layout(&e).unwrap_err().contains("busy"));
            theme.validate_decoration_candidate_layout(candidate)?;
            if stage==2 && calls==2 { rebind(); }
            Ok(())
        }, |s| { if stage==1 && s==1 { rebind(); } Ok(()) });
        assert!(matches!(out,Outcome::NotCommitted(_)),"byte-identical pin rebind stage={stage}: {out:?}");
        assert_eq!(std::fs::read(slice5a_record(&f,&old)).unwrap(),active);
    }
}
