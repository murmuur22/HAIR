use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static ATTEMPTS: AtomicUsize = AtomicUsize::new(0);
fn content(a: Address, c: &hair::acquisition::Cancellation, d: Instant) -> Result<Page> {
    c.check(d)?;
    Page::html(a, b"<title>Pocket</title><h1>Orchard</h1><p>Apple trees.</p><h2>Harbour</h2><p>Boats arrive.</p><a href='https://second.example/landing'>Uncharted pocket</a>")
}
pub(super) fn unavailable(
    _: Address,
    _: &hair::acquisition::Cancellation,
    _: Instant,
) -> Result<Page> {
    ATTEMPTS.fetch_add(1, Ordering::SeqCst);
    Err("Test destination unavailable".into())
}
fn tap(a: &mut App, k: KeyCode) {
    a.keyboard_input(Some(k), true, false, None).unwrap();
    a.keyboard_input(Some(k), false, false, None).unwrap();
}
#[test]
fn failed_journey_updates_the_aimed_portal_but_allows_retry_and_return() {
    ATTEMPTS.store(0, Ordering::SeqCst);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = web_native::prepare(
        Address::parse("https://example.org/pocket").unwrap(),
        &[
            root.join("packs/office.toml"),
            root.join("packs/archive.toml"),
        ],
        content,
        &Default::default(),
    )
    .unwrap();
    let mut a = App::new(root, p.themes, 0, p.generated.world, false);
    a.web = Some(web_native::Runtime::new(p.generated.doors, unavailable));
    let door = a.web.as_ref().unwrap().doors[0].anchor.clone();
    let anchor = a.themes[0]
        .host
        .anchors
        .iter()
        .find(|h| h.id == door)
        .unwrap();
    a.pose = Pose {
        position: [anchor.at[0] + 0.4, -2.7],
        yaw: 0.0,
        pitch: -0.2,
    };
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    assert_eq!(a.world.interaction.as_ref().unwrap().anchor, door);
    let initial = hair::text::portal_overlay(&a.world, 960, 640);
    let current = a.world.current.clone();
    let pose = a.pose;
    let host = a.themes[0].host.clone();
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(ATTEMPTS.load(Ordering::SeqCst), 1);
    assert_eq!(a.world.current, current);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[0].host, host);
    assert!(a.web.as_ref().unwrap().history.is_empty());
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    let failed = hair::text::portal_overlay(&a.world, 960, 640);
    assert_ne!(
        bytemuck::cast_slice::<_, u8>(&initial),
        bytemuck::cast_slice::<_, u8>(&failed),
        "failure must be represented at the selected portal, not only a global error log"
    );
    // CPU App has no renderer: actual T switching is exercised in the GPU proof.
    a.web.as_mut().unwrap().fetcher = content;
    tap(&mut a, KeyCode::F5);
    finish(&mut a);
    assert!(
        hair::web::portals::view(&a.world, &door)
            .unwrap()
            .last_failure
            .is_some(),
        "refreshing the source must not pretend the failed destination succeeded"
    );
    a.web.as_mut().unwrap().fetcher = unavailable;
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(
        ATTEMPTS.load(Ordering::SeqCst),
        2,
        "previous failure must not disable the owner's chosen retry"
    );
    a.web.as_mut().unwrap().fetcher = content;
    let target = hair::web::door_destination(&a.world, &door).unwrap();
    tap(&mut a, KeyCode::KeyE);
    finish(&mut a);
    assert_eq!(a.world.current, target);
    assert_eq!(a.web.as_ref().unwrap().history.len(), 1);
    tap(&mut a, KeyCode::KeyR);
    assert_eq!(a.world.current, current);
    assert_eq!(a.pose.position, pose.position);
    assert_eq!(a.themes[0].host, host);
    hair::interaction::refresh(&a.themes[0].host, &a.pose, &mut a.world);
    let returned = hair::text::portal_overlay(&a.world, 960, 640);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&initial),
        bytemuck::cast_slice::<_, u8>(&returned),
        "successful arrival must clear stale failed-arrival warning before history snapshot"
    );
    a.web.as_mut().unwrap().shutdown();
}
