//! Static HTTPS acquisition. Not wired to URL entry or renderer.
use crate::Result;
#[derive(Debug)]
pub struct Source {
    pub original: String,
    pub(crate) url: reqwest::Url,
}
impl Source {
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.len() > 2048
            || !raw.starts_with("https://")
            || !raw.is_ascii()
            || raw.bytes().any(|b| b.is_ascii_control() || b == b' ')
        {
            return Err("literal ASCII HTTPS directory required".into());
        }
        let rest = &raw[8..];
        let (host, path) = rest.split_once('/').ok_or("directory slash required")?;
        if host.len() > 253
            || !host.contains('.')
            || host.parse::<std::net::IpAddr>().is_ok()
            || host.split('.').any(|s| {
                s.is_empty()
                    || s.len() > 63
                    || s.starts_with('-')
                    || s.ends_with('-')
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
            || !host
                .rsplit('.')
                .next()
                .unwrap_or("")
                .bytes()
                .all(|b| b.is_ascii_lowercase())
            || [
                ".local",
                ".localhost",
                ".internal",
                ".home",
                ".test",
                ".invalid",
                ".onion",
            ]
            .iter()
            .any(|s| host.ends_with(s))
        {
            return Err(
                "ordinary lowercase public DNS hostname required; no credentials or explicit port"
                    .into(),
            );
        }
        if !path.is_empty() {
            let segments = path
                .strip_suffix('/')
                .ok_or("directory must end in slash")?;
            validate_path(segments)?;
        }
        let url = reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
        if url.as_str() != raw {
            return Err("URL normalization refused".into());
        }
        Ok(Self {
            original: raw.into(),
            url,
        })
    }
}
fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 240
        || path.split('/').count() > 8
        || path.split('/').any(|s| {
            s.is_empty()
                || s.len() > 64
                || s.starts_with('.')
                || s.ends_with('.')
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
        })
    {
        return Err("ambiguous/unsafe path refused".into());
    }
    Ok(())
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub version: u32,
    pub files: Vec<Resource>,
}
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
impl Inventory {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 32768 {
            return Err("inventory exceeds 32 KiB".into());
        }
        let inv: Self = toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if inv.version != 1 || inv.files.is_empty() || inv.files.len() > 128 {
            return Err("inventory version/count refused".into());
        }
        let mut names = std::collections::BTreeSet::new();
        let mut dirs = std::collections::BTreeSet::new();
        let (mut total, mut pages, mut images) = (bytes.len() as u64, 0, 0);
        for f in &inv.files {
            validate_path(&f.path)?;
            let limit = if f.path.ends_with(".md") {
                pages += 1;
                16384
            } else if f.path.ends_with(".png") {
                images += 1;
                262144
            } else if f.path == "host-scene.toml" {
                131072
            } else if f.path == "site-navigation.toml" {
                16384
            } else {
                return Err("unsupported inventory resource".into());
            };
            if f.bytes > limit
                || f.sha256.len() != 64
                || !f
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !names.insert(f.path.clone())
            {
                return Err("resource size/hash/duplicate refused".into());
            }
            let parts: Vec<_> = f.path.split('/').collect();
            for n in 1..parts.len() {
                if parts[n - 1].contains('.') {
                    return Err("file-like directory refused".into());
                }
                dirs.insert(parts[..n].join("/"));
            }
            total = total.checked_add(f.bytes).ok_or("budget overflow")?;
        }
        if pages == 0
            || pages > 32
            || images > 16
            || total > 2_097_152
            || names.len() + dirs.len() > 128
            || dirs.iter().any(|d| names.contains(d))
        {
            return Err("inventory aggregate/count/path collision refused".into());
        }
        Ok(inv)
    }
}
fn public_answers(ips: &[std::net::IpAddr]) -> Result<()> {
    use std::net::IpAddr;
    let public = |ip: &IpAddr| match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192
                    && (b == 168
                        || (b == 0 && (c == 0 || c == 2))
                        || (b == 88 && c == 99)
                        || (b == 31 && c == 196)
                        || (b == 52 && c == 193)
                        || (b == 175 && c == 48)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
                && !(s[0] == 0x2620 && s[1] == 0x004f && s[2] == 0x8000)
        }
    };
    if ips.is_empty() || ips.len() > 32 || !ips.iter().all(public) {
        Err("DNS empty/excessive/nonpublic answer refused".into())
    } else {
        Ok(())
    }
}
mod transaction;
pub use transaction::{Acquirer, ActivatedSite, LocalSelection};
pub use transport::Cancellation;
mod cache;
pub use cache::Cache;
pub(crate) mod transport;
#[cfg(test)]
mod transport_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(path: &str, size: u64) -> String {
        format!(
            "[[files]]\npath = '{path}'\nbytes = {size}\nsha256 = '{}'\n",
            "a".repeat(64)
        )
    }
    #[test]
    fn inventory_refuses_ambiguous_aliases_unsupported_data_and_budgets() {
        let good = format!("version = 1\n{}", entry("index.md", 20));
        assert!(Inventory::parse(good.as_bytes()).is_ok());
        let mut bad = vec![
            good.replace("version = 1", "version = 2"),
            "version = 1\nfiles = []".into(),
            good.replace("bytes = 20", "bytes = 16385"),
            good.replace(&"a".repeat(64), &"A".repeat(64)),
            format!("{good}{}", entry("index.md", 20)),
        ];
        for p in [
            "Index.md",
            "x.js",
            "x.glb",
            "x.zip",
            "x.toml",
            "hair-site.toml",
            "../index.md",
            "a/./x.md",
            "a//x.md",
            "/x.md",
            "x%2emd",
            "x\\y.md",
            "a.md/b.md",
            "a:foo.md",
            "a./x.md",
        ] {
            bad.push(format!("version = 1\n{}", entry(p, 20)));
        }
        bad.push(format!("{good}{}", entry("index.md/x.md", 1)));
        bad.push(format!(
            "version = 1\n{}",
            (0..33)
                .map(|i| entry(&format!("p{i}.md"), 1))
                .collect::<String>()
        ));
        bad.push(format!(
            "version = 1\n{}",
            (0..9)
                .map(|i| entry(&format!("p{i}.png"), 262144))
                .collect::<String>()
        ));
        for b in bad {
            assert!(Inventory::parse(b.as_bytes()).is_err(), "accepted {b}");
        }
    }
    #[test]
    fn every_dns_answer_must_be_public_including_mapped_and_special_ranges() {
        use std::net::IpAddr;
        let public: IpAddr = "93.184.216.34".parse().unwrap();
        assert!(public_answers(&[public, "2606:4700:4700::1111".parse().unwrap()]).is_ok());
        assert!(public_answers(&[]).is_err());
        for ip in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.0.9",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:93.184.216.34",
            "64:ff9b::a00:1",
            "100::1",
            "2001::1",
            "2001:db8::1",
            "2002:0808:0808::1",
            "3fff::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
        ] {
            assert!(
                public_answers(&[public, ip.parse().unwrap()]).is_err(),
                "accepted mixed answer {ip}"
            );
        }
    }
    #[test]
    fn literal_https_directory_rejects_normalization_and_authority_ambiguity() {
        for raw in [
            "http://example.com/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://2130706433/",
            "https://localhost/",
            "https://a.local/",
            "https://a.example:444/",
            "https://a.example:443/",
            "https://u:p@a.example/",
            "https://a.example/?x",
            "https://a.example/#x",
            "https://a.example/a/../",
            "https://a.example/%2e/",
            "https://a.example//",
            "https://a.example/a\\b/",
            "https://a.example/A/",
            "https://a.example/a",
            "https://EXAMPLE.com/",
            "https://é.example/",
            "https://example.com./",
            " https://example.com/",
            "https://example.com/\n",
        ] {
            assert!(Source::parse(raw).is_err(), "accepted {raw:?}");
        }
        let s = Source::parse("https://www.example.com/greenleaf/").unwrap();
        assert_eq!(s.original, "https://www.example.com/greenleaf/");
        assert_eq!(s.url.as_str(), s.original);
    }
}
