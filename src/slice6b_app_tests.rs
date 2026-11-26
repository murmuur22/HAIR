//! Actual App input qualification. All filesystem mutations are disposable fixtures.
use super::*;
#[path = "slice6b_controls_tests.rs"]
mod controls;
#[path = "slice6b_retained_tests.rs"]
mod retained;

fn choose_pin(a: &mut App) {
    // Reveal the actual paged property before asserting its complete label.
    reveal_control(a, place_mode_native::Control::PinAsset);
    let control = a.toolbox_controls().into_iter()
        .find(|(c, _, _)| *c == place_mode_native::Control::PinAsset)
        .map(|(c, _, _)| c)
        .expect("new decoration toolbox must offer explicit Pin this asset to this folder");
    click(a, control);
    assert!(a.decoration_position().unwrap().is_pinned(), "{}", a.local_status);
}
fn mesh(a: &App, d: &hair::placement_store::Decoration) -> Vec<u8> {
    bytemuck::cast_slice(&a.themes[a.theme_index].decoration_preview(d).unwrap()).to_vec()
}

#[test]
fn slice6b_actual_toolbox_pin_preview_freeze_commit_refresh_reopen() {
    let (f, mut a) = open_fixture("slice6b-tracer");
    let original = records(&f.0);
    start(&mut a, [-2., 0.], true);
    assert!(!a.decoration_position().unwrap().is_pinned(), "Follow theme remains the default");
    let follow = a.decoration_position().unwrap();
    let selected = a.themes[0].decorations.entries[0].clone();
    let source_asset = a.themes[0].decorations.asset(&selected.asset).unwrap();
    let source_spec = source_asset.spec.clone();
    let source_model = source_asset.model.clone();
    let source_bytes = std::fs::read(a.themes[0].source.parent().unwrap().join(&source_spec.file)).unwrap();
    choose_pin(&mut a);
    let proposed = a.decoration_position().unwrap();
    assert_eq!(proposed.placement_id, follow.placement_id);
    assert_eq!(proposed.tag, follow.tag);
    let preview = a.place_preview().unwrap();
    assert!(preview.len() > 36, "real catalog mesh, not an envelope stub");
    let appearance = mesh(&a, &proposed);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&preview), appearance);
    assert!(!a.place_ui().is_empty());
    assert_eq!(records(&f.0), original, "choice and preview are write-free");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("frozen") && a.local_status.contains("pin"), "{}", a.local_status);
    assert_eq!(a.decoration_position().unwrap(), proposed);
    assert_eq!(records(&f.0), original, "first Enter is write-free");
    tap(&mut a, KeyCode::Enter);
    assert!(a.local_status.contains("decoration pinned"), "{}", a.local_status);
    let saved = layout(&a);
    assert_eq!(saved.decorations, vec![proposed.clone()]);
    assert_eq!(mesh(&a, &saved.decorations[0]), appearance);
    let empty = hair::decoration::Catalog::default();
    let (entry, retained) = saved.decorations[0].resolve(&empty).unwrap();
    let mut owned_spec = source_spec.clone(); owned_spec.file = "asset.glb".into();
    assert_eq!(entry, &selected); assert_eq!(retained.spec, owned_spec);
    assert_eq!(bytemuck::cast_slice::<_, u8>(&retained.model.vertices), bytemuck::cast_slice::<_, u8>(&source_model.vertices));
    assert_eq!(retained.model.textures.len(), source_model.textures.len());
    for (retained, original) in retained.model.textures.iter().zip(&source_model.textures) {
        assert_eq!((retained.width, retained.height, &retained.rgba), (original.width, original.height, &original.rgba));
    }
    assert_eq!(std::fs::read(f.0.join(format!(".hair-layout/ac-{}/asset.glb", proposed.closure_identity().unwrap()))).unwrap(), source_bytes);
    let text = std::fs::read_to_string(f.0.join(format!(".hair-layout/dc-{}/decoration.toml", proposed.placement_id))).unwrap();
    assert!(text.contains("version = 2") && text.contains("decoration-pinned"));
    for (p, value) in &original { assert_eq!(records(&f.0).get(p), Some(value)); }
    let published = records(&f.0);
    tap(&mut a, KeyCode::F5);
    assert_eq!(layout(&a), saved);
    assert_eq!(mesh(&a, &proposed), appearance);
    let mut reopened = app(); reopened.complete_folder(Ok(Some(f.0.clone()))).unwrap();
    assert_eq!(layout(&reopened), saved);
    assert_eq!(mesh(&reopened, &layout(&reopened).decorations[0]), appearance);
    assert_eq!(records(&f.0), published);
    assert!(a.inventory.items().is_empty() && a.placement_action.is_none());
}
