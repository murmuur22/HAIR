//! Lazy current-room PNG/JPEG, text, static GLB and tiny macOS H264 stills.
//! One shared owned worker; native tracked retirement can outlive Rust join.
//! Separate from the opened reader: no shared handles, pixels or reader tokens.
use super::image;
use hair::{
    assets::Texture,
    folder_scene,
    local_explorer::{Entry, Explorer, Kind},
};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::Arc,
};

const MIB: usize = 1024 * 1024;
const CACHE_BYTES: usize = 8 * MIB;
const ITEM_CAP: usize = 8;
// Conservative shared logical work budget; video uses a SUBSET, never an
// additive allowance: <=1 MiB Rust source + <=1 MiB tracked native source +
// <=8 MiB delivered live backing + <=512 chunks/64 requests + bounded raster,
// native output and Rust pixel copies (each <=160*96*4). Native private codec
// heaps/work are NOT bounded by this estimate. Existing JPEG estimate is32MiB.
// Reservation persists while worker.busy(), including tracked post-FFI state.
const WORK_RESERVATION: usize = 32 * MIB;
const TOTAL_BYTES: usize = 40 * MIB;
const MAX_RGBA: usize = 4 * MIB;

#[derive(Clone)]
struct Source {
    anchor: String,
    index: usize,
    entry: Entry, // original raw component, never display-derived
}
struct Active {
    generation: image::Token,
    attempt: image::Token,
    source: Arc<Source>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Queued,
    Working,
    Refused(&'static str),
    CpuReady,
    NotScheduled,
}
struct Inspection {
    generation: image::Token,
    // Share the scheduler's original binding; no extra path/body/payload copy.
    source: Arc<Source>,
    stage: Stage,
}
// Exact equality only: never echo an arbitrary worker/OS/parser error, even
// when it starts with a known prefix. All returned strings are fixed ASCII.
fn safe_reason(error: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "MP4 still: tracked native slot still occupied; raw bytes retained",
        "MP4 still: not a bounded MP4 ftyp source; raw bytes retained",
        "MP4 still: native owned-source allocation refused; raw bytes retained",
        "MP4 still: native reference/loader policy readback refused; raw bytes retained",
        "MP4 still: asset/track load refused or observation window elapsed; raw bytes retained",
        "MP4 still: only one H264 video track, identity transform, <=160x96 and <=2.02 seconds is supported; raw bytes retained",
        "MP4 still: image generator initialization refused; raw bytes retained",
        "MP4 still: first-frame observation window elapsed; cancellation is not a drain; raw bytes retained",
        "MP4 still: native first-frame image/error/time refused; raw bytes retained",
        "MP4 still: finite opaque sRGB RGBA raster refused; raw bytes retained",
        "MP4 still: held loader request/delivery budget refused; raw bytes retained",
        "MP4 still: unknown native refusal; raw bytes retained",
        "MP4 still invalid owned RGBA boundary result",
        "MP4 still native slot allocation refused",
        "MP4 still tracked native reservation occupied; no replacement",
        "MP4 still decoding requires macOS; raw bytes retained",
        "MP4 still request cancelled",
        "MP4 still reservation poisoned",
        "object/native work reservation occupied",
        "image worker reservation occupied",
        "object worker reservation occupied",
        "object image suffix/signature mismatch; fallback retained",
        "empty text prefix; pack fallback retained",
        "blank text prefix; pack fallback retained",
        "image worker panicked; result discarded",
    ];
    KNOWN
        .iter()
        .copied()
        .find(|known| *known == error)
        .unwrap_or("held-source acquisition or preview processing refused; raw bytes retained")
}
#[derive(Default)]
pub(crate) struct Runtime {
    worker: image::Runtime,
    generation: Option<image::Token>,
    active: Option<Active>,
    queue: VecDeque<Arc<Source>>,
    inspections: Vec<Inspection>, // <= PAGE_SIZE, current generation only
    native_pending: bool,         // last poll observation, never inferred from status
    pub cache: BTreeMap<String, Arc<Texture>>,
    // Separate typed map keeps existing literal/raster hooks useful. Both maps
    // share the SAME item/cache/work admission ledger and sole worker.
    pub models: BTreeMap<String, Arc<hair::assets::Model>>,
    pub status: String,
    dirty: bool,
    // Scheduler open_reader attempts only; bounded identity log, not syscall audit.
    #[cfg(test)]
    acquisition_attempts: usize,
    #[cfg(test)]
    acquisition_sources: Vec<Entry>,
}
fn object_format(entry: &Entry) -> Option<hair::assets::LocalImageFormat> {
    if entry.kind != Kind::File {
        return None;
    }
    let suffix = Path::new(&entry.name).extension()?.as_encoded_bytes();
    if suffix.eq_ignore_ascii_case(b"png") {
        Some(hair::assets::LocalImageFormat::Png)
    } else if suffix.eq_ignore_ascii_case(b"jpg") || suffix.eq_ignore_ascii_case(b"jpeg") {
        Some(hair::assets::LocalImageFormat::Jpeg)
    } else {
        None
    }
}
// Suffix is intent only, never file identity or encoding authority. Actual bytes
// come from open_reader on the held directory's original raw regular component.
fn text_eligible(entry: &Entry) -> bool {
    entry.kind == Kind::File
        && Path::new(&entry.name).extension().is_some_and(|suffix| {
            [b"txt".as_slice(), b"md".as_slice(), b"markdown".as_slice()]
                .iter()
                .any(|allowed| suffix.as_encoded_bytes().eq_ignore_ascii_case(allowed))
        })
}
fn model_eligible(entry: &Entry) -> bool {
    entry.kind == Kind::File
        && Path::new(&entry.name)
            .extension()
            .is_some_and(|suffix| suffix.as_encoded_bytes().eq_ignore_ascii_case(b"glb"))
}
fn video_eligible(entry: &Entry) -> bool {
    entry.kind == Kind::File
        && Path::new(&entry.name)
            .extension()
            .is_some_and(|suffix| suffix.as_encoded_bytes().eq_ignore_ascii_case(b"mp4"))
}
fn model_bytes(model: &hair::assets::Model) -> usize {
    // Preserve the private signature for inherited controls. A checked overflow
    // poisons admission; never wrap, panic or undercharge retained image capacity.
    hair::assets::retained_model_bytes(model).unwrap_or(usize::MAX)
}
fn eligible(entry: &Entry) -> bool {
    object_format(entry).is_some()
        || text_eligible(entry)
        || model_eligible(entry)
        || video_eligible(entry)
}
fn admits(bytes: usize, items: usize) -> bool {
    items < ITEM_CAP
        && bytes
            .checked_add(MAX_RGBA)
            .is_some_and(|n| n <= CACHE_BYTES)
        && bytes
            .checked_add(WORK_RESERVATION)
            .is_some_and(|n| n <= TOTAL_BYTES)
}
fn matches_source(source: &Source, view: &folder_scene::View) -> bool {
    view.bindings
        .get(&source.anchor)
        .is_some_and(|bound| bound.index == source.index && bound.entry == source.entry)
}
impl Runtime {
    fn inspection(&self, anchor: &str, bound: &folder_scene::BoundEntry) -> Option<&Inspection> {
        let generation = self.generation.as_ref()?;
        self.inspections.iter().find(|record| {
            Arc::ptr_eq(generation, &record.generation)
                && record.source.anchor == anchor
                && record.source.index == bound.index
                && record.source.entry == bound.entry
        })
    }
    fn record_stage(&mut self, generation: &image::Token, source: &Arc<Source>, stage: Stage) {
        if !self
            .generation
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, generation))
        {
            return;
        }
        if let Some(record) = self.inspections.iter_mut().find(|record| {
            Arc::ptr_eq(&record.generation, generation) && Arc::ptr_eq(&record.source, source)
        }) {
            record.stage = stage;
        }
    }
    fn refuse_queued(&mut self, reason: &'static str) {
        // Metadata only. Do not release worker/native reservations or set dirty.
        for record in &mut self.inspections {
            if record.stage == Stage::Queued
                && self
                    .generation
                    .as_ref()
                    .is_some_and(|g| Arc::ptr_eq(g, &record.generation))
            {
                record.stage = Stage::Refused(reason);
            }
        }
    }
    /// World I only: an on-demand bounded snapshot, not a live document, source
    /// acquisition, retry, native call or renderer/presentation observation.
    pub fn inspection_snapshot(
        &self,
        anchor: &str,
        bound: &folder_scene::BoundEntry,
        theme: &hair::theme::Theme,
    ) -> String {
        let record = self.inspection(anchor, bound);
        let stage = record.map_or(Stage::NotScheduled, |record| record.stage);
        let source = match stage {
            Stage::Queued => "queued/pending; no source acquired",
            Stage::Working => "active/working; worker pending; native result not observed",
            Stage::Refused(reason) => reason,
            Stage::CpuReady => "CPU cache ready; not proof of a visible image",
            Stage::NotScheduled if !eligible(&bound.entry) => "unsupported/not scheduled",
            Stage::NotScheduled => "not scheduled for this current binding",
        };
        // Only a current matching record with actual cache admission may query
        // Theme identity. Cache presence alone is never Theme admission.
        let admitted = stage == Stage::CpuReady
            && (self
                .cache
                .get(anchor)
                .is_some_and(|image| theme.folder_preview_admitted(anchor, Some(image), None))
                || self
                    .models
                    .get(anchor)
                    .is_some_and(|model| theme.folder_preview_admitted(anchor, None, Some(model))));
        let refusal = if matches!(stage, Stage::Refused(_)) {
            "refused; "
        } else {
            ""
        };
        let theme = if admitted { "admitted" } else { "not admitted" };
        let reservation = if record.is_some() && self.native_pending {
            "\nShared native reservation: pending at last poll; remains charged; no retry"
        } else {
            ""
        };
        // Everything interpolated is fixed public vocabulary, never an entry,
        // path, raw bytes, arbitrary error, token, pointer or payload dump.
        format!(
            "\nPREVIEW STATUS (snapshot at inspection; not live monitoring):\n{refusal}{source}\nTheme: {theme} (CPU admission map only){reservation}\nRendering/presentation: unverified; no owner-process thumbnail claim\n"
        )
    }
    fn cached_bytes(&self) -> usize {
        // Raster charges stay length-based; models use the full retained helper.
        self.cache
            .values()
            .map(|t| t.rgba.len())
            .chain(self.models.values().map(|m| model_bytes(m)))
            .try_fold(0_usize, |sum, bytes| sum.checked_add(bytes))
            .unwrap_or(usize::MAX)
    }
    fn cached_items(&self) -> usize {
        self.cache.len().saturating_add(self.models.len())
    }
    pub fn invalidate(&mut self) {
        if let Some(generation) = self.generation.take() {
            image::cancel(&generation);
        }
        if let Some(active) = &self.active {
            image::cancel(&active.attempt);
        }
        // Do NOT drop active/worker; cancellation cannot release its reservation.
        self.queue.clear();
        self.inspections.clear();
        self.native_pending = false;
        self.cache.clear();
        self.models.clear();
        self.status.clear();
        self.dirty = true;
    }
    pub fn room(&mut self, view: &folder_scene::View) {
        self.invalidate();
        let generation = image::token();
        self.generation = Some(generation.clone());
        // Only the already-projected page, in deterministic binding order, <=12.
        self.queue = view
            .bindings
            .iter()
            .take(folder_scene::PAGE_SIZE)
            .filter(|(_, bound)| eligible(&bound.entry))
            .map(|(anchor, bound)| {
                Arc::new(Source {
                    anchor: anchor.clone(),
                    index: bound.index,
                    entry: bound.entry.clone(),
                })
            })
            .collect();
        self.inspections = self
            .queue
            .iter()
            .map(|source| Inspection {
                generation: generation.clone(),
                source: source.clone(),
                stage: Stage::Queued,
            })
            .collect();
        self.status = if self
            .queue
            .iter()
            .any(|source| video_eligible(&source.entry))
        {
            "MP4 still intent: tiny H264 or fallback; scoped native safety, not arbitrary containment or codec memory/work bounds; no playback".into()
        } else {
            "PNG/JPEG/text/static GLB objects: current-room lazy reads; fallback on refusal".into()
        };
    }
    pub fn poll(
        &mut self,
        explorer: Option<&Explorer>,
        view: Option<&folder_scene::View>,
        paused: bool,
    ) -> bool {
        if let Some((attempt, result)) = self.worker.poll_object()
            && let Some(active) = self.active.take()
        {
            let current = self
                .generation
                .as_ref()
                .is_some_and(|g| Arc::ptr_eq(g, &active.generation));
            if current
                && Arc::ptr_eq(&attempt, &active.attempt)
                && view.is_some_and(|v| matches_source(&active.source, v))
            {
                match result {
                    Ok(output) => {
                        let retained = match &output {
                            image::Output::Image(t) => t.rgba.len(),
                            image::Output::Model(m) => model_bytes(m),
                        };
                        if retained <= MAX_RGBA
                            && self
                                .cached_bytes()
                                .checked_add(retained)
                                .is_some_and(|bytes| {
                                    bytes <= CACHE_BYTES
                                        && bytes
                                            .checked_add(WORK_RESERVATION)
                                            .is_some_and(|total| total <= TOTAL_BYTES)
                                })
                            && self.cached_items() < ITEM_CAP
                        {
                            self.record_stage(&active.generation, &active.source, Stage::CpuReady);
                            match output {
                                image::Output::Image(t) => {
                                    self.cache.insert(active.source.anchor.clone(), Arc::new(t));
                                }
                                image::Output::Model(m) => {
                                    self.models
                                        .insert(active.source.anchor.clone(), Arc::new(m));
                                }
                            }
                            self.dirty = true;
                        } else {
                            self.record_stage(
                                &active.generation,
                                &active.source,
                                Stage::Refused(
                                    "object cache budget refused; pack fallback retained",
                                ),
                            );
                            self.status =
                                "object cache budget refused; pack fallback retained".into();
                        }
                    }
                    Err(error) => {
                        self.record_stage(
                            &active.generation,
                            &active.source,
                            Stage::Refused(safe_reason(&error)),
                        );
                        self.status = format!("object fallback: {error}");
                    }
                }
            }
        }
        self.native_pending = self.worker.native_retirement_pending();
        if self.native_pending && !self.queue.is_empty() {
            // Shared 32 MiB reservation remains occupied AFTER Rust join.
            // Refuse queued work once; don't allocate another native context or
            // automatically retry any refused source when reclamation changes.
            self.refuse_queued(
                "tracked native reclamation unobserved; shared work slot remains charged; no retry",
            );
            self.queue.clear();
            self.status = "object fallback: tracked native reclamation unobserved; shared work slot remains charged; no retry".into();
        }
        // One acquisition per poll, no retained source FD in the queue. Ordinary
        // openat/fstat can stall on OS/network filesystems; this is not a deadline.
        if !paused
            && !self.worker.busy()
            && let (Some(explorer), Some(view), Some(generation)) =
                (explorer, view, self.generation.clone())
            && let Some(source) = self.queue.pop_front()
        {
            if !admits(self.cached_bytes(), self.cached_items()) {
                // Includes the just-popped record, whose stage is still queued.
                self.refuse_queued("object budget exhausted; remaining objects keep pack fallback");
                self.queue.clear(); // deterministic no eviction/retry churn
                self.status =
                    "object budget exhausted; remaining objects keep pack fallback".into();
            } else if matches_source(&source, view)
                && explorer.current().entries.get(source.index) == Some(&source.entry)
            {
                let attempt = image::token();
                #[cfg(test)]
                {
                    self.acquisition_attempts += 1;
                    if self.acquisition_sources.len() < 16 {
                        self.acquisition_sources.push(source.entry.clone());
                    }
                }
                let mut source_opened = false;
                match explorer.open_reader(source.index).and_then(|file| {
                    source_opened = true;
                    if text_eligible(&source.entry) {
                        self.worker.start_text(attempt.clone(), file)
                    } else if model_eligible(&source.entry) {
                        self.worker.start_model(attempt.clone(), file)
                    } else if video_eligible(&source.entry) {
                        self.worker.start_video(attempt.clone(), file)
                    } else {
                        self.worker.start_object(
                            attempt.clone(),
                            file,
                            object_format(&source.entry)
                                .ok_or("object format no longer eligible")?,
                        )
                    }
                }) {
                    Ok(()) => {
                        self.record_stage(&generation, &source, Stage::Working);
                        self.active = Some(Active {
                            generation,
                            attempt,
                            source,
                        });
                    }
                    Err(error) => {
                        let reason = if source_opened {
                            "object worker start refused; pack fallback retained"
                        } else {
                            "held source open refused; pack fallback retained"
                        };
                        self.record_stage(&generation, &source, Stage::Refused(reason));
                        self.status = format!("object source refused; fallback: {error}");
                    }
                }
            } else {
                self.record_stage(&generation, &source, Stage::NotScheduled);
            }
        }
        std::mem::take(&mut self.dirty)
    }
}
// Read-only observation for the App-level R1 regression; no scheduler driving,
// cache injection, result substitution or production-state mutation.
#[cfg(test)]
impl Runtime {
    pub(crate) fn test_model_fallback_progress(&self) -> (usize, bool) {
        (
            self.acquisition_attempts,
            self.queue.is_empty() && self.active.is_none() && !self.worker.busy(),
        )
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.invalidate();
    } // worker Drop joins; no timeout claim
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "model_safety_tests.rs"]
mod model_safety_tests;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "text_safety_tests.rs"]
mod text_safety_tests;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "video_safety_tests.rs"]
mod video_safety_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_logical_budget_is_reserved_with_full_cache() {
        let work = std::hint::black_box(WORK_RESERVATION);
        let total = std::hint::black_box(TOTAL_BYTES);
        assert!(
            work >= 32 * MIB,
            "JPEG decoder logical budget must be reserved before acquisition"
        );
        assert!(total >= work + CACHE_BYTES);
    }
    #[test]
    fn aggregate_reservation_and_item_cap_are_explicit() {
        assert!(admits(4 * MIB, 1));
        assert!(!admits(4 * MIB + 1, 1));
        assert!(!admits(0, ITEM_CAP));
        assert!(!admits(usize::MAX, 0));
    }
    #[test]
    fn reused_slot_and_name_do_not_reuse_generation() {
        let mut runtime = Runtime::default();
        runtime.generation = Some(image::token());
        let old = runtime.generation.as_ref().unwrap().clone();
        runtime.invalidate();
        runtime.generation = Some(image::token());
        assert!(!Arc::ptr_eq(&old, runtime.generation.as_ref().unwrap()));
        assert!(old.load(std::sync::atomic::Ordering::Relaxed));
        assert!(runtime.queue.is_empty() && runtime.cache.is_empty());
    }
    #[test]
    fn suffix_is_only_a_raw_component_gate() {
        let mut entry = Entry {
            name: "literal\nÉ.PNG".into(),
            path: "not-authority.txt".into(),
            kind: Kind::File,
        };
        assert!(eligible(&entry));
        entry.kind = Kind::Symlink;
        assert!(!eligible(&entry));
        entry.kind = Kind::File;
        entry.name = "image.jpeg".into();
        assert!(eligible(&entry));
        assert_eq!(
            object_format(&entry),
            Some(hair::assets::LocalImageFormat::Jpeg)
        );
        entry.name = "image.bin".into();
        assert!(!eligible(&entry));
    }
    fn inspection_view() -> folder_scene::View {
        folder_scene::View {
            file_names: Default::default(),
            bindings: (0..2)
                .map(|index| {
                    (
                        format!("entry-{index}"),
                        folder_scene::BoundEntry {
                            index,
                            slot_index: index,
                            entry: Entry {
                                name: "private\nname.mp4".into(),
                                path: "/private/not-for-diagnostics".into(),
                                kind: Kind::File,
                            },
                        },
                    )
                })
                .collect(),
            continuations: Default::default(),
            parent: None,
            page: 0,
            count: 2,
            source: String::new(),
            reader: String::new(),
        }
    }
    #[test]
    fn inspection_rejects_rebound_identity_old_generation_and_old_source() {
        let view = inspection_view();
        let mut runtime = Runtime::default();
        runtime.room(&view);
        let old_generation = runtime.generation.as_ref().unwrap().clone();
        let old_source = runtime.queue.front().unwrap().clone();
        let bound = &view.bindings[&old_source.anchor];
        assert_eq!(
            runtime.inspection(&old_source.anchor, bound).unwrap().stage,
            Stage::Queued
        );
        let mut rebound = bound.clone();
        rebound.index += 1;
        assert!(runtime.inspection(&old_source.anchor, &rebound).is_none());
        rebound = bound.clone();
        rebound.entry.path = "/different-binding".into();
        assert!(runtime.inspection(&old_source.anchor, &rebound).is_none());
        runtime.invalidate();
        assert!(runtime.inspections.is_empty());
        assert!(runtime.inspection(&old_source.anchor, bound).is_none());
        runtime.room(&view);
        runtime.record_stage(&old_generation, &old_source, Stage::Refused("old refusal"));
        let current_generation = runtime.generation.as_ref().unwrap().clone();
        runtime.record_stage(
            &current_generation,
            &old_source,
            Stage::Refused("wrong source"),
        );
        assert_eq!(
            runtime.inspection(&old_source.anchor, bound).unwrap().stage,
            Stage::Queued
        );
        // An old record cannot satisfy a query merely by keeping its binding.
        runtime.inspections[0].generation = old_generation;
        assert!(runtime.inspection(&old_source.anchor, bound).is_none());
    }
    #[test]
    fn queued_refusal_preserves_completed_cause_and_global_status_is_not_authority() {
        let view = inspection_view();
        let mut runtime = Runtime::default();
        runtime.room(&view);
        let generation = runtime.generation.as_ref().unwrap().clone();
        let source = runtime.queue.pop_front().unwrap();
        let reason = safe_reason("MP4 still: not a bounded MP4 ftyp source; raw bytes retained");
        runtime.record_stage(&generation, &source, Stage::Refused(reason));
        runtime.status = "unrelated successful peer".into();
        runtime.dirty = false;
        runtime.refuse_queued("tracked native reclamation unobserved; no retry");
        assert_eq!(
            runtime
                .inspection(&source.anchor, &view.bindings[&source.anchor])
                .unwrap()
                .stage,
            Stage::Refused(reason)
        );
        assert_eq!(
            runtime.inspections[1].stage,
            Stage::Refused("tracked native reclamation unobserved; no retry")
        );
        assert!(!runtime.dirty);
        // Budget refusal includes an item popped before admission, not just
        // the remaining deque. These are pure record-state tests, not decoding.
        runtime.room(&view);
        let popped = runtime.queue.pop_front().unwrap();
        runtime.refuse_queued("object budget exhausted; remaining objects keep pack fallback");
        assert!(
            runtime
                .inspections
                .iter()
                .all(|record| matches!(record.stage, Stage::Refused(_)))
        );
        assert!(
            runtime
                .inspection(&popped.anchor, &view.bindings[&popped.anchor])
                .is_some()
        );
    }
    #[test]
    fn public_refusals_are_fixed_bounded_ascii_and_redact_arbitrary_errors() {
        let code2 = "MP4 still: not a bounded MP4 ftyp source; raw bytes retained";
        let profile = "MP4 still: only one H264 video track, identity transform, <=160x96 and <=2.02 seconds is supported; raw bytes retained";
        assert_eq!(safe_reason(code2), code2);
        assert_eq!(safe_reason(profile), profile);
        for error in [
            "private\nJSON: {secret}\u{1b}/private/source.mp4",
            "MP4 still: not a bounded MP4 ftyp source; raw bytes retained\nSECRET",
            "unknown native refusal with private decoder data",
        ] {
            let public = safe_reason(error);
            assert_eq!(
                public,
                "held-source acquisition or preview processing refused; raw bytes retained"
            );
            assert!(public.is_ascii() && public.len() <= 192);
            assert!(!public.bytes().any(|byte| byte.is_ascii_control()));
        }
        assert!(safe_reason(profile).len() <= 192);
    }
    #[test]
    fn room_intents_are_bounded_and_rebinding_checks_raw_source() {
        let entry = Entry {
            name: "source.PNG".into(),
            path: "display-only".into(),
            kind: Kind::File,
        };
        let mut view = folder_scene::View {
            file_names: Default::default(),
            bindings: (0..24)
                .map(|index| {
                    (
                        format!("entry-{index}"),
                        folder_scene::BoundEntry {
                            index,
                            slot_index: index,
                            entry: entry.clone(),
                        },
                    )
                })
                .collect(),
            continuations: Default::default(),
            parent: None,
            page: 0,
            count: 24,
            source: String::new(),
            reader: String::new(),
        };
        let mut runtime = Runtime::default();
        runtime.room(&view);
        assert_eq!(runtime.queue.len(), folder_scene::PAGE_SIZE);
        assert_eq!(runtime.inspections.len(), folder_scene::PAGE_SIZE);
        assert!(
            runtime
                .inspections
                .iter()
                .zip(&runtime.queue)
                .all(|(record, source)| {
                    Arc::ptr_eq(&record.source, source) && record.stage == Stage::Queued
                })
        );
        let source = runtime.queue.front().unwrap().clone();
        assert!(matches_source(&source, &view));
        view.bindings.get_mut(&source.anchor).unwrap().entry.name = "different.PNG".into();
        assert!(!matches_source(&source, &view));
    }
}
