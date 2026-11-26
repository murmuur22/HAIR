#![cfg(target_os = "macos")]
use hair::{local_explorer::Explorer, placement_store::{self as store, Decoration, Outcome, ResumeStage}, theme::Theme};
use std::{path::{Path, PathBuf}, sync::atomic::{AtomicBool, AtomicU64, Ordering}};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("hair-slice8b-public-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&p).unwrap(); Self(p)
    }
}
impl Drop for Fixture { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }
fn theme() -> Theme { Theme::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("packs/office.toml"), &mut Default::default()).unwrap() }
fn proposal(t: &Theme) -> Decoration { Decoration::from_entry(&t.decorations.entries[0], [2., 0.], 0., [0.4; 3]).unwrap() }
#[test]
#[ignore = "child-only actual interrupted public writer"]
fn slice8b_public_writer_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8B_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let e = Explorer::new(&root).unwrap(); let t = theme(); let d = proposal(&t);
    let mut review_passed = false;
    let out = store::place_decoration_checked(&e, &d, |live| {
        t.validate_decoration_layout(live, &d)?;
        hair::placement_scene::validate_decoration(&e, live, &d, [0., 2.], 0)?;
        if review_passed { unsafe { libc::_exit(73) }; }
        review_passed = true; Ok(())
    });
    panic!("writer did not reach prepared validated boundary: {out:?}");
}
#[test]
fn slice8b_public_interrupted_writer_selection_resume_refresh_and_cold_reopen() {
    let f = Fixture::new(); let e = Explorer::new(&f.0).unwrap();
    let crash = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "slice8b_public_writer_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE8B_ROOT", &f.0).output().unwrap();
    assert_eq!(crash.status.code(), Some(73), "{}{}", String::from_utf8_lossy(&crash.stdout), String::from_utf8_lossy(&crash.stderr));
    let inspection = store::inspect_placements(&e).unwrap();
    let selection = inspection.select_decoration_resume(0).expect("complete real pending intent must be selectable");
    let expected = selection.proposal().clone(); assert!(selection.predecessor().is_none());
    let mut reached = vec![];
    let result = theme().resume_decoration_checked(&e, &selection, &AtomicBool::new(false), |stage, candidate| {
        reached.push(stage);
        assert_eq!(candidate.decorations, vec![expected.clone()]);
        hair::placement_scene::validate_decoration_candidate(&e, candidate, [0., 2.], 0)
    });
    assert_eq!(result, Outcome::Committed { synced: true });
    assert_eq!(reached, vec![ResumeStage::Review, ResumeStage::Final]);
    let mut reopened = Explorer::new(&f.0).unwrap(); reopened.refresh().unwrap();
    assert_eq!(store::read_layout(&reopened).unwrap().decorations, vec![expected]);
    let read = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "slice8b_public_cold_reader_child", "--ignored", "--nocapture"])
        .env("HAIR_SLICE8B_ROOT", &f.0).output().unwrap();
    assert!(read.status.success(), "{}{}", String::from_utf8_lossy(&read.stdout), String::from_utf8_lossy(&read.stderr));
}
#[test]
#[ignore = "child-only cold strict reader and real Theme admission"]
fn slice8b_public_cold_reader_child() {
    let root = PathBuf::from(std::env::var_os("HAIR_SLICE8B_ROOT").unwrap());
    assert!(root.starts_with(std::env::temp_dir()));
    let e = Explorer::new(&root).unwrap(); let layout = store::read_layout(&e).unwrap();
    assert_eq!(layout.decorations.len(), 1); assert_eq!(layout.incomplete_records, 0);
    theme().validate_layout_decorations(&layout).unwrap();
    hair::placement_scene::validate_decoration_candidate(&e, &layout, [0., 2.], 0).unwrap();
}
