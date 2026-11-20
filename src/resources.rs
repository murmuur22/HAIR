//! Explicit development override or standard executable-relative macOS bundle resources.
use crate::{Graph, Pack, Result, safe_path};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};

// Darwin <sys/fcntl.h>; openat is available since macOS 10.10. No new crate.
// This private macOS diagnostic pins one validated directory, not a general sandbox.
const O_NOFOLLOW: i32 = 0x100;
const O_DIRECTORY: i32 = 0x0010_0000;
const O_CLOEXEC: i32 = 0x0100_0000;
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn openat(fd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
}

pub struct SmokeOutput {
    path: PathBuf,
    directory: File,
}
impl SmokeOutput {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn create_capture(&self, name: &str) -> Result<File> {
        if !["office.png", "reader-welcome.png", "reader-workshop.png"].contains(&name) {
            return Err(format!(
                "unexpected smoke capture {name:?}; use only the three fixed capture names"
            ));
        }
        let name_c = CString::new(name).map_err(|e| e.to_string())?;
        // O_WRONLY | O_CREAT | O_EXCL: existence rejection and creation are ONE
        // operation relative to the retained directory, not a path re-open.
        // SAFETY: live owned directory fd and NUL-terminated allowlisted leaf;
        // mode is supplied as promoted C int; a successful fd is owned exactly once.
        #[cfg(target_os = "macos")]
        let fd = unsafe {
            openat(
                self.directory.as_raw_fd(),
                name_c.as_ptr(),
                0x1 | 0x200 | 0x800 | O_NOFOLLOW | O_CLOEXEC,
                0o600_i32,
            )
        };
        #[cfg(not(target_os = "macos"))]
        return Err("exclusive diagnostic captures currently require macOS".into());
        #[cfg(target_os = "macos")]
        {
            if fd < 0 {
                return Err(format!(
                    "smoke output {name}: {}; refusing overwrite; use a fresh output directory",
                    std::io::Error::last_os_error()
                ));
            }
            // SAFETY: openat returned a new owned fd above, not borrowed elsewhere.
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
}

pub fn resolve_root(executable: &Path, override_root: Option<&Path>) -> Result<PathBuf> {
    let selected = if let Some(root) = override_root {
        if !root.is_absolute() {
            return Err(
                "HAIR_RESOURCE_ROOT must be an absolute directory; no fallback is attempted".into(),
            );
        }
        root.to_path_buf()
    } else {
        let macos = executable
            .parent()
            .ok_or("cannot locate executable directory")?;
        let contents = macos.parent().ok_or("cannot locate bundle Contents")?;
        if macos.file_name().is_none_or(|s| s != "MacOS")
            || contents.file_name().is_none_or(|s| s != "Contents")
        {
            return Err("not a macOS bundle executable; set HAIR_RESOURCE_ROOT to the development resource directory (no implicit development fallback)".into());
        }
        contents.join("Resources")
    };
    let root = selected
        .canonicalize()
        .map_err(|e| resource_error(&selected, e))?;
    if !root.is_dir() {
        return Err(resource_error(&selected, "expected a resource directory"));
    }
    Ok(root)
}
pub fn resource_error(root: &Path, error: impl std::fmt::Display) -> String {
    format!(
        "resources at {}: {error}; restore the complete private bundle or set HAIR_RESOURCE_ROOT to a complete resource directory; no fallback attempted",
        root.display()
    )
}
pub fn paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for name in [
        "content/graph.toml",
        "scene.toml",
        "assets/kenney/pack.toml",
    ] {
        paths.push(safe_path(root, name).map_err(|e| format!("{name}: {e}"))?);
    }
    let graph_bytes = crate::content::read_local(root, "content/graph.toml", 32768)?;
    let graph: Graph = toml::from_str(
        std::str::from_utf8(&graph_bytes).map_err(|e| format!("content/graph.toml: {e}"))?,
    )
    .map_err(|e| format!("content/graph.toml: {e}"))?;
    // Match World::parse's bound before following the declared page closure.
    if graph.version != 1 || graph.pages.is_empty() || graph.pages.len() > 16 {
        return Err("content/graph.toml: unsupported graph version or page count".into());
    }
    let pack_bytes = crate::content::read_local(root, "assets/kenney/pack.toml", 32768)?;
    let pack = Pack::parse(
        std::str::from_utf8(&pack_bytes).map_err(|e| format!("assets/kenney/pack.toml: {e}"))?,
    )
    .map_err(|e| format!("assets/kenney/pack.toml: {e}"))?;
    let content = safe_path(root, "content")?;
    let assets = safe_path(root, "assets/kenney")?;
    for page in graph.pages {
        paths.push(
            safe_path(&content, &page.file).map_err(|e| format!("content/{}: {e}", page.file))?,
        );
    }
    // Startup/report only: retain the declared legacy dependency contract even
    // though v2 appearance uses a separate cache. Decode one bounded model at a
    // time, using the real loader and the legacy scene's extent/footprint rules.
    for asset in pack.assets {
        let path = safe_path(&assets, &asset.file)
            .map_err(|e| format!("assets/kenney/{}: {e}", asset.file))?;
        let model = (|| {
            let bytes = crate::content::read_local(&assets, &asset.file, 8_000_000)?;
            crate::assets::decode(&bytes, asset.max_extent)
        })()
        .map_err(|e| format!("assets/kenney/{}: {e}", asset.file))?;
        if (model.max[0] - model.min[0]) * 2.0 > asset.footprint[0] as f32 * 0.5 + 0.01
            || (model.max[2] - model.min[2]) * 2.0 > asset.footprint[1] as f32 * 0.5 + 0.01
        {
            return Err(format!(
                "assets/kenney/{}: role {} exceeds grid footprint",
                asset.file, asset.role
            ));
        }
        paths.push(path);
    }
    Ok(paths)
}

pub fn smoke_output(root: &Path, output: &Path) -> Result<SmokeOutput> {
    if !output.is_absolute() {
        return Err("smoke output must be an absolute existing directory outside Resources".into());
    }
    let output = output.canonicalize().map_err(|e| {
        format!(
            "smoke output {}: {e}; create a separate directory first",
            output.display()
        )
    })?;
    if !output.is_dir() || output.starts_with(root) || root.starts_with(&output) {
        return Err(
            "smoke output must be a separate directory, not Resources, its child or ancestor"
                .into(),
        );
    }
    for name in ["office.png", "reader-welcome.png", "reader-workshop.png"] {
        match std::fs::symlink_metadata(output.join(name)) {
            Ok(_) => {
                return Err(format!(
                    "smoke output {name} already exists; use a fresh output directory"
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("smoke output {name}: {e}")),
        }
    }
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        .open(&output)
        .map_err(|e| format!("smoke output directory: {e}; use a fresh separate directory"))?;
    let held = directory.metadata().map_err(|e| e.to_string())?;
    let current = output
        .canonicalize()
        .map_err(|e| format!("smoke output directory changed: {e}"))?;
    let named = std::fs::metadata(&current).map_err(|e| e.to_string())?;
    if !held.is_dir()
        || held.dev() != named.dev()
        || held.ino() != named.ino()
        || current.starts_with(root)
        || root.starts_with(&current)
    {
        return Err(
            "smoke output directory changed during validation; use a fresh separate directory"
                .into(),
        );
    }
    // Path checks above give early guidance only. The actual write uses openat
    // O_EXCL on this held directory and never reopens a capture by its full path.
    Ok(SmokeOutput {
        path: output,
        directory,
    })
}
