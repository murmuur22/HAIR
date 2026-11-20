use super::transport::*;
use std::time::{Duration, Instant};
fn fixture(ca: bool, host: &str) -> Transport {
    let port =
        std::env::var("HAIR_FIXTURE_PORT").expect("run fixture.py; isolated generated CA required");
    let pem = std::fs::read(std::env::var("HAIR_FIXTURE_CA").unwrap()).unwrap();
    Transport::fixture(
        format!("https://{host}:{port}/").parse().unwrap(),
        format!("127.0.0.1:{port}").parse().unwrap(),
        ca.then_some(pem.as_slice()),
    )
    .unwrap()
}
#[test]
#[ignore = "explicit isolated local TLS fixture, not public PKI"]
fn real_tls_pin_headers_status_encoding_size_and_cancellation() {
    let t = fixture(true, "fixture.example");
    let c = Cancellation::default();
    assert_eq!(
        t.get(
            "ok",
            4,
            Some(4),
            &c,
            Instant::now() + Duration::from_secs(5)
        )
        .unwrap(),
        b"data"
    );
    for p in [
        "redirect",
        "gzip",
        "overflow",
        "length",
        "missing",
        "wrongtype",
        "cookie-check",
    ] {
        assert!(
            t.get(p, 4, Some(4), &c, Instant::now() + Duration::from_secs(5))
                .is_err(),
            "accepted {p}"
        );
    }
    assert!(
        fixture(false, "fixture.example")
            .get("ok", 4, None, &c, Instant::now() + Duration::from_secs(5))
            .is_err(),
        "production public roots must reject fixture CA"
    );
    assert!(
        fixture(true, "wrong.example")
            .get("ok", 4, None, &c, Instant::now() + Duration::from_secs(5))
            .is_err(),
        "hostname verification"
    );
    c.cancel();
    assert!(
        t.get("ok", 4, None, &c, Instant::now() + Duration::from_secs(5))
            .is_err()
    );
    let c = Cancellation::default();
    let start = Instant::now();
    assert!(
        t.get("stall", 4, None, &c, start + Duration::from_millis(200))
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(3));
}
