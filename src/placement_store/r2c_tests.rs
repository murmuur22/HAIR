// Strict schema, bounded visual motion, checked mutation and retained evidence.
#[test]
fn decoration_animation_guard_public_intent_refuses_all_modes_and_clearing() {
    use crate::placement::animation::Mode;
    for saved in [None,Some(Mode::None),Some(Mode::Breathe),Some(Mode::BreatheRotate)] {
        let mut old=slice5a_decoration([-2.,0.]);old.animation=saved;
        old.validate().unwrap(); // LEGACY schema remains valid.
        for animation in [None,Some(Mode::None),Some(Mode::Breathe),Some(Mode::BreatheRotate)] {
            let mut proposed=old.clone();proposed.animation=animation;
            assert!(old.validate_animation(&proposed).is_err(),"accepted {saved:?} -> {animation:?}");
            assert!(DecorationAnimation::new(&old,&proposed).is_err());
        }
    }
}
#[test]
fn decoration_animation_guard_new_pin_refuses_explicit_animation() {
    use crate::placement::animation::Mode;
    let f=Fixture::new();let (_,theme)=slice6a_pack(&f);let source=select_pin_asset(&theme,"chair").unwrap();
    let mut d=slice5a_decoration([-2.,0.]);
    assert!(source.decoration(&d).is_ok());
    for mode in [Mode::None,Mode::Breathe,Mode::BreatheRotate] {
        d.animation=Some(mode);d.validate().unwrap();
        assert!(source.decoration(&d).is_err(),"new pin accepted {mode:?}");
        let e=Explorer::new(&f.0).unwrap();let before=slice5a_snapshot(&f.0);
        let out=publish_pin_checked(&e,&source,&d,|_|panic!("animated pin reached publication callback"));
        assert!(matches!(out,Outcome::NotCommitted(_)));assert_eq!(slice5a_snapshot(&f.0),before);
    }
}
#[test]
fn decoration_animation_guard_new_follow_publication_refuses_explicit_animation() {
    use crate::placement::animation::Mode;
    for mode in [Mode::None,Mode::Breathe,Mode::BreatheRotate] {
        let f=Fixture::new();let e=Explorer::new(&f.0).unwrap();
        let mut d=slice5a_decoration([-2.,0.]);d.animation=Some(mode);
        let mut calls=0;
        let out=place_decoration_checked(&e,&d,|_|{calls+=1;Ok(())});
        assert!(matches!(out,Outcome::NotCommitted(_)),"new Follow accepted {mode:?}: {out:?}");
        assert_eq!(calls,0);assert!(!f.0.join(".hair-layout").exists());
    }
}
#[test]
fn r2c_animation_schema_legacy_exact_roundtrip_and_contradictions_refuse() {
    use crate::placement::{animation::Mode,assembly::Properties};
    for (text,mode) in [("animated=true",Mode::Breathe),("animated=false",Mode::None),("",Mode::None)] {
        let p:Properties=toml::from_str(text).unwrap(); assert_eq!(p.mode(),mode);
        assert!(p.animation.is_none()); assert!(!toml::to_string(&p).unwrap().contains("animation ="));
    }
    for mode in [Mode::None,Mode::Breathe,Mode::BreatheRotate] {
        let mut p=Properties::default();p.set_mode(mode);p.validate().unwrap();
        assert_eq!(toml::from_str::<Properties>(&toml::to_string(&p).unwrap()).unwrap(),p);
        p.animated = !p.animated;assert!(p.validate().is_err());
        assert!(toml::from_str::<Properties>(&toml::to_string(&p).unwrap()).is_err());
    }
    for text in ["animation='spin'","animation=1","animated=true\nanimation='none'","animated=false\nanimation='breathe'","animated=1","animation='breathe'\nunknown=true"] {
        assert!(toml::from_str::<Properties>(text).is_err(),"{text}");
    }
    let d=slice5a_decoration([-2.,0.]);let text=toml::to_string(&d).unwrap();assert!(!text.contains("animation"));
    assert!(toml::from_str::<Decoration>(&format!("animation='unknown'\n{text}")).is_err());
    let mut proposed=d.clone();proposed.animation=Some(Mode::BreatheRotate);
    assert!(d.validate_relocation(&proposed).is_err());assert!(d.validate_resize(&proposed).is_err());
    assert!(DecorationAnimation::new(&d,&proposed).is_err());
    assert!(d.validate_legacy_animation(&proposed).is_ok(),"read-only historical classification remains valid");
    for bad in ["tag","identity","reference"] {
        let mut wrong=proposed.clone();match bad {"tag"=>wrong.tag="hair.other".into(),"identity"=>wrong.placement_id="ffffffffffffffffffffffffffffffff".into(),_=>{wrong.set_scale(75).unwrap();wrong.scaling.as_mut().unwrap().reference[0]+=0.1;}}
        assert!(DecorationAnimation::new(&d,&wrong).is_err(),"{bad}");
        assert!(d.validate_legacy_animation(&wrong).is_err(),"legacy {bad}");
    }
}
#[test]
fn r2c_presentation_rotation_swept_envelope_and_base_stable_all_sizes_times_surfaces() {
    use crate::placement::animation::Mode;
    let bytes=|m:&[crate::assets::Vertex]|bytemuck::cast_slice::<_,u8>(m).to_vec();
    let mut art=vec![];crate::scene::cuboid(&mut art,[0.1,0.5,0.],[0.7,1.,0.2],[0.2,0.8,0.3]);
    for percent in crate::placement::scaling::STOPS { for base in [false,true] { for surface in [false,true] {
        if base&&surface {continue;}
        let mut p=pos();p.at=[0.;2];p.yaw=0.7;p.properties.base=base;p.properties.set_mode(Mode::BreatheRotate);
        let scale=crate::placement::scaling::Scaling::new([0.8,0.85,0.28],percent).unwrap();p.size=scale.size().unwrap();p.properties.scaling=Some(scale);
        if surface {p.surface=Some(crate::placement::surface::Contact{at:[0.,0.3,0.],normal:[0.,0.8,0.6]});}
        let saved=p.clone();let b=p.file_bounds().unwrap();let base_bounds=p.base_bounds().unwrap();
        let model=p.transform(&art).unwrap();let base_art=p.base_transform(&art).unwrap();
        let first=p.present_model(&art,0.).unwrap();
        for t in [-12.,-3.,0.,1.5,3.,6.,9.,12.,1e300,f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
            let mesh=p.present_model(&art,t).unwrap();
            assert_eq!(p,saved);assert_eq!(p.base_bounds().unwrap(),base_bounds);assert_eq!(bytes(&p.base_transform(&art).unwrap()),bytes(&base_art));
            assert_eq!(bytes(&p.transform(&art).unwrap()),bytes(&model));
            for v in mesh {for i in 0..3 {let low=if i==1{b.at[i]}else{b.at[i]-b.size[i]*0.5};let high=if i==1{b.at[i]+b.size[i]}else{b.at[i]+b.size[i]*0.5};assert!(v.position[i]>=low-0.00001&&v.position[i]<=high+0.00001,"t={t} axis={i} {:?} {b:?}",v.position);}}
        }
        assert_eq!(bytes(&first),bytes(&p.present_model(&art,12.).unwrap()));assert_eq!(bytes(&first),bytes(&p.present_model(&art,f64::NAN).unwrap()));
    }}}
    for mode in [Mode::None,Mode::Breathe,Mode::BreatheRotate] {
        let mut d=slice5a_decoration([-2.,0.]);d.animation=Some(mode);let b=d.bounds().unwrap();let old=d.clone();
        for t in [0.,1.5,3.,6.,12.,f64::NAN,1e300] {
            let mesh=d.artwork_mesh_at(&Default::default(),t).unwrap();
            for v in &mesh[..36] {assert!(v.position[1]>=-0.000001&&v.position[1]<=d.size[1]+0.000001);assert!((v.position[0]-d.at[0]).abs()<=b.size[0]*0.5+0.000001);assert!((v.position[2]-d.at[1]).abs()<=b.size[2]*0.5+0.000001);}
            assert_eq!(d,old);assert_eq!(d.bounds().unwrap(),b);
        }
    }
}
#[test]
fn r2c_legacy_animation_resize_two_stages_pin_provenance_and_unsupported_pending() {
    use crate::placement::animation::Mode;
    for pin in [false,true] {for failure in ["success","first","late","listing","stale","busy","pin-rebind"] {
        if !pin&&failure=="pin-rebind" {continue;}
        let (f,e,follow,_)=slice5a_saved();
        // Use independent pinned fixture when requested, preserving the same writer.
        let (old,theme)=if pin {
            let (_,t)=slice6a_pack(&f);let source=select_pin_asset(&t,"chair").unwrap();
            let mut d=follow.clone();d.placement_id="ffffffffffffffffffffffffffffffff".into();d.at=[2.,0.];
            assert_eq!(t.publish_pin_checked(&e,&source,&d,|_|Ok(())),Outcome::Committed{synced:true});
            let old=read_layout(&e).unwrap().decorations.into_iter().find(|d|d.is_pinned()).unwrap();(old,Some(t))
        }else{(follow,None)};
        // LEGACY saved mode authored in the disposable fixture, not by a new
        // animation API. Exercise the same two-stage safeguards via legal resize.
        let mut old=old;old.animation=Some(Mode::BreatheRotate);
        std::fs::write(slice5a_record(&f,&old),toml::to_string(&old).unwrap()).unwrap();
        let old=read_layout(&e).unwrap().decorations.into_iter().find(|d|d.placement_id==old.placement_id).unwrap();
        let active=std::fs::read(slice5a_record(&f,&old)).unwrap();let mut proposed=old.clone();proposed.set_scale(75).unwrap();
        let mut expected=old.clone();if failure=="stale"{expected.yaw=0.2;}
        let intent=DecorationResize::new(&expected,&proposed).unwrap();let mut calls=0;
        use std::os::fd::AsRawFd;
        let lock=std::fs::File::open(f.0.join(".hair-layout/lock")).unwrap();
        if failure=="busy"{assert_eq!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);}
        let out=resize_decoration_checked(&e,&intent,|candidate|{
            calls+=1;assert!(read_layout(&e).unwrap_err().contains("busy"));
            if let Some(t)=&theme{t.validate_decoration_candidate_layout(candidate)?;}
            crate::placement_scene::validate_decoration_candidate(&e,candidate,[0.,2.],0)?;
            if failure=="first"||failure=="late"&&calls==2{return Err("owned animation refusal".into());}
            if failure=="listing"&&calls==2{std::fs::write(f.0.join("late.txt"),b"late listing").unwrap();}
            if failure=="pin-rebind"&&calls==2{
                use std::os::unix::fs::PermissionsExt;
                let p=f.0.join(format!(".hair-layout/ac-{}/asset.glb",old.closure_identity().unwrap()));let bytes=std::fs::read(&p).unwrap();
                std::fs::rename(&p,f.0.join("old-input.glb")).unwrap();std::fs::write(&p,bytes).unwrap();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            Ok(())
        });
        if failure=="busy"{assert_eq!(unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_UN)},0);}
        if failure=="success"{assert_eq!(calls,2);assert_eq!(out,Outcome::Committed{synced:true});assert!(read_layout(&e).unwrap().decorations.contains(&proposed));}
        else{assert!(matches!(out,Outcome::NotCommitted(_)),"{failure}: {out:?}");assert_eq!(std::fs::read(slice5a_record(&f,&old)).unwrap(),active);
            if ["late","listing","pin-rebind"].contains(&failure){
                assert!(format!("{out:?}").contains("Pending resize retained; not resumable; recovery required"));
                if failure!="pin-rebind"{
                    let i=inspect_placements(&e).unwrap();let row=i.attempts().iter().position(|r|r.identity()==format!("dc-{}",old.placement_id)).unwrap();
                    assert_eq!(i.attempts()[row].kind(),AttemptKind::DecorationResize);assert_eq!(i.attempts()[row].state(),AttemptState::Unsupported);
                    assert!(i.select_decoration_resume(row).err().unwrap().contains("not resumable"));
                }
            }
        }
    }}
}
#[test]
fn r2c_legacy_interrupted_animation_evidence_classifies_without_new_api_authority() {
    use crate::placement::animation::Mode;
    use std::{io::Write,os::unix::fs::OpenOptionsExt};
    for pin in [false,true] {for bad in ["valid","tag","identity","reference"] {
        let (f,e,follow,_)=slice5a_saved();
        let old=if pin {
            let (_,t)=slice6a_pack(&f);let source=select_pin_asset(&t,"chair").unwrap();
            let mut d=follow.clone();d.placement_id="ffffffffffffffffffffffffffffffff".into();d.at=[2.,0.];
            assert_eq!(t.publish_pin_checked(&e,&source,&d,|_|Ok(())),Outcome::Committed{synced:true});
            read_layout(&e).unwrap().decorations.into_iter().find(|d|d.is_pinned()).unwrap()
        }else{follow};
        let mut proposed=old.clone();proposed.animation=Some(Mode::BreatheRotate);
        match bad {"tag"=>proposed.tag="hair.other".into(),"identity"=>proposed.placement_id="eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into(),"reference"=>{proposed.set_scale(75).unwrap();proposed.scaling.as_mut().unwrap().reference[0]+=0.1;},_=>{}}
        assert!(DecorationAnimation::new(&old,&proposed).is_err());
        // Intentionally authored LEGACY interrupted transaction. No current
        // publication API is bypassed or claimed to produce animation intent.
        let pending=slice5a_record(&f,&old).with_file_name("pending.toml");
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600)
            .custom_flags(libc::O_NOFOLLOW|libc::O_CLOEXEC).open(&pending).unwrap();
        file.write_all(toml::to_string(&proposed).unwrap().as_bytes()).unwrap();file.sync_all().unwrap();
        let before=slice5a_snapshot(&f.0);let i=inspect_placements(&e).unwrap();
        let row=i.attempts().iter().position(|r|r.identity()==format!("dc-{}",old.placement_id)).unwrap();
        assert_eq!(i.attempts()[row].state(),AttemptState::Unsupported);
        assert_eq!(i.attempts()[row].kind()==AttemptKind::DecorationAnimation,bad=="valid");
        if bad=="valid" {assert!(i.select_decoration_resume(row).err().unwrap().contains("Pending animation retained; not resumable; recovery required"));}
        else {assert!(i.select_decoration_resume(row).is_err());}
        assert!(read_layout(&e).unwrap().decorations.contains(&old));assert_eq!(slice5a_snapshot(&f.0),before);
    }}
}
#[test]
fn decoration_animation_guard_public_writer_refuses_even_internal_legacy_intent() {
    use crate::placement::animation::Mode;
    for saved in [None,Some(Mode::None),Some(Mode::Breathe),Some(Mode::BreatheRotate)] {
        let (f,e,mut old,_)=slice5a_saved();old.animation=saved;
        std::fs::write(slice5a_record(&f,&old),toml::to_string(&old).unwrap()).unwrap();
        for animation in [None,Some(Mode::None),Some(Mode::Breathe),Some(Mode::BreatheRotate)] {
            let mut proposed=old.clone();proposed.animation=animation;
            // Adversarial internal construction only. External callers cannot
            // construct this private intent; even this cannot authorize writing.
            let intent=DecorationAnimation{expected:old.clone(),proposed};
            let before=slice5a_snapshot(&f.0);
            let out=animate_decoration_checked(&e,&intent,|_|panic!("refused animation reached callback"));
            assert!(matches!(out,Outcome::NotCommitted(_)));assert_eq!(slice5a_snapshot(&f.0),before);
        }
    }
}
#[test]
fn r2c_legacy_prepared_new_and_relocation_resume_preserve_animation_and_exact_bytes() {
    use crate::placement::animation::Mode;
    for kind in ["new","pin","relocation","pin-relocation"] {
        let (f,e,theme,mut proposed)=slice8b_prepare(kind);
        let pending=slice5a_record(&f,&proposed).with_file_name("pending.toml");
        let active=pending.with_file_name("decoration.toml");
        // Intentionally authored completed LEGACY preparation. Recovery retains
        // schema validation rather than incorrectly applying new-intent gates.
        proposed.animation=Some(Mode::BreatheRotate);
        std::fs::write(&pending,toml::to_string(&proposed).unwrap()).unwrap();
        if kind.contains("relocation") {
            let mut old:Decoration=toml::from_str(&std::fs::read_to_string(&active).unwrap()).unwrap();
            old.animation=proposed.animation;std::fs::write(&active,toml::to_string(&old).unwrap()).unwrap();
        }
        let before=slice8a_snapshot(&f.0);let i=inspect_placements(&e).unwrap();
        let index=slice8b_index(&i,&proposed);let s=i.select_decoration_resume(index).unwrap();
        assert_eq!(s.proposal(),&proposed);assert_eq!(slice8a_snapshot(&f.0),before);
        let mut stages=vec![];
        let out=theme.resume_decoration_checked(&e,&s,&AtomicBool::new(false),|stage,candidate| {
            stages.push(stage);crate::placement_scene::validate_decoration_candidate(&e,candidate,[0.,2.],0)
        });
        assert_eq!(out,Outcome::Committed{synced:true});assert_eq!(stages,vec![ResumeStage::Review,ResumeStage::Final]);
        let after=slice8a_snapshot(&f.0);
        let old_pending=before.files.iter().find(|x|x.0==pending).unwrap();
        let new_active=after.files.iter().find(|x|x.0==active).unwrap();
        assert_eq!((old_pending.1,old_pending.2,&old_pending.3),(new_active.1,new_active.2,&new_active.3));
        for item in &before.files {if item.0!=pending&&item.0!=active {assert!(after.files.contains(item));}}
        for item in &before.attrs {if item.0!=pending&&item.0!=active {assert!(after.attrs.contains(item));}}
        let live=read_layout(&Explorer::new(&f.0).unwrap()).unwrap();assert!(live.decorations.contains(&proposed));assert_eq!(live.incomplete_records,0);
        slice8b_cold(&f.0,&live);
    }
}
