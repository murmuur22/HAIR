//! Parent-owned actual App/input continuation tracer.
use super::*;
fn rooms(a: Address, cancel: &hair::acquisition::Cancellation, deadline: Instant) -> Result<Page> {
    cancel.check(deadline)?;
    let mut html = String::from("<title>Rooms</title><p>Start</p>");
    for i in 0..130 {
        html.push_str(&format!(
            "<a href='/Exact/{i}?Case=AB%2Fcd#Tail'>Link{i}</a>"
        ));
    }
    html.push_str("<p>Final tail</p>");
    Page::html(a, html.as_bytes())
}
#[test]
fn physical_next_e_activates_same_response_second_room() {
    let mut a = app();
    a.web.as_mut().unwrap().fetcher = rooms;
    a.request_web(
        "https://example.org/Board?Case=A%2Fb",
        web_native::Mode::Initial,
    )
    .unwrap();
    finish(&mut a);
    assert_eq!(a.world.reader_links()[0].label, "Link0");
    let host = &a.themes[a.theme_index].host;
    let door = host
        .anchors
        .iter()
        .find(|d| d.id == "web-next-window")
        .unwrap();
    let inward = host.portal_inward(&door.id);
    a.pose = Pose {
        eye_offset: 1.65,
        feet_y: 0.0,
        position: [door.at[0] + inward[0] * 1.0, door.at[2] + inward[1] * 1.0],
        yaw: (-inward[0]).atan2(inward[1]),
        pitch: -0.1,
    };
    hair::interaction::refresh(host, &a.pose, &mut a.world);
    a.keyboard_input(Some(KeyCode::KeyE), true, false, None)
        .unwrap();
    a.keyboard_input(Some(KeyCode::KeyE), false, false, None)
        .unwrap();
    finish(&mut a);
    assert_eq!(
        a.world.reader_links()[0].label,
        "Link64",
        "APP_CONTINUATION_RED: physical Next E must activate the next bounded room of the same response"
    );
    assert_eq!(a.world.current, "https://example.org/Board?Case=A%2Fb");
}
