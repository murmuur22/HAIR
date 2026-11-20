use crate::Result;
use std::{
    io::Read,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicU8>);
impl Cancellation {
    /// Returns true only when cancellation won before the atomic activation claim.
    /// Once committing, cancellation is too late and the completed result must be consumed.
    pub fn cancel(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn check(&self, deadline: Instant) -> Result<()> {
        if self.0.load(Ordering::Acquire) == 1 || Instant::now() >= deadline {
            Err("cancelled/deadline".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn claim_commit(&self, deadline: Instant) -> Result<()> {
        self.check(deadline)?;
        self.0
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| "cancelled/already committed".into())
    }
    pub(super) fn committed(&self) {
        self.0.store(3, Ordering::Release);
    }
}
pub(crate) struct Transport {
    client: reqwest::blocking::Client,
    pins: Vec<SocketAddr>,
    base: reqwest::Url,
}
fn resolve(host: &str, deadline: Instant) -> Result<Vec<std::net::IpAddr>> {
    let (config, _) = hickory_resolver::system_conf::read_system_conf()
        .map_err(|e| format!("system DNS configuration: {e}"))?;
    resolve_config(host, deadline, config)
}
fn resolve_config(
    host: &str,
    deadline: Instant,
    config: hickory_resolver::config::ResolverConfig,
) -> Result<Vec<std::net::IpAddr>> {
    use hickory_resolver::{
        TokioAsyncResolver,
        config::ResolverOpts,
        error::ResolveErrorKind,
        proto::{
            op::ResponseCode,
            rr::{Name, RData, RecordType},
        },
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let budget = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(2));
    let config = hickory_resolver::config::ResolverConfig::from_parts(
        None,
        vec![],
        config.name_servers().to_vec(),
    );
    runtime.block_on(async {
        tokio::time::timeout(budget, async {
            let mut opts = ResolverOpts::default();
            opts.timeout = Duration::from_millis(500);
            opts.attempts = 1;
            opts.use_hosts_file = false;
            opts.cache_size = 0;
            opts.num_concurrent_reqs = 1;
            let resolver = TokioAsyncResolver::tokio(config, opts);
            let name = Name::from_ascii(format!("{host}.")).map_err(|e| e.to_string())?;
            let mut ips = Vec::new();
            // Both families must finish, not a first-success fallback that hides bad answers.
            for ty in [RecordType::A, RecordType::AAAA] {
                match resolver.lookup(name.clone(), ty).await {
                    Ok(answer) => {
                        for r in answer.iter() {
                            match r {
                                RData::A(a) => ips.push(a.0.into()),
                                RData::AAAA(a) => ips.push(a.0.into()),
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        if !matches!(
                            e.kind(),
                            ResolveErrorKind::NoRecordsFound {
                                response_code: ResponseCode::NoError,
                                ..
                            }
                        ) {
                            return Err(format!("DNS {ty}: {e}"));
                        }
                    }
                }
            }
            if ips.is_empty() || ips.len() > 32 {
                return Err("DNS count refused".into());
            }
            Ok(ips)
        })
        .await
        .map_err(|_| "DNS deadline".to_string())?
    })
}
impl Transport {
    fn builder() -> reqwest::blocking::ClientBuilder {
        reqwest::blocking::Client::builder()
            .use_rustls_tls()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .http1_only()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .pool_max_idle_per_host(0)
            .referer(false)
    }
    pub(crate) fn public(
        source: &super::Source,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Self> {
        cancel.check(deadline)?;
        let host = source.url.host_str().ok_or("missing hostname")?;
        let ips = resolve(host, deadline)?;
        super::public_answers(&ips)?;
        cancel.check(deadline)?;
        let pins: Vec<_> = ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, source.url.port_or_known_default().unwrap_or(443)))
            .collect();
        let client = Self::builder()
            .resolve_to_addrs(host, &pins)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            pins,
            base: source.url.clone(),
        })
    }
    #[cfg(test)]
    pub(super) fn fixture(base: reqwest::Url, addr: SocketAddr, ca: Option<&[u8]>) -> Result<Self> {
        let mut b = Self::builder().resolve_to_addrs(base.host_str().unwrap(), &[addr]);
        if let Some(pem) = ca {
            b = b.tls_built_in_root_certs(false).add_root_certificate(
                reqwest::Certificate::from_pem(pem).map_err(|e| e.to_string())?,
            );
        }
        Ok(Self {
            client: b.build().map_err(|e| e.to_string())?,
            pins: vec![addr],
            base,
        })
    }
    pub(crate) fn get_page(
        &self,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<(Vec<u8>, String)> {
        self.get_typed(self.base.as_str(), 262144, None, cancel, deadline, true)
    }
    pub(super) fn get(
        &self,
        path: &str,
        limit: u64,
        exact: Option<u64>,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Vec<u8>> {
        self.get_typed(path, limit, exact, cancel, deadline, false)
            .map(|r| r.0)
    }
    fn get_typed(
        &self,
        path: &str,
        limit: u64,
        exact: Option<u64>,
        cancel: &Cancellation,
        deadline: Instant,
        page: bool,
    ) -> Result<(Vec<u8>, String)> {
        cancel.check(deadline)?;
        let url = self.base.join(path).map_err(|e| e.to_string())?;
        if url.origin() != self.base.origin() {
            return Err("cross-origin transport request refused".into());
        }
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(2));
        let mut response = self
            .client
            .get(url)
            .timeout(timeout)
            .send()
            .map_err(|e| format!("HTTPS: {e}"))?;
        if response
            .remote_addr()
            .is_none_or(|a| !self.pins.contains(&a))
        {
            return Err("transport peer not pinned".into());
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err(format!("HTTPS status refused: {}", response.status()));
        }
        for v in response
            .headers()
            .get_all(reqwest::header::CONTENT_ENCODING)
        {
            if v.as_bytes() != b"identity" {
                return Err("encoded body refused".into());
            }
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .unwrap_or("")
            .trim();
        let content_type = content_type.to_owned();
        if page
            && response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| {
                    v.split(';').skip(1).any(|p| {
                        let p = p.trim().to_ascii_lowercase();
                        p.starts_with("charset=")
                            && !matches!(
                                p.as_str(),
                                "charset=utf-8" | "charset=\"utf-8\"" | "charset=us-ascii"
                            )
                    })
                })
        {
            return Err("only UTF-8/ASCII page charset supported".into());
        }
        let allowed = if page {
            matches!(
                content_type.as_str(),
                "text/html" | "text/markdown" | "text/plain"
            )
        } else if path.ends_with(".png") {
            content_type == "image/png"
        } else {
            matches!(
                content_type.as_str(),
                "application/octet-stream"
                    | "text/plain"
                    | "text/markdown"
                    | "text/toml"
                    | "application/toml"
            )
        };
        if !allowed {
            return Err("content type refused".into());
        }
        if response
            .content_length()
            .is_some_and(|n| n > limit || exact.is_some_and(|e| n != e))
        {
            return Err("Content-Length refused".into());
        }
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            cancel.check(deadline)?;
            let remaining = (limit + 1)
                .saturating_sub(bytes.len() as u64)
                .min(chunk.len() as u64) as usize;
            if remaining == 0 {
                return Err("body overflow".into());
            }
            let n = response
                .read(&mut chunk[..remaining])
                .map_err(|e| format!("HTTPS read: {e}"))?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
            if bytes.len() as u64 > limit {
                return Err("body overflow".into());
            }
        }
        cancel.check(deadline)?;
        if exact.is_some_and(|n| n != bytes.len() as u64) {
            return Err("body length mismatch".into());
        }
        Ok((bytes, content_type))
    }
}

#[cfg(test)]
mod dns_tests {
    use super::*;
    #[test]
    #[ignore = "requires generated-CA local HTML server; not public PKI"]
    fn actual_html_page_transport_without_inventory() {
        let port: u16 = std::env::var("HAIR_WEB_PORT").unwrap().parse().unwrap();
        let ca = std::fs::read(std::env::var("HAIR_WEB_CA").unwrap()).unwrap();
        let raw = format!("https://fixture.example:{port}/ordinary?token=AB%2Fcd#top");
        let address = crate::web::Address::parse(&raw).unwrap();
        let t = Transport::fixture(
            address.url.clone(),
            format!("127.0.0.1:{port}").parse().unwrap(),
            Some(&ca),
        )
        .unwrap();
        let (bytes, mime) = t
            .get_page(
                &Cancellation::default(),
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(mime, "text/html");
        let page = crate::web::Page::html(address, &bytes).unwrap();
        assert_eq!(page.title, "Ordinary first");
        assert!(page.text.contains("Read without a companion"));
        assert_eq!(page.links.len(), 2);
    }
    use hickory_resolver::{
        config::{NameServerConfigGroup, ResolverConfig},
        proto::{
            op::{Message, MessageType},
            rr::{
                RData, Record, RecordType,
                rdata::{A, AAAA},
            },
        },
    };
    #[test]
    fn empty_dns_configuration_is_refused() {
        assert!(
            resolve_config(
                "fixture.example",
                Instant::now() + Duration::from_secs(1),
                ResolverConfig::new()
            )
            .is_err()
        );
    }
    #[test]
    fn actual_udp_dns_collects_both_families_before_mixed_answer_refusal() {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let port = socket.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let mut types = Vec::new();
            for _ in 0..2 {
                let mut buf = [0; 2048];
                let (n, peer) = socket.recv_from(&mut buf).unwrap();
                let query = Message::from_vec(&buf[..n]).unwrap();
                let q = query.queries()[0].clone();
                types.push(q.query_type());
                let mut response = Message::new();
                response
                    .set_id(query.id())
                    .set_message_type(MessageType::Response)
                    .set_recursion_desired(true)
                    .set_recursion_available(true)
                    .add_query(q.clone());
                let data = match q.query_type() {
                    RecordType::A => RData::A(A("93.184.216.34".parse().unwrap())),
                    RecordType::AAAA => RData::AAAA(AAAA("::1".parse().unwrap())),
                    _ => panic!("unexpected query"),
                };
                response.add_answer(Record::from_rdata(q.name().clone(), 60, data));
                socket.send_to(&response.to_vec().unwrap(), peer).unwrap();
            }
            types
        });
        let config = ResolverConfig::from_parts(
            None,
            vec![],
            NameServerConfigGroup::from_ips_clear(&["127.0.0.1".parse().unwrap()], port, false),
        );
        let answers = resolve_config(
            "fixture.example",
            Instant::now() + Duration::from_secs(2),
            config,
        );
        assert_eq!(
            worker.join().unwrap(),
            vec![RecordType::A, RecordType::AAAA]
        );
        let answers = answers.unwrap();
        assert_eq!(answers.len(), 2);
        assert!(
            super::super::public_answers(&answers).is_err(),
            "private AAAA cannot be hidden by public A success"
        );
    }
    #[test]
    fn dns_deadline_bounds_silent_udp_fixture() {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        let config = ResolverConfig::from_parts(
            None,
            vec![],
            NameServerConfigGroup::from_ips_clear(&["127.0.0.1".parse().unwrap()], port, false),
        );
        let before = Instant::now();
        assert!(
            resolve_config(
                "fixture.example",
                before + Duration::from_millis(50),
                config
            )
            .is_err()
        );
        assert!(before.elapsed() < Duration::from_secs(1));
        drop(socket);
    }
}
