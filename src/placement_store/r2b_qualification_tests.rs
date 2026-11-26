// Included in placement_store::tests. Independent valid scaled fixtures.
#[test]
fn r2b_qual_150_ceiling_overlap_spawn_and_support_boundaries() {
    use crate::{placement::{surface, Overlap}, host_scene::Geometry};
    let f=Fixture::new(); let e=Explorer::new(&f.0).unwrap();
    let (world,mut host)=crate::folder_scene::build(e.current(),0,true).unwrap();
    host.anchors.clear(); host.geometry.clear(); host.overhead.clear();
    let old=slice5a_decoration([-2.,0.]); let mut d=old.clone(); d.set_scale(150).unwrap();
    let mut low=host.clone(); low.open_platform=false; low.height=1.;
    surface::clearance(&low,&old.envelope_mesh().unwrap(),Overlap::Prevent).unwrap();
    assert!(surface::clearance(&low,&d.envelope_mesh().unwrap(),Overlap::Prevent).unwrap_err().contains("ceiling"));
    let other=slice5a_decoration([-1.05,0.]);
    host.decorations.insert(other.placement_id.clone(),other);
    surface::clearance(&host,&old.envelope_mesh().unwrap(),Overlap::Prevent).unwrap();
    assert!(surface::clearance(&host,&d.envelope_mesh().unwrap(),Overlap::Prevent).unwrap_err().contains("saved decoration"));
    host.decorations.clear();
    host.spawn=[0.,2.]; // Interior reservation, not the platform edge bound.
    let mut spawn=d.clone(); spawn.at=host.spawn;
    assert_eq!(surface::clearance(&host,&spawn.envelope_mesh().unwrap(),Overlap::Prevent).unwrap_err(), "Placement blocks arrival");
    // The scaled mesh genuinely touches the independent top plane; a lowered
    // contact penetrates it and is rejected by the production clearance path.
    host.geometry.push(Geometry{at:[2.,0.,0.],size:[2.,0.8,2.]});
    let mut p=pos(); p.at=[2.,0.]; p.properties.scaling=Some(crate::placement::scaling::Scaling::new(p.size,150).unwrap()); p.size=p.properties.scaled_size(p.size).unwrap();
    p.surface=Some(surface::Contact{at:[2.,0.8,0.],normal:[0.,1.,0.]});
    surface::clearance(&host,&p.envelope_mesh().unwrap(),Overlap::Prevent).unwrap();
    p.surface.as_mut().unwrap().at[1]=0.79;
    assert!(surface::clearance(&host,&p.envelope_mesh().unwrap(),Overlap::Prevent).unwrap_err().contains("structural"));
    p.surface.as_mut().unwrap().at[1]=0.8; p.properties.base=true;
    assert!(p.validate().unwrap_err().contains("floor contact"));
    let _=world;
}
#[test]
fn r2b_qual_150_route_disconnection_has_clear_local_footprint() {
    let f=Fixture::new(); let e=Explorer::new(&f.0).unwrap();
    let (world,mut host)=crate::folder_scene::build(e.current(),0,true).unwrap();
    host.room=[-5.,-5.,5.,5.]; host.spawn=[0.,3.];
    host.anchors.retain(|a|a.role=="document"); host.anchors[0].at=[0.,0.,-3.];
    host.geometry=vec![crate::host_scene::Geometry{at:[-3.,0.,0.],size:[4.4,2.,0.6]},crate::host_scene::Geometry{at:[3.,0.,0.],size:[4.4,2.,0.6]}];
    host.check_routes(&[[0.,-2.]]).unwrap();
    let mut d=slice5a_decoration([0.,0.]); d.size=[0.8,0.8,0.4]; d.set_scale(150).unwrap();
    crate::placement::surface::clearance(&host,&d.envelope_mesh().unwrap(),crate::placement::Overlap::Prevent).unwrap();
    let layout=LayoutRead{decorations:vec![d],..Default::default()};
    let before=host.clone(); let err=crate::placement_scene::apply(&layout,&world,&mut host).unwrap_err();
    assert!(err.contains("unreachable"),"{err}"); assert_eq!(host,before);
}
#[test]
fn r2b_qual_150_fresh_all_page_generated_scene_refusal() {
    let f=Fixture::new();
    for n in 0..13 {std::fs::write(f.0.join(format!("file-{n:02}.txt")),b"owned").unwrap();}
    let e=Explorer::new(&f.0).unwrap();
    let mut d=slice5a_decoration([-4.5,-3.]); d.size=[0.8,0.2,0.8]; d.set_scale(150).unwrap();
    let candidate=LayoutRead{decorations:vec![d],..Default::default()};
    // Last page has only one file; first page's generated object at the exact
    // placement cannot be omitted merely because current_page is the last page.
    let err=crate::placement_scene::validate_decoration_candidate(&e,&candidate,[0.,2.],1).unwrap_err();
    assert!(err.contains("existing object"),"intended generated-object boundary: {err}");
    let before=slice5a_snapshot(&f.0);
    assert!(before.iter().all(|(p,_,_,_)|!p.to_string_lossy().contains(".hair-layout")));
}
#[test]
fn r2b_qual_expected_record_and_pin_descriptor_rebound_both_locked_stages() {
    use std::os::unix::fs::PermissionsExt;
    for stage in [1,2] {for record in [false,true] {
        let f=Fixture::new(); let e=Explorer::new(&f.0).unwrap(); let (_,theme)=slice6a_pack(&f);
        let follow=slice5a_decoration([-2.,0.]); let source=select_pin_asset(&theme,"chair").unwrap();
        assert_eq!(theme.publish_pin_checked(&e,&source,&follow,|_|Ok(())),Outcome::Committed{synced:true});
        let old=read_layout(&e).unwrap().decorations[0].clone(); let mut proposed=old.clone(); proposed.set_scale(150).unwrap();
        let active=slice5a_record(&f,&old); let active_bytes=std::fs::read(&active).unwrap();
        let path=if record {active.clone()} else {f.0.join(format!(".hair-layout/ac-{}/closure.toml",old.closure_identity().unwrap()))};
        let original=std::fs::read(&path).unwrap(); let displaced=f.0.join("held-original");
        let rebind=|| {std::fs::rename(&path,&displaced).unwrap(); std::fs::write(&path,&original).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();};
        let mut calls=0;
        let out=mac::update_decoration(&e,&old,&proposed,true,&mut |candidate| {
            calls+=1; theme.validate_decoration_candidate_layout(candidate)?;
            crate::placement_scene::validate_decoration_candidate(&e,candidate,[0.,2.],0)?;
            if stage==2 && calls==2 {rebind();} Ok(())
        },|s| {if stage==1 && s==1 {rebind();}Ok(())});
        let Outcome::NotCommitted(reason)=out else {panic!("rebound accepted: {out:?}")};
        assert!(reason.contains("changed")||reason.contains("identity"),"{reason}");
        assert_eq!(std::fs::read(&active).unwrap(),active_bytes); assert_eq!(std::fs::read(&displaced).unwrap(),original);
        assert_eq!(read_layout(&e).unwrap().decorations,vec![old]);
    }}
}
#[test]
fn r2b_qual_untrusted_pending_is_not_classified_as_resize() {
    for kind in ["malformed","tag","reference","noop"] {
        let (f,e,old,_)=slice5a_saved(); let mut proposed=old.clone(); proposed.set_scale(150).unwrap();
        let _=mac::update_decoration(&e,&old,&proposed,true,&mut |_|Ok(()),|s|if s==1 {Err("owned between-stage refusal".into())}else{Ok(())});
        let pending=slice5a_record(&f,&old).with_file_name("pending.toml");
        match kind {"malformed"=>std::fs::write(&pending,b"[bad").unwrap(),"tag"=>{proposed.tag="hair.other".into();std::fs::write(&pending,toml::to_string(&proposed).unwrap()).unwrap();},"reference"=>{proposed.scaling.as_mut().unwrap().reference[0]+=0.1;std::fs::write(&pending,toml::to_string(&proposed).unwrap()).unwrap();},_=>std::fs::write(&pending,toml::to_string(&old).unwrap()).unwrap()}
        let before=slice5a_snapshot(&f.0); let i=inspect_placements(&e).unwrap();
        assert_ne!(i.attempts()[0].kind(),AttemptKind::DecorationResize); assert_eq!(i.attempts()[0].state(),AttemptState::Unsupported);
        assert!(i.select_decoration_resume(0).is_err()); assert_eq!(slice5a_snapshot(&f.0),before);
    }
}
#[test]
fn r2b_qual_ordinary_relocation_unacquired_pin_error_is_not_resize() {
    let f=Fixture::new(); let e=Explorer::new(&f.0).unwrap(); let (_,theme)=slice6a_pack(&f);
    let source=select_pin_asset(&theme,"chair").unwrap(); let follow=slice5a_decoration([-2.,0.]);
    assert_eq!(theme.publish_pin_checked(&e,&source,&follow,|_|Ok(())),Outcome::Committed{synced:true});
    let old=read_layout(&e).unwrap().decorations[0].clone();
    let unacquired:Decoration=toml::from_str(&toml::to_string(&old).unwrap()).unwrap();
    let mut proposed=unacquired.clone(); proposed.at[0]+=0.1;
    assert_eq!(relocate_decoration_checked(&e,&unacquired,&proposed,|_|panic!("no source acquisition")),Outcome::NotCommitted("Decoration pin not acquired".into()));
}
