//! Actual Explorer -> sole model/image worker -> scheduler publication.
//! Acquisition counters are local open_reader attempts, NOT an OS-wide I/O audit.
use super::*;
use crate::local_model_fallback_tests::fixture;
use fixture::owned::Fixture;
use std::{
    fs,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[19, 37, 83, 255].repeat((width * height) as usize))
            .unwrap();
        writer.finish().unwrap();
    }
    bytes
}
fn view(explorer: &Explorer) -> folder_scene::View {
    folder_scene::build(explorer.current(), 0, false)
        .unwrap()
        .0
        .folder_view
        .unwrap()
}
fn anchor(view: &folder_scene::View, name: &str) -> String {
    view.bindings
        .iter()
        .find(|(_, b)| b.entry.name == name)
        .unwrap()
        .0
        .clone()
}
fn finished(worker: &image::Runtime) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.test_finished() {
        assert!(
            Instant::now() < deadline,
            "model/image worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn drain(runtime: &mut Runtime, explorer: &Explorer, view: &folder_scene::View) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        runtime.poll(Some(explorer), Some(view), false);
        assert!(runtime.cached_items() <= ITEM_CAP);
        assert!(runtime.cached_bytes() <= CACHE_BYTES);
        if runtime.worker.busy() {
            assert!(
                runtime.active.is_some(),
                "one active source owns the sole worker"
            );
            assert!(runtime.cached_bytes() + WORK_RESERVATION <= TOTAL_BYTES);
        }
        if runtime.queue.is_empty() && runtime.active.is_none() && !runtime.worker.busy() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "scheduler did not drain: {}",
            runtime.status
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn actual_charge(runtime: &Runtime) -> usize {
    runtime.cache.values().map(|t| t.rgba.len()).sum::<usize>()
        + runtime
            .models
            .values()
            .map(|m| {
                m.vertices.capacity() * std::mem::size_of::<hair::assets::Vertex>()
                    + std::mem::size_of::<hair::assets::Model>()
            })
            .sum::<usize>()
}

#[test]
fn mixed_real_model_image_results_share_eight_item_cap_and_capacity_ledger() {
    let f = Fixture::new();
    let model = fixture::control();
    let png = png_bytes(1, 1);
    hair::assets::decode_local_model(&model).unwrap();
    assert_eq!(
        hair::assets::decode_local_image(&png).unwrap().rgba,
        [19, 37, 83, 255]
    );
    for i in 0..10 {
        if i % 2 == 0 {
            f.file(&format!("f{i:02}.glb"), &model);
        } else {
            f.file(&format!("f{i:02}.png"), &png);
        }
    }
    let explorer = Explorer::new(&f.0).unwrap();
    let view = view(&explorer);
    let mut runtime = Runtime::default();
    runtime.room(&view);
    assert_eq!(runtime.acquisition_attempts, 0);
    for _ in 0..4 {
        runtime.poll(Some(&explorer), Some(&view), true);
    }
    assert_eq!(
        runtime.acquisition_attempts, 0,
        "paused means no scheduler acquisitions"
    );
    drain(&mut runtime, &explorer, &view);
    assert_eq!((runtime.models.len(), runtime.cache.len()), (4, 4));
    assert_eq!(runtime.cached_items(), 8);
    assert_eq!(runtime.acquisition_attempts, 8);
    assert_eq!(runtime.cached_bytes(), actual_charge(&runtime));
    assert!(
        runtime
            .acquisition_sources
            .iter()
            .all(|e| view.bindings.values().any(|b| b.entry == *e))
    );
    for (id, bound) in &view.bindings {
        if runtime.models.contains_key(id) {
            assert!(bound.entry.name.as_encoded_bytes().ends_with(b".glb"));
            let actual = &runtime.models[id];
            assert_eq!(actual.vertices.len(), 12);
            assert_eq!(actual.min, [1.0, 2.0, 3.0]);
            assert_eq!(actual.max, [5.0, 4.0, 9.0]);
        }
        if runtime.cache.contains_key(id) {
            assert!(bound.entry.name.as_encoded_bytes().ends_with(b".png"));
            assert_eq!(runtime.cache[id].rgba, [19, 37, 83, 255]);
        }
    }
    for _ in 0..32 {
        runtime.poll(Some(&explorer), Some(&view), false);
    }
    assert_eq!(
        runtime.acquisition_attempts, 8,
        "no ordinary-poll retry/eviction churn"
    );
    drop(runtime);
}

#[test]
fn real_model_charge_participates_in_conservative_next_image_admission() {
    for (height, expected_items) in [(1023, 3), (1024, 2)] {
        let f = Fixture::new();
        f.file("a.glb", &fixture::control());
        f.file("b.png", &png_bytes(1024, height));
        f.file("c.glb", &fixture::control());
        let explorer = Explorer::new(&f.0).unwrap();
        let view = view(&explorer);
        let ordered: Vec<_> = view
            .bindings
            .values()
            .map(|b| b.entry.name.clone())
            .collect();
        assert_eq!(
            ordered,
            ["a.glb", "b.png", "c.glb"].map(std::ffi::OsString::from)
        );
        let mut runtime = Runtime::default();
        runtime.room(&view);
        drain(&mut runtime, &explorer, &view);
        assert_eq!(runtime.cached_items(), expected_items);
        assert_eq!(runtime.acquisition_attempts, expected_items);
        assert_eq!(runtime.cached_bytes(), actual_charge(&runtime));
        assert_eq!(
            runtime.cache[&anchor(&view, "b.png")].rgba.len(),
            1024 * height as usize * 4
        );
        let first_charge = model_bytes(&runtime.models[&anchor(&view, "a.glb")]);
        assert!(
            first_charge > 0 && first_charge < 4096,
            "known small fixture charge"
        );
        assert_eq!(
            runtime.models.contains_key(&anchor(&view, "c.glb")),
            height == 1023
        );
        if height == 1024 {
            assert!(runtime.cached_bytes() > 4 * MIB);
            assert!(runtime.cached_bytes() < CACHE_BYTES);
            assert!(runtime.status.contains("budget exhausted"));
        }
        drop(runtime);
    }
}

#[test]
fn completed_model_old_generation_is_joined_without_publishing_into_mixed_room() {
    let f = Fixture::new();
    f.file("a.glb", &fixture::control());
    f.file("b.png", &png_bytes(1, 1));
    let explorer = Explorer::new(&f.0).unwrap();
    let view = view(&explorer);
    let mut runtime = Runtime::default();
    runtime.room(&view);
    runtime.poll(Some(&explorer), Some(&view), false);
    assert!(model_eligible(
        &runtime.active.as_ref().unwrap().source.entry
    ));
    finished(&runtime.worker); // A real completed success is still owned, not injected.
    let generation = runtime.generation.as_ref().unwrap().clone();
    let attempt = runtime.active.as_ref().unwrap().attempt.clone();
    assert!(runtime.models.is_empty());
    runtime.room(&view);
    assert!(generation.load(Ordering::Relaxed) && attempt.load(Ordering::Relaxed));
    assert!(
        runtime.worker.busy() && runtime.active.is_some(),
        "cancel retains slot until joined"
    );
    runtime.poll(Some(&explorer), Some(&view), true);
    assert!(!runtime.worker.busy() && runtime.active.is_none());
    assert!(runtime.models.is_empty() && runtime.cache.is_empty());
    assert_eq!(runtime.acquisition_attempts, 1);
    drain(&mut runtime, &explorer, &view);
    assert_eq!((runtime.models.len(), runtime.cache.len()), (1, 1));
    assert_eq!(runtime.acquisition_attempts, 3);
    assert_eq!(runtime.models[&anchor(&view, "a.glb")].vertices.len(), 12);
    drop(runtime);
}

#[test]
fn completed_model_same_generation_rejects_changed_entry_index_or_anchor() {
    let f = Fixture::new();
    f.file("a.glb", &fixture::control());
    f.file("b.png", &png_bytes(1, 1));
    let explorer = Explorer::new(&f.0).unwrap();
    let original = view(&explorer);
    for change in ["entry", "index", "anchor"] {
        let mut runtime = Runtime::default();
        runtime.room(&original);
        runtime.poll(Some(&explorer), Some(&original), false);
        finished(&runtime.worker);
        let active = runtime.active.as_ref().unwrap();
        assert!(model_eligible(&active.source.entry));
        let id = active.source.anchor.clone();
        let generation = runtime.generation.as_ref().unwrap().clone();
        let mut rebound = original.clone();
        match change {
            "entry" => {
                rebound.bindings.get_mut(&id).unwrap().entry =
                    original.bindings[&anchor(&original, "b.png")].entry.clone()
            }
            "index" => rebound.bindings.get_mut(&id).unwrap().index += 1,
            "anchor" => {
                let bound = rebound.bindings.remove(&id).unwrap();
                rebound
                    .bindings
                    .insert("different-owned-slot".into(), bound);
            }
            _ => unreachable!(),
        }
        runtime.poll(Some(&explorer), Some(&rebound), true);
        assert!(Arc::ptr_eq(
            &generation,
            runtime.generation.as_ref().unwrap()
        ));
        assert!(
            !generation.load(Ordering::Relaxed),
            "same-generation identity guard under test"
        );
        assert!(
            runtime.models.is_empty() && runtime.cache.is_empty(),
            "STALE_MODEL_PUBLISHED: {change}"
        );
        assert!(!runtime.worker.busy() && runtime.active.is_none());
        assert_eq!(runtime.acquisition_attempts, 1);
        drop(runtime);
    }
    // Unmodified identity positive control through the same real scheduler.
    let mut runtime = Runtime::default();
    runtime.room(&original);
    drain(&mut runtime, &explorer, &original);
    assert_eq!(runtime.models.len(), 1);
    assert_eq!(runtime.cache.len(), 1);
    drop(runtime);
}

#[test]
fn cancelled_completed_model_reserves_same_slot_against_image_until_joined() {
    let f = Fixture::new();
    f.file("a.glb", &fixture::control());
    f.file("b.png", &png_bytes(1, 1));
    let explorer = Explorer::new(&f.0).unwrap();
    let view = view(&explorer);
    let model_index = view.bindings[&anchor(&view, "a.glb")].index;
    let image_index = view.bindings[&anchor(&view, "b.png")].index;
    let mut worker = image::Runtime::default();
    let cancelled = image::token();
    image::cancel(&cancelled);
    worker
        .start_model(
            cancelled.clone(),
            explorer.open_reader(model_index).unwrap(),
        )
        .unwrap();
    finished(&worker);
    assert!(
        worker.busy(),
        "completed cancellation still owns reservation until poll/join"
    );
    assert!(
        worker
            .start_object(
                image::token(),
                explorer.open_reader(image_index).unwrap(),
                hair::assets::LocalImageFormat::Png
            )
            .unwrap_err()
            .contains("occupied")
    );
    let (returned, result) = worker.poll_object().unwrap();
    assert!(Arc::ptr_eq(&returned, &cancelled));
    match result {
        Ok(_) => panic!("CANCELLED_MODEL_RESULT_PUBLISHED"),
        Err(error) => assert!(error.contains("cancelled"), "{error}"),
    }
    assert!(!worker.busy());
    worker
        .start_object(
            image::token(),
            explorer.open_reader(image_index).unwrap(),
            hair::assets::LocalImageFormat::Png,
        )
        .unwrap();
    finished(&worker);
    let (_, result) = worker.poll_object().unwrap();
    match result.unwrap() {
        image::Output::Image(texture) => assert_eq!(texture.rgba, [19, 37, 83, 255]),
        image::Output::Model(_) => panic!("image request returned model"),
    }
    assert!(!worker.busy());
    drop(worker);
}

#[test]
fn model_signature_refusal_is_not_retried_and_raw_bytes_remain_exact() {
    let f = Fixture::new();
    let bytes = b"not GLB\0\xff\n<script>inert</script>";
    f.file("a.GLB", bytes);
    f.file("b.png", &png_bytes(1, 1));
    let explorer = Explorer::new(&f.0).unwrap();
    let view = view(&explorer);
    let mut runtime = Runtime::default();
    runtime.room(&view);
    drain(&mut runtime, &explorer, &view);
    assert!(runtime.models.is_empty());
    assert_eq!(
        runtime.cache.len(),
        1,
        "model refusal must not poison following image"
    );
    assert_eq!(runtime.acquisition_attempts, 2);
    let bound = &view.bindings[&anchor(&view, "a.GLB")];
    assert_eq!(
        explorer
            .open_reader(bound.index)
            .unwrap()
            .read_chunk(0)
            .unwrap()
            .body(),
        bytes
    );
    assert_eq!(
        explorer
            .read_entry(bound.index, hair::local_explorer::MAX_BODY_BYTES)
            .unwrap(),
        bytes
    );
    // Repair ONLY owned fixture bytes: ordinary polling must not silently retry.
    fs::write(f.0.join("a.GLB"), fixture::control()).unwrap();
    for _ in 0..32 {
        runtime.poll(Some(&explorer), Some(&view), false);
    }
    assert!(runtime.models.is_empty());
    assert_eq!(runtime.acquisition_attempts, 2);
    runtime.room(&view); // Explicit lifecycle requeue is permitted.
    drain(&mut runtime, &explorer, &view);
    assert_eq!((runtime.models.len(), runtime.cache.len()), (1, 1));
    assert_eq!(runtime.acquisition_attempts, 4);
    assert_eq!(fs::read(f.0.join("a.GLB")).unwrap(), fixture::control());
    drop(runtime);
}
