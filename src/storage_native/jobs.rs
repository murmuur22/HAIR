//! One worker per held authority plus one replaceable selected-preview request.
//! No unfinished join in input/poll/Drop; adoption refuses until reconciliation.
use super::*;
use hair::{inventory::Inventory, local_explorer::{Kind, ReaderFile}};
use std::{sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread::JoinHandle};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation { Collect, Preview(u64) }
enum Work { Collect(std::ffi::OsString, ReaderFile), Preview(u64) }
enum Observation {
    Collect(std::result::Result<u64, Failure>, Result<()>),
    Preview(Result<()>),
    Unknown,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Activation { #[default] None, Committed, NotCommitted, Unknown }
impl Activation {
    fn label(self) -> &'static str {
        match self { Self::None => "no collect activation", Self::Committed => "COMMITTED", Self::NotCommitted => "Not committed", Self::Unknown => "UNKNOWN" }
    }
}
pub(super) struct Job {
    handle: Option<JoinHandle<(Inventory, Observation)>>,
    cancelled: Arc<AtomicBool>,
    operation: Operation,
    generation: u64,
    epoch: u64,
    authority: String,
}
impl Drop for Job {
    fn drop(&mut self) { self.cancelled.store(true, Ordering::Relaxed); }
}
#[cfg(test)]
fn checkpoint(gate: &Option<WorkerGate>, after: bool) {
    if let Some(gate) = gate && !gate.body && gate.after == after {
        let _ = gate.entered.send(());
        let _ = gate.release.recv();
    }
}
impl App {
    /// Post-event-loop retirement entrypoint; never called from frame/input code.
    pub(crate) fn shutdown_documents(&mut self, budget: Duration) -> bool {
        self.storage.shutting_down = true;
        self.cancel_documents();
        let start = Instant::now();
        loop {
            // Zero budget still reconciles completed-unpolled work. Only finished
            // handles are joined; the normal result path owns Store adoption.
            self.poll_documents();
            if self.storage.job.is_none() { break; }
            let remaining = budget.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                let job = self.storage.job.as_ref().unwrap();
                let activation = if job.operation == Operation::Collect { Activation::Unknown } else { self.storage.activation };
                self.local_status = format!("Documents UNSETTLED; collect activation {}; worker and held authority retained until process exit; recovery evidence may remain, preserve cache and inspect after safe restart; {}; last reconciled outcome: {}", activation.label(), job.authority, self.storage.outcome);
                return false;
            }
            std::thread::park_timeout(remaining.min(Duration::from_millis(1)));
        }
        let settled = !self.inventory.is_busy() && !self.storage.cache_blocked
            && self.storage.activation != Activation::Unknown;
        self.local_status = format!("Documents shutdown {}: {}; {}; preserved evidence={:?}; no automatic cleanup or retry",
            if settled { "settled" } else { "BLOCKED/UNKNOWN; safely restart and inspect held cache" },
            self.storage.activation.label(), self.storage.outcome,
            self.inventory.store().and_then(|store| store.evidence()));
        settled
    }
    #[cfg(test)]
    pub(crate) fn documents_cancel_token(&self) -> Arc<AtomicBool> {
        self.storage.job.as_ref().unwrap().cancelled.clone()
    }
    pub(crate) fn document_activity(&self) -> &'static str {
        match self.storage.job.as_ref() {
            Some(job) if job.cancelled.load(Ordering::Relaxed) => "Cancelling Documents work",
            Some(job) => match job.operation { Operation::Collect => "Loading collection", Operation::Preview(_) => "Loading selected preview" },
            None if self.storage.cache_blocked => "Documents blocked; inspect cache or safely restart",
            None => "Documents idle",
        }
    }
    pub(crate) fn require_documents_idle(&self) -> Result<()> {
        if self.storage.preferences_unknown {
            return Err("Preferences activation UNKNOWN; preserve both caches and settings and safely restart before explicit inspection/reopen".into());
        }
        if self.inventory.has_lost_authority() {
            return Err("Documents blocked; worker authority lost, activation UNKNOWN. Preserve cache evidence and safely restart before explicitly reopening; no in-process pathname fallback".into());
        }
        if self.storage.job.is_some() || self.inventory.is_busy() {
            return Err("Documents busy; no duplicate import or competing mutation/adoption started. Held cache retained; wait for reconciliation, then explicitly repeat the action; Escape explicitly cancels; no session fallback".into());
        }
        Ok(())
    }
    pub(crate) fn cancel_documents(&mut self) {
        self.storage.queued_preview = None;
        self.storage.generation = self.storage.generation.wrapping_add(1);
        if let Some(job) = &self.storage.job {
            job.cancelled.store(true, Ordering::Relaxed);
            self.local_status = "Cancelling Documents work; commit may already have occurred. Await authoritative result; originals unchanged; no cleanup".into();
        }
    }
    pub(crate) fn cancel_document_preview(&mut self) {
        self.storage.queued_preview = None;
        if self.storage.job.as_ref().is_some_and(|job| matches!(job.operation, Operation::Preview(_))) {
            self.cancel_documents();
        }
    }
    /// A committed source/view transition disowns UI intent, not held-source I/O.
    pub(crate) fn disown_document_intent(&mut self) {
        self.cancel_document_preview();
        self.storage.generation = self.storage.generation.wrapping_add(1);
    }
    pub(crate) fn start_document_collect(&mut self, index: usize) -> Result<()> {
        self.require_documents_idle()?;
        if self.storage.cache_blocked { return Err("Persistent Documents unavailable; explicitly refresh/reopen".into()); }
        let source = &self.local.as_ref().ok_or("Source directory closed")?.explorer;
        let entry = source.current().entries.get(index).ok_or("File no longer listed")?;
        if entry.kind != Kind::File { return Err("Only regular files can be collected".into()); }
        let name = entry.name.clone();
        // Metadata-only acquisition on the event thread. ReaderFile carries its
        // immutable dev/inode/length/timestamps, checked at every backend chunk.
        let held = source.open_reader(index)?;
        self.storage.generation = self.storage.generation.wrapping_add(1);
        self.start_document_work(Work::Collect(name, held))
    }
    pub(crate) fn request_document_preview(&mut self, id: u64) -> Result<()> {
        if !self.inventory.is_persistent() { return Ok(()); }
        if self.storage.cache_blocked { return Err("Persistent Documents unavailable; explicitly refresh/reopen".into()); }
        if let Some(job) = &self.storage.job {
            if job.operation == Operation::Preview(id) && !job.cancelled.load(Ordering::Relaxed) { return Ok(()); }
            if job.operation == Operation::Collect {
                // A deliberate selection wins over auto-selection on completion,
                // without changing the collect's cancellation token.
                self.storage.generation = self.storage.generation.wrapping_add(1);
            }
            // Latest preview intent never supersedes a durable collect.
            self.cancel_document_preview();
            self.storage.queued_preview = Some(id); // finite latest-only queue
            return Ok(());
        }
        if self.inventory.preview(id).is_some() { return Ok(()); }
        self.storage.generation = self.storage.generation.wrapping_add(1);
        self.start_document_work(Work::Preview(id))
    }
    fn start_document_work(&mut self, work: Work) -> Result<()> {
        if self.storage.shutting_down { return Err("Documents shutdown in progress; no new work admitted".into()); }
        let operation = match &work { Work::Collect(..) => Operation::Collect, Work::Preview(id) => Operation::Preview(*id) };
        let worker = self.inventory.take_worker()?;
        // Retain a recoverable owner if thread creation fails; never fallback to
        // Inventory::default or reopen/initialize a fresh Store.
        let transfer = Arc::new(Mutex::new(Some(worker)));
        let owned = transfer.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = cancelled.clone();
        let authority = format!("cache epoch {} ({})", self.storage.epoch,
            self.storage.preferences.path().map(|p| hair::local_explorer::escape_display(p.as_os_str())).unwrap_or_else(|| "held authority".into()));
        #[cfg(test)]
        let gate = self.storage.worker_gate.take();
        let handle = std::thread::Builder::new().name("documents-io".into()).spawn(move || {
            let mut worker = owned.lock().unwrap().take().unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                checkpoint(&gate, false);
                #[cfg(test)]
                let mut fired = false;
                let chunk = || {
                    #[cfg(test)]
                    if !fired && let Some(gate) = &gate && gate.body {
                        fired = true;
                        let _ = gate.entered.send(());
                        let _ = gate.release.recv();
                    }
                };
                let observation = match work {
                    Work::Collect(name, held) => {
                        let outcome = worker.collect_held_checkpoint(&name, &held, &token, chunk);
                        // Reconcile even after cancellation/ambiguous activation;
                        // no further action is admitted against the old snapshot.
                        let reconciled = worker.refresh();
                        Observation::Collect(outcome, reconciled)
                    }
                    Work::Preview(id) => {
                        Observation::Preview(worker.prepare_selected_checkpoint(id, &token, chunk))
                    }
                };
                #[cfg(test)]
                checkpoint(&gate, true);
                observation
            })).unwrap_or(Observation::Unknown);
            #[cfg(test)]
            if gate.as_ref().is_some_and(|gate| gate.panic_on_return) {
                panic!("fixture: join panic outside operation catch, held authority lost");
            }
            (worker, result)
        });
        match handle {
            Ok(handle) => {
                if operation == Operation::Collect { self.storage.activation = Activation::Unknown; }
                self.storage.job = Some(Job { handle: Some(handle), cancelled, operation,
                    generation: self.storage.generation, epoch: self.storage.epoch, authority });
                self.local_status = format!("{}; Escape cancels; original unchanged", self.document_activity());
                Ok(())
            }
            Err(error) => {
                self.inventory.return_worker(transfer.lock().unwrap().take().unwrap());
                Err(format!("Documents worker unavailable; held cache retained: {error}"))
            }
        }
    }
    pub(crate) fn poll_documents(&mut self) {
        if !self.storage.job.as_ref().is_some_and(|job| job.handle.as_ref().is_some_and(JoinHandle::is_finished)) { return; }
        let mut job = self.storage.job.take().unwrap();
        let Ok((mut worker, mut observation)) = job.handle.take().unwrap().join() else {
            self.inventory.mark_worker_lost();
            self.storage.cache_blocked = true;
            self.storage.activation = Activation::Unknown;
            self.storage.outcome = format!("Documents worker join failed; activation UNKNOWN; authority lost, actions blocked. Preserve evidence and safely restart before explicitly reopening; no retry or session fallback; {}", job.authority);
            self.local_status = self.storage.outcome.clone();
            return;
        };
        // Adoption is refused until this exact authority returns. Epoch is still
        // checked independently of UI intent; an old result never replaces it.
        if job.epoch != self.storage.epoch {
            self.inventory.mark_worker_lost();
            self.storage.cache_blocked = true;
            self.storage.activation = Activation::Unknown;
            let result = match observation {
                Observation::Collect(Ok(id), reconciled) => format!("Collection COMMITTED item-{id:016x}; reconciliation={reconciled:?}"),
                Observation::Collect(Err(error), _) => failure(error),
                Observation::Preview(_) => "Preview disowned".into(),
                Observation::Unknown => "Activation UNKNOWN".into(),
            };
            self.storage.outcome = format!("{}: {result}; old authority not installed; inspect old cache before retry", job.authority);
            self.local_status = self.storage.outcome.clone();
            return;
        }
        let current = job.generation == self.storage.generation && !job.cancelled.load(Ordering::Relaxed);
        let mut preview_current = matches!(job.operation, Operation::Preview(id) if self.storage.selected == Some(id));
        if let Operation::Preview(id) = job.operation && !self.storage.shutting_down {
            // Bounded metadata only: also catches peer removal/replacement after
            // decode completed but before the App polled. Body/export stays off
            // this admission path; synchronous metadata is a separate pending slice.
            let admission = if current && preview_current { worker.refresh_selected(id) }
                else { worker.refresh().map(|_| false) };
            match admission {
                Ok(true) => {},
                result => {
                    preview_current = false;
                    if current && self.storage.selected == Some(id) {
                        self.storage.selected = None;
                        self.inventory_selection = usize::MAX;
                    }
                    if let Err(error) = result {
                        self.storage.cache_blocked = true;
                        observation = Observation::Preview(Err(format!("Preview admission metadata unavailable: {error}")));
                    }
                }
            }
        }
        if !current || !preview_current { worker.clear_previews(); }
        self.inventory.return_worker(worker);
        match observation {
            Observation::Collect(result, reconciled) => {
                self.storage.activation = match &result {
                    Ok(_) => Activation::Committed,
                    Err(Failure::Refused(_)) => Activation::NotCommitted,
                    Err(Failure::Ambiguous(_)) => Activation::Unknown,
                };
                let uncertain = matches!(&result, Err(Failure::Ambiguous(_)));
                self.storage.cache_blocked = reconciled.is_err() || uncertain;
                self.storage.outcome = match result {
                    Ok(id) => {
                        if current && !self.storage.cache_blocked {
                            self.inventory_selection = self.inventory.items().iter().position(|item| item.id == id).unwrap_or(usize::MAX);
                            self.storage.selected = self.inventory.items().get(self.inventory_selection).map(|item| item.id);
                            if self.storage.selected == Some(id) { self.storage.queued_preview = Some(id); }
                        }
                        format!("Collection COMMITTED: item-{id:016x}; original unchanged{}{}", if current { "" } else { "; UI intent changed or cancellation requested; durable commit retained" },
                            self.inventory.notice().map(|notice| format!("; {notice}")).unwrap_or_default())
                    }
                    Err(error) => failure(error),
                };
                self.storage.outcome.push_str(&format!("; remaining evidence={:?}", self.inventory.store().and_then(|store| store.evidence())));
                self.storage.outcome.push_str(&format!("; {}", job.authority));
                if let Err(error) = reconciled { self.storage.outcome.push_str(&format!("; metadata reconciliation failed, actions blocked: {error}")); }
                self.local_status = self.storage.outcome.clone();
            }
            Observation::Preview(result) => {
                if !current || !preview_current {
                    self.local_status = "Preview cancelled/disowned; no stale result installed; cache metadata reconciled, select again if identity changed".into();
                } else {
                    self.local_status = match result {
                        Ok(()) => match self.storage.selected.and_then(|id| self.inventory.preview(id)) {
                            Some(preview) if !preview.content_derived || preview.warning.is_some() => format!("Representative preview; document remains collected; {}", preview.warning.as_deref().unwrap_or("not content verified")),
                            Some(_) => "Selected preview ready; document remains collected".into(),
                            None => "Selected preview unavailable; document remains collected".into(),
                        },
                        Err(error) => format!("Selected preview failed; document remains collected: {error}"),
                    };
                }
            }
            Observation::Unknown => {
                self.storage.cache_blocked = true;
                self.storage.activation = Activation::Unknown;
                self.storage.outcome = format!("Documents worker panicked; activation UNKNOWN. Preserve evidence; safely restart and explicitly inspect/reopen before retry; {}", job.authority);
                self.local_status = self.storage.outcome.clone();
            }
        }
        if !self.storage.shutting_down && !self.storage.cache_blocked && let Some(id) = self.storage.queued_preview.take()
            && self.storage.selected == Some(id) {
                // Keep durable outcome pinned separately while preview is loading.
                let status = self.local_status.clone();
                if let Err(error) = self.request_document_preview(id) { self.local_status = error; }
                else { self.local_status = status; }
            }
    }
    #[cfg(test)]
    pub(crate) fn documents_worker_count(&self) -> usize { usize::from(self.storage.job.is_some()) }
    #[cfg(test)]
    pub(crate) fn documents_worker_finished(&self) -> bool {
        self.storage.job.as_ref().is_some_and(|job| job.handle.as_ref().is_some_and(JoinHandle::is_finished))
    }
}
