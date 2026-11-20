use super::{
    Cache, Inventory, Source,
    transport::{Cancellation, Transport},
};
use crate::{
    Pose, Result, World,
    host_scene::HostScene,
    theme::{AssetCache, Theme},
};
use std::{
    path::PathBuf,
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct LocalSelection {
    pub packs: Vec<(PathBuf, bool)>,
    pub active: usize,
    pub resource_root: PathBuf,
    pub selected_site: Option<PathBuf>,
}
impl LocalSelection {
    fn validate(&self, cache: &Cache) -> Result<()> {
        if self.packs.is_empty() || self.packs.len() > 8 || self.active >= self.packs.len() {
            return Err("invalid local pack selection".into());
        }
        let mut protected = vec![self.resource_root.clone()];
        if let Some(site) = &self.selected_site {
            protected.push(site.clone());
        }
        for (p, _) in &self.packs {
            protected.push(p.parent().ok_or("pack parent absent")?.to_path_buf());
        }
        cache.protect(&protected)
    }
}
pub struct ActivatedSite {
    pub world: World,
    pub themes: Vec<Theme>,
    pub active_pack: usize,
    pub pose: Pose,
    pub source: Source,
    pub cache_path: PathBuf,
    pub durability_warning: Option<String>,
}
impl ActivatedSite {
    pub fn original_address(&self, relative: &str) -> Result<String> {
        super::validate_path(relative)?;
        Ok(self
            .source
            .url
            .join(relative)
            .map_err(|e| e.to_string())?
            .to_string())
    }
}
type Fetch<'a> = dyn Fn(&str, u64, Option<u64>) -> Result<Vec<u8>> + 'a;
fn acquire(
    cache: &Cache,
    source: Source,
    selection: LocalSelection,
    cancel: &Cancellation,
    deadline: Instant,
    fetch: &Fetch<'_>,
) -> Result<ActivatedSite> {
    use sha2::{Digest, Sha256};
    cancel.check(deadline)?;
    selection.validate(cache)?;
    let manifest = fetch("hair-site.toml", 32768, None)?;
    let inventory = Inventory::parse(&manifest)?;
    let mut files = Vec::new();
    for r in &inventory.files {
        cancel.check(deadline)?;
        let bytes = fetch(&r.path, r.bytes, Some(r.bytes))?;
        if bytes.len() as u64 != r.bytes || format!("{:x}", Sha256::digest(&bytes)) != r.sha256 {
            return Err(format!("resource hash/length mismatch: {}", r.path));
        }
        files.push((r.path.clone(), bytes));
    }
    cancel.check(deadline)?;
    let mut generation = cache.stage_checked(&files, &|| cancel.check(deadline))?;
    cancel.check(deadline)?;
    let world = World::load_site(&generation.path)?;
    cancel.check(deadline)?;
    let site = world.site.as_ref().ok_or("staged site absent")?;
    let expected_pages: std::collections::BTreeSet<_> = inventory
        .files
        .iter()
        .filter(|r| r.path.ends_with(".md"))
        .map(|r| r.path.as_str())
        .collect();
    let expected_images: std::collections::BTreeSet<_> = inventory
        .files
        .iter()
        .filter(|r| r.path.ends_with(".png"))
        .map(|r| r.path.as_str())
        .collect();
    if site
        .pages
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        != expected_pages
        || site
            .images
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            != expected_images
    {
        return Err("loaded ownership differs from inventory".into());
    }
    let host = if let Some(spaces) = &world.spaces {
        spaces.space(&world.current_space)?.scene.clone()
    } else {
        HostScene::load_site(&generation.path)?
    };
    let mut asset_cache = AssetCache::default();
    let themes = selection
        .packs
        .iter()
        .map(|(p, failed)| {
            cancel.check(deadline)?;
            let mut t = Theme::load_for_scene(p, &mut asset_cache, &host)?;
            cancel.check(deadline)?;
            t.load_failed = *failed;
            Ok(t)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut pose = Pose::default();
    themes[selection.active].respawn(&mut pose);
    if let Some(spaces) = &world.spaces {
        pose = spaces
            .destination(
                &spaces.entry_space,
                &spaces.entry_page,
                &spaces.entry_arrival,
                site,
            )?
            .1;
    }
    generation.check()?;
    // Re-read the exact loader inputs before commit; never trust a reused path/cache hit.
    for r in &inventory.files {
        cancel.check(deadline)?;
        let b = crate::content::read_local(&generation.path, &r.path, r.bytes)?;
        if b.len() as u64 != r.bytes || format!("{:x}", Sha256::digest(&b)) != r.sha256 {
            return Err("staged bytes changed".into());
        }
    }
    let durability_warning = cache.activate_guarded(
        &mut generation,
        &source.original,
        &format!("{:x}", Sha256::digest(&manifest)),
        || cancel.claim_commit(deadline),
    )?;
    cancel.committed();
    Ok(ActivatedSite {
        world,
        themes,
        active_pack: selection.active,
        pose,
        source,
        cache_path: generation.path.clone(),
        durability_warning,
    })
}
/// One bounded owned worker. Poll never waits for unfinished work or does disk/network IO.
/// Keep this owner alive, cancel then poll to acknowledge cleanup before shutdown.
pub struct Acquirer {
    cache: Arc<Cache>,
    job: Option<(Cancellation, JoinHandle<Result<ActivatedSite>>)>,
}
impl Acquirer {
    pub fn new(cache: Cache) -> Self {
        Self {
            cache: Arc::new(cache),
            job: None,
        }
    }
    pub fn start(&mut self, raw: &str, selection: LocalSelection) -> Result<()> {
        if self.job.is_some() {
            return Err("acquisition already in flight; cancel and reap first".into());
        }
        let source = Source::parse(raw)?;
        self.start_using(source, selection, Transport::public)
    }
    // Test fixtures replace only transport construction, never the transaction/worker.
    // No alternate trust roots or private-address bypass are exposed in the public API.
    fn start_using(
        &mut self,
        source: Source,
        selection: LocalSelection,
        transport: impl FnOnce(&Source, &Cancellation, Instant) -> Result<Transport> + Send + 'static,
    ) -> Result<()> {
        self.spawn(move |cache, cancel| {
            let deadline = Instant::now() + Duration::from_secs(30);
            selection.validate(&cache)?;
            let transport = transport(&source, &cancel, deadline)?;
            acquire(
                &cache,
                source,
                selection,
                &cancel,
                deadline,
                &|p, limit, exact| transport.get(p, limit, exact, &cancel, deadline),
            )
        })
    }
    fn spawn(
        &mut self,
        work: impl FnOnce(Arc<Cache>, Cancellation) -> Result<ActivatedSite> + Send + 'static,
    ) -> Result<()> {
        if self.job.is_some() {
            return Err("acquisition already in flight; cancel and reap first".into());
        }
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let cache = Arc::clone(&self.cache);
        let job = std::thread::Builder::new()
            .name("hair-https".into())
            .spawn(move || work(cache, worker_cancel))
            .map_err(|e| format!("spawn HTTPS worker: {e}"))?;
        self.job = Some((cancel, job));
        Ok(())
    }
    pub fn cancel(&self) -> bool {
        self.job.as_ref().is_some_and(|(c, _)| c.cancel())
    }
    pub fn poll(&mut self) -> Option<Result<ActivatedSite>> {
        if !self.job.as_ref()?.1.is_finished() {
            return None;
        }
        let (_, job) = self.job.take()?;
        Some(
            job.join()
                .unwrap_or_else(|_| Err("HTTPS worker panicked".into())),
        )
    }
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }
    /// Shutdown-only: cancel and join, returning any already committed completion.
    /// Unlike poll, this may wait on in-progress DNS/network/filesystem/decoder work.
    /// Do not call this or drop a running owner on a renderer/event-loop thread.
    pub fn shutdown(&mut self) -> Option<Result<ActivatedSite>> {
        self.cancel();
        let (_, job) = self.job.take()?;
        Some(
            job.join()
                .unwrap_or_else(|_| Err("HTTPS worker panicked".into())),
        )
    }
}
// Safety fallback: never silently detach a worker or abandon its cache descriptors.
// Normal integration must cancel + poll to idle, or move this owner to shutdown work.
impl Drop for Acquirer {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::Path;
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).into()
    }
    fn selection() -> LocalSelection {
        LocalSelection {
            packs: vec![
                (root().join("packs/office.toml"), false),
                (root().join("packs/archive.toml"), false),
            ],
            active: 1,
            resource_root: root(),
            selected_site: Some(root().join("examples/GreenleafReadingRoom-hallway")),
        }
    }
    pub(super) fn fixture() -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut files = std::collections::BTreeMap::from([
            (
                "index.md".into(),
                b"# First site\n[Second](second.md)".to_vec(),
            ),
            ("second.md".into(), b"# Second page".to_vec()),
        ]);
        let mut inv = "version = 1\n".to_string();
        for (p, b) in &files {
            inv += &format!(
                "[[files]]\npath = '{p}'\nbytes = {}\nsha256 = '{:x}'\n",
                b.len(),
                Sha256::digest(b)
            );
        }
        files.insert("hair-site.toml".into(), inv.into_bytes());
        files
    }
    #[test]
    fn one_worker_nonblocking_poll_cancel_reap_and_no_stale_completion() {
        let p = std::env::temp_dir().join(format!("https-worker-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        let p = p.canonicalize().unwrap();
        let mut owner = Acquirer::new(Cache::create_new(&p, "cache", &[root()]).unwrap());
        let (sent, received) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        owner
            .spawn(move |cache, cancel| {
                sent.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
                let files = fixture();
                acquire(
                    &cache,
                    Source::parse("https://fixture.example/").unwrap(),
                    selection(),
                    &cancel,
                    Instant::now() + Duration::from_secs(10),
                    &|p, _, _| files.get(p).cloned().ok_or("missing".into()),
                )
            })
            .unwrap();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        let before = Instant::now();
        assert!(owner.poll().is_none());
        assert!(before.elapsed() < Duration::from_millis(100));
        assert!(
            owner
                .start("https://fixture.example/", selection())
                .is_err(),
            "second worker refused"
        );
        assert!(owner.cancel());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = owner.poll() {
                assert!(result.is_err());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!owner.is_running());
        assert!(owner.poll().is_none());
        assert_eq!(std::fs::read_dir(p.join("cache")).unwrap().count(), 0);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn dropping_owner_cancels_and_joins_instead_of_detaching() {
        let p = std::env::temp_dir().join(format!("https-drop-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        let p = p.canonicalize().unwrap();
        let mut owner = Acquirer::new(Cache::create_new(&p, "cache", &[root()]).unwrap());
        let (sent, received) = std::sync::mpsc::channel();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed = done.clone();
        owner
            .spawn(move |_, cancel| {
                sent.send(()).unwrap();
                while cancel
                    .check(Instant::now() + Duration::from_secs(1))
                    .is_ok()
                {
                    std::thread::yield_now();
                }
                std::thread::sleep(Duration::from_millis(40));
                completed.store(true, std::sync::atomic::Ordering::Release);
                Err("cancelled".into())
            })
            .unwrap();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(owner);
        assert!(
            done.load(std::sync::atomic::Ordering::Acquire),
            "drop must not lose the join handle"
        );
        std::fs::remove_dir_all(p).unwrap();
    }
    fn reap(owner: &mut Acquirer) -> Result<ActivatedSite> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = owner.poll() {
                assert!(owner.poll().is_none());
                return result;
            }
            assert!(Instant::now() < deadline, "worker did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn worker_success_late_cancel_error_timeout_panic_and_restart_keep_lastgood() {
        let p = std::env::temp_dir().join(format!("https-worker-cycles-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        let p = p.canonicalize().unwrap();
        let mut owner = Acquirer::new(Cache::create_new(&p, "cache", &[root()]).unwrap());
        let (sent, received) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        owner
            .spawn(move |cache, cancel| {
                let files = fixture();
                let result = acquire(
                    &cache,
                    Source::parse("https://fixture.example/greenleaf/").unwrap(),
                    selection(),
                    &cancel,
                    Instant::now() + Duration::from_secs(5),
                    &|p, _, _| files.get(p).cloned().ok_or("missing".into()),
                );
                sent.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(2)).unwrap();
                result
            })
            .unwrap();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            !owner.cancel(),
            "committed result cannot be discarded as cancelled"
        );
        assert!(
            owner.poll().is_none(),
            "commit does not mean thread finished"
        );
        assert!(
            owner
                .start("https://fixture.example/", selection())
                .is_err()
        );
        release.send(()).unwrap();
        let good = reap(&mut owner).unwrap();
        assert_eq!(good.active_pack, 1);
        assert_eq!(good.themes[1].source, selection().packs[1].0);
        assert_eq!(
            good.original_address("second.md").unwrap(),
            "https://fixture.example/greenleaf/second.md"
        );
        let active = p.join("cache/active.toml");
        let before = std::fs::read(&active).unwrap();
        owner.spawn(|_, _| Err("fetch error".into())).unwrap();
        assert_eq!(reap(&mut owner).err().unwrap(), "fetch error");
        owner
            .spawn(|cache, cancel| {
                let files = fixture();
                acquire(
                    &cache,
                    Source::parse("https://fixture.example/").unwrap(),
                    selection(),
                    &cancel,
                    Instant::now(),
                    &|p, _, _| files.get(p).cloned().ok_or("missing".into()),
                )
            })
            .unwrap();
        assert!(reap(&mut owner).is_err());
        owner.spawn(|_, _| panic!("isolated worker panic")).unwrap();
        assert_eq!(reap(&mut owner).err().unwrap(), "HTTPS worker panicked");
        // Public start is exercised without external DNS/network: selection fails first.
        let mut invalid = selection();
        invalid.packs.clear();
        owner.start("https://fixture.example/", invalid).unwrap();
        assert!(reap(&mut owner).is_err());
        assert!(owner.start("http://fixture.example/", selection()).is_err());
        assert!(!owner.is_running());
        assert_eq!(std::fs::read(active).unwrap(), before);
        assert!(good.cache_path.join("index.md").exists());
        assert_eq!(std::fs::read_dir(p.join("cache")).unwrap().count(), 2);
        assert!(owner.shutdown().is_none());
        drop(owner);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    #[ignore = "explicit isolated generated-CA TLS worker fixture; not public PKI"]
    fn real_tls_worker_transaction_success_error_cancel_timeout_retains_lastgood() {
        let p = std::env::temp_dir().join(format!("https-tls-worker-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        let p = p.canonicalize().unwrap();
        let mut owner = Acquirer::new(Cache::create_new(&p, "cache", &[root()]).unwrap());
        let start = |owner: &mut Acquirer, mode: &str| {
            let port = std::env::var("HAIR_FIXTURE_PORT").unwrap();
            let ca = std::fs::read(std::env::var("HAIR_FIXTURE_CA").unwrap()).unwrap();
            let base = format!("https://fixture.example:{port}/{mode}/")
                .parse()
                .unwrap();
            let addr = format!("127.0.0.1:{port}").parse().unwrap();
            owner
                .start_using(
                    Source::parse(&format!("https://fixture.example/{mode}/")).unwrap(),
                    selection(),
                    move |_, _, _| Transport::fixture(base, addr, Some(&ca)),
                )
                .unwrap();
        };
        start(&mut owner, "good");
        let good = reap(&mut owner).unwrap();
        assert!(good.world.document().unwrap().contains("First site"));
        assert_eq!(good.active_pack, 1);
        assert_eq!(good.themes[1].source, selection().packs[1].0);
        assert_eq!(
            good.original_address("second.md").unwrap(),
            "https://fixture.example/good/second.md"
        );
        let before = std::fs::read(p.join("cache/active.toml")).unwrap();
        for mode in ["bad", "timeout", "cancel"] {
            start(&mut owner, mode);
            if mode == "cancel" {
                let marker =
                    PathBuf::from(std::env::var("HAIR_FIXTURE_ROOT").unwrap()).join("cancel-seen");
                let deadline = Instant::now() + Duration::from_secs(3);
                while !marker.exists() {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(2));
                }
                let before = Instant::now();
                assert!(owner.cancel());
                assert!(before.elapsed() < Duration::from_millis(100));
                assert!(owner.is_running());
                assert!(
                    owner
                        .start("https://fixture.example/", selection())
                        .is_err()
                );
            }
            let before_reap = Instant::now();
            assert!(reap(&mut owner).is_err());
            assert!(before_reap.elapsed() < Duration::from_secs(4));
            assert_eq!(std::fs::read(p.join("cache/active.toml")).unwrap(), before);
            assert!(good.cache_path.join("index.md").exists());
            assert_eq!(std::fs::read_dir(p.join("cache")).unwrap().count(), 2);
        }
        drop(owner);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn transaction_uses_existing_world_and_local_pack_then_atomic_lastgood() {
        let p = std::env::temp_dir().join(format!("https-transaction-{}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        let p = p.canonicalize().unwrap();
        let c = Cache::create_new(&p, "cache", &[root()]).unwrap();
        let files = fixture();
        let cancel = Cancellation::default();
        let run = |f: &std::collections::BTreeMap<String, Vec<u8>>, cancellation: &Cancellation| {
            acquire(
                &c,
                Source::parse("https://fixture.example/greenleaf/").unwrap(),
                selection(),
                cancellation,
                Instant::now() + Duration::from_secs(20),
                &|p, _, _| f.get(p).cloned().ok_or("missing file".into()),
            )
        };
        let good = run(&files, &cancel).unwrap();
        assert_eq!(good.active_pack, 1);
        assert_eq!(good.themes[1].source, selection().packs[1].0);
        assert!(good.world.document().unwrap().contains("First site"));
        assert_eq!(
            good.original_address("second.md").unwrap(),
            "https://fixture.example/greenleaf/second.md"
        );
        let active = c.path.join("active.toml");
        let before = std::fs::read(&active).unwrap();
        let mut bad = files.clone();
        bad.insert("index.md".into(), b"tampered".to_vec());
        assert!(run(&bad, &Cancellation::default()).is_err());
        let cancelled = Cancellation::default();
        assert!(cancelled.cancel());
        assert!(run(&files, &cancelled).is_err());
        assert!(
            run(&files, &cancel).is_err(),
            "completion cannot activate twice"
        );
        let mut invalid = selection();
        invalid.packs[1].0 = root().join("absent-pack.toml");
        assert!(
            acquire(
                &c,
                Source::parse("https://fixture.example/").unwrap(),
                invalid,
                &Cancellation::default(),
                Instant::now() + Duration::from_secs(20),
                &|p, _, _| files.get(p).cloned().ok_or("missing".into())
            )
            .is_err()
        );
        assert_eq!(std::fs::read(active).unwrap(), before);
        assert!(good.cache_path.join("index.md").exists());
        assert_eq!(
            std::fs::read_dir(&c.path).unwrap().count(),
            2,
            "failed generation cleaned, lastgood + pointer only"
        );
        std::fs::remove_dir_all(p).unwrap();
    }
}
