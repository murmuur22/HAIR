//! Explicit local folder input: bounded inline `[label](path)` / `![alt](image.png)`, not CommonMark.
use crate::Result;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Page(String),
    Image(String),
    External(String),
    Unsupported(String),
    Invalid(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub label: String,
    pub raw: String,
    pub destination: Destination,
}
#[derive(Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Debug)]
pub struct Site {
    pub root: PathBuf,
    pub start: String,
    pub pages: BTreeMap<String, String>,
    pub links: BTreeMap<String, Vec<String>>,
    pub records: BTreeMap<String, Vec<Link>>,
    pub images: BTreeMap<String, Image>,
    pub skipped: Vec<String>,
}
/// Read through held descriptors; no symlink in a selected relative file chain is followed.
/// Darwin-only scope, like the existing capture writer; not a general same-user sandbox.
pub fn read_local(root: &Path, relative: &str, limit: u64) -> Result<Vec<u8>> {
    use std::{
        ffi::CString,
        fs::{File, OpenOptions},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        },
    };
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err("unsafe relative local path".into());
    }
    unsafe extern "C" {
        fn openat(fd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
    }
    let mut held = OpenOptions::new()
        .read(true)
        .custom_flags(0x100 | 0x0010_0000 | 0x0100_0000)
        .open(root)
        .map_err(|e| e.to_string())?;
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let name = CString::new(*part).map_err(|e| e.to_string())?;
        let flags = 0x100 | 0x0100_0000 | 0x4 | if i + 1 < parts.len() { 0x0010_0000 } else { 0 }; // NOFOLLOW, CLOEXEC, NONBLOCK; directories pinned.
        // SAFETY: live held fd and NUL-terminated single component; success transfers ownership.
        let fd = unsafe { openat(held.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(format!(
                "local path {relative}: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: new descriptor returned above, owned once.
        held = unsafe { File::from_raw_fd(fd) };
    }
    let meta = held.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > limit {
        return Err(format!(
            "local file {relative} must be regular and <= {limit} bytes"
        ));
    }
    let mut bytes = Vec::new();
    held.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("local file grew beyond budget".into());
    }
    Ok(bytes)
}
/// Same bounded inline subset for both legacy bound documents and external pages.
pub fn inline_links(text: &str) -> Result<Vec<(String, String)>> {
    if text.len() > 16384 {
        return Err("document exceeds 16 KiB".into());
    }
    let mut rest = text;
    let mut out = Vec::new();
    while let Some(begin) = rest.find('[') {
        rest = &rest[begin + 1..];
        let Some(mid) = rest.find("](") else { break };
        let after = &rest[mid + 2..];
        let Some(end) = after.find(')') else { break };
        if out.len() >= 64 {
            return Err("site links exceed 64 per page".into());
        }
        out.push((rest[..mid].into(), after[..end].into()));
        rest = &after[end + 1..];
    }
    Ok(out)
}
impl Site {
    pub fn load(root: &Path) -> Result<Self> {
        if std::fs::symlink_metadata(root)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("selected site root symlink refused".into());
        }
        let root = root
            .canonicalize()
            .map_err(|e| format!("selected site: {e}"))?;
        let mut names = Vec::new();
        fn walk(
            base: &Path,
            dir: &Path,
            names: &mut Vec<String>,
            depth: usize,
            count: &mut usize,
        ) -> Result<()> {
            if depth > 8 {
                return Err("site directory depth exceeds 8".into());
            }
            for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                *count += 1;
                if *count > 128 {
                    return Err(
                        "site entry budget exceeds 128 (including unsupported inputs)".into(),
                    );
                }
                let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if meta.file_type().is_symlink() {
                    return Err("site symlinks are refused".into());
                }
                if meta.is_dir() {
                    walk(base, &path, names, depth + 1, count)?;
                } else if meta.is_file() {
                    names.push(
                        path.strip_prefix(base)
                            .map_err(|e| e.to_string())?
                            .to_str()
                            .ok_or("site names must be UTF-8")?
                            .into(),
                    );
                } else {
                    return Err("site special files refused".into());
                }
            }
            Ok(())
        }
        walk(&root, &root, &mut names, 0, &mut 0)?;
        names.sort();
        let mut pages = BTreeMap::new();
        let mut images = BTreeMap::new();
        let mut skipped = Vec::new();
        let mut total = 0;
        for id in names {
            if id.ends_with(".md") {
                if pages.len() >= 32 {
                    return Err("site page budget exceeds 32".into());
                }
                let bytes = read_local(&root, &id, 16384)?;
                total += bytes.len();
                pages.insert(id, String::from_utf8(bytes).map_err(|e| e.to_string())?);
            } else if id.ends_with(".png") {
                if images.len() >= 16 {
                    return Err("site image budget exceeds 16".into());
                }
                let bytes = read_local(&root, &id, 262144)?;
                total += bytes.len();
                // Site PNG pixels do not use text or ICC profiles. Disable their
                // decompression BEFORE read_info, not after metadata allocation.
                // This path also validates downloaded-site staging; the separate
                // accepted local GLB/pack/Workshop decoder is unchanged.
                let mut options = png::DecodeOptions::default();
                options.set_ignore_text_chunk(true);
                options.set_ignore_iccp_chunk(true);
                options.set_ignore_adler32(false);
                options.set_skip_ancillary_crc_failures(false);
                let mut decoder =
                    png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
                decoder.set_limits(png::Limits { bytes: 262144 });
                decoder.ignore_checksums(false);
                decoder.set_transformations(png::Transformations::normalize_to_color8());
                // Reject huge dimensions from IHDR before the decoder prepares
                // row/image storage or processes any following metadata.
                let header = decoder
                    .read_header_info()
                    .map_err(|e| format!("PNG {id}: {e}"))?;
                if header.width == 0
                    || header.height == 0
                    || header.width > 256
                    || header.height > 256
                {
                    return Err("PNG profile: static, 1..256 pixels per axis".into());
                }
                let mut reader = decoder.read_info().map_err(|e| format!("PNG {id}: {e}"))?;
                let info = reader.info();
                if info.width == 0
                    || info.height == 0
                    || info.width > 256
                    || info.height > 256
                    || info.animation_control.is_some()
                {
                    return Err("PNG profile: static, 1..256 pixels per axis".into());
                }
                let size = reader.output_buffer_size().ok_or("PNG buffer overflow")?;
                if size > 262144 {
                    return Err("PNG decoded pixel budget exceeds 256 KiB".into());
                }
                let mut buffer = vec![0; size];
                let frame = reader
                    .next_frame(&mut buffer)
                    .map_err(|e| format!("PNG {id}: {e}"))?;
                // Consume and validate IEND / post-IDAT chunks too. Metadata
                // ignore flags and decoder budget remain in force through EOF.
                reader.finish().map_err(|e| format!("PNG {id}: {e}"))?;
                if reader.info().animation_control.is_some() {
                    return Err("PNG profile: static, 1..256 pixels per axis".into());
                }
                let mut rgba = Vec::with_capacity((frame.width * frame.height * 4) as usize);
                for pixel in buffer[..frame.buffer_size()].chunks(frame.color_type.samples()) {
                    let color = match frame.color_type {
                        png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
                        png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
                        png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
                        png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
                        _ => return Err("unsupported PNG output".into()),
                    };
                    rgba.extend(color);
                }
                images.insert(
                    id,
                    Image {
                        width: frame.width,
                        height: frame.height,
                        rgba,
                    },
                );
            } else {
                skipped.push(id);
            }
            if total > 2_097_152 {
                return Err("site aggregate input exceeds 2 MiB".into());
            }
        }
        let start = if pages.contains_key("index.md") {
            "index.md".into()
        } else {
            pages
                .keys()
                .next()
                .ok_or("selected site has no .md pages")?
                .clone()
        };
        let mut records = BTreeMap::new();
        let mut links = BTreeMap::new();
        for (id, text) in &pages {
            let mut page_links = Vec::new();
            let mut targets = Vec::new();
            for (label, raw) in inline_links(text)? {
                let raw = raw.as_str();
                let destination = if raw.starts_with("https://") || raw.starts_with("http://") {
                    Destination::External(raw.into())
                } else if raw.contains(':') {
                    Destination::Unsupported(raw.into())
                } else if raw.is_empty()
                    || raw.len() > 512
                    || raw.contains(['\\', '#', '?', '%'])
                    || raw.starts_with('/')
                    || raw.split('/').any(|p| p == ".." || p == ".")
                {
                    Destination::Invalid("unsafe/unsupported local path syntax".into())
                } else {
                    let relative = Path::new(id)
                        .parent()
                        .unwrap_or(Path::new(""))
                        .join(raw)
                        .to_string_lossy()
                        .to_string();
                    if pages.contains_key(&relative) {
                        Destination::Page(relative)
                    } else if images.contains_key(&relative) {
                        Destination::Image(relative)
                    } else if raw.ends_with('/') {
                        let prefix = format!("{}/", relative.trim_end_matches('/'));
                        let index = format!("{prefix}index.md");
                        if pages.contains_key(&index) {
                            Destination::Page(index)
                        } else if let Some(p) = pages.keys().find(|p| p.starts_with(&prefix)) {
                            Destination::Page(p.clone())
                        } else {
                            Destination::Invalid("folder has no supported page".into())
                        }
                    } else {
                        Destination::Invalid("missing or unsupported local destination".into())
                    }
                };
                if let Destination::Page(p) = &destination
                    && !targets.contains(p)
                {
                    targets.push(p.clone());
                }
                page_links.push(Link {
                    label,
                    raw: raw.into(),
                    destination,
                });
            }
            records.insert(id.clone(), page_links);
            links.insert(id.clone(), targets);
        }
        Ok(Self {
            root,
            start,
            pages,
            links,
            records,
            images,
            skipped,
        })
    }
}
