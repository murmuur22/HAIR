//! System panels are only reachable from the interactive winit main-thread path.
use hair::{Result, workshop::DialogKind};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel, NSSavePanel};
use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSURL};
use std::{
    ffi::{CStr, OsStr},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

struct Policy {
    title: &'static str,
    prompt: &'static str,
    name: &'static str,
    message: &'static str,
    extension: Option<&'static str>,
}
fn policy(kind: DialogKind) -> Policy {
    match kind {
        DialogKind::WallTexture | DialogKind::FloorTexture => Policy {
            title: if kind == DialogKind::WallTexture {
                "Import wall PNG"
            } else {
                "Import floor PNG"
            },
            prompt: "Import PNG",
            name: "",
            extension: Some("png"),
            message: "Choose a local static PNG, at most 1024 × 1024. Original bytes are retained. Texture alpha is composited over opaque white; walls never become invisible.",
        },
        DialogKind::Import => Policy {
            title: "Import GLB model",
            prompt: "Import GLB",
            name: "",
            message: "Choose a supported .glb model. Original bytes are never changed.",
            extension: Some("glb"),
        },
        DialogKind::Open => Policy {
            title: "Open editable HAIR project",
            prompt: "Open project",
            name: "project.toml",
            message: "Choose project.toml inside an editable v2, v3 or v4 project folder. Legacy v1 is refused without changes.",
            extension: Some("toml"),
        },
        DialogKind::SaveNew => Policy {
            title: "Save NEW editable project folder",
            prompt: "Choose new folder",
            name: "HAIR Project",
            message: "In Save As, enter a NEW folder name such as HAIR Project. Choose the existing parent (for example Downloads), not Downloads itself as the output. Do not create the target first. Workshop writes project.toml + model.glb. Existing targets are refused, never replaced.",
            extension: None,
        },
        DialogKind::Export => Policy {
            title: "Export NEW visitor appearance pack folder",
            prompt: "Choose export folder",
            name: "HAIR Appearance Pack",
            message: "In Save As, enter a separate NEW folder name such as HAIR Appearance Pack under an existing parent. Do not create the target first. Export does not save edits: save a NEW editable project separately. Existing targets are refused, never replaced.",
            extension: None,
        },
    }
}

/// Only called after an interactive frame, never from offscreen proof.
/// runModal runs AppKit's own modal event loop; Workshop's exclusive mutable borrow
/// and claimed request prevent duplicate/reentrant actions. No callbacks retain Rust
/// state. All retained panels/strings/URLs are dropped on this thread before the
/// autorelease pool drains; only an owned PathBuf crosses the boundary.
pub fn present(kind: DialogKind) -> Result<Option<PathBuf>> {
    let mtm = MainThreadMarker::new().ok_or("Native panels require the main thread")?;
    autoreleasepool(|_| {
        let p = policy(kind);
        // SAFETY: checked main-thread marker; retained AppKit objects remain local.
        // Panels only select a URL. We never call a file-save/replace API, nor
        // create directories here. The existing exclusive writer is authoritative.
        unsafe {
            if let Some(extension) = p.extension {
                let panel = NSOpenPanel::openPanel(mtm);
                panel.setCanChooseFiles(true);
                panel.setCanChooseDirectories(false);
                panel.setAllowsMultipleSelection(false);
                panel.setResolvesAliases(false);
                configure(&panel, &p);
                let types = NSArray::from_vec(vec![NSString::from_str(extension)]);
                // Compatible cached bindings expose extension filters. This API is
                // deprecated in favor of UTType, not unsafe; core validation still runs.
                #[allow(deprecated)]
                panel.setAllowedFileTypes(Some(&types));
                panel.setAllowsOtherFileTypes(false);
                outcome(&panel, panel.runModal())
            } else {
                let panel = NSSavePanel::savePanel(mtm);
                configure(&panel, &p);
                panel.setAllowsOtherFileTypes(true);
                panel.setShowsTagField(false);
                panel.setExtensionHidden(false);
                outcome(&panel, panel.runModal())
            }
        }
    })
}
unsafe fn configure(panel: &NSSavePanel, p: &Policy) {
    // SAFETY: caller owns this main-thread panel, strings are retained for each call.
    unsafe {
        panel.setTitle(Some(&NSString::from_str(p.title)));
        panel.setPrompt(Some(&NSString::from_str(p.prompt)));
        panel.setMessage(Some(&NSString::from_str(p.message)));
        // Open panels select existing files; only Save/Export name a NEW folder.
        if p.extension.is_none() {
            panel.setNameFieldLabel(Some(&NSString::from_str("New folder name (Save As):")));
        }
        panel.setNameFieldStringValue(&NSString::from_str(p.name));
        panel.setCanCreateDirectories(false);
        panel.setTreatsFilePackagesAsDirectories(false);
    }
}
unsafe fn outcome(panel: &NSSavePanel, response: isize) -> Result<Option<PathBuf>> {
    if response == NSModalResponseCancel {
        return Ok(None);
    }
    if response != NSModalResponseOK {
        return Err(format!("Unexpected system panel response: {response}"));
    }
    // SAFETY: live retained panel and URL; filesystem C string copied while URL lives.
    unsafe {
        let url: objc2::rc::Retained<NSURL> =
            panel.URL().ok_or("System panel returned no file URL")?;
        if !url.isFileURL() {
            return Err("System panel returned a non-file URL".into());
        }
        let bytes = CStr::from_ptr(url.fileSystemRepresentation().as_ptr()).to_bytes();
        Ok(Some(PathBuf::from(OsStr::from_bytes(bytes))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hair::workshop::DialogKind;
    #[test]
    fn native_presenter_refuses_worker_thread_before_creating_panel() {
        let result = std::thread::spawn(|| present(DialogKind::Import))
            .join()
            .unwrap();
        assert!(result.unwrap_err().contains("main thread"));
    }
    #[test]
    fn native_policy_distinguishes_files_from_new_folders() {
        assert_eq!(policy(DialogKind::Import).extension, Some("glb"));
        assert_eq!(policy(DialogKind::Open).extension, Some("toml"));
        assert_eq!(policy(DialogKind::Open).name, "project.toml");
        assert_eq!(policy(DialogKind::SaveNew).name, "HAIR Project");
        assert_eq!(policy(DialogKind::Export).name, "HAIR Appearance Pack");
        for kind in [DialogKind::SaveNew, DialogKind::Export] {
            let p = policy(kind);
            assert!(p.extension.is_none());
            assert!(p.message.contains("existing parent"));
            assert!(p.message.contains("NEW folder"));
            assert!(
                p.message.contains("Save As"),
                "explain the native filename field, not folder selection: {}",
                p.message
            );
        }
        assert!(
            policy(DialogKind::Export)
                .message
                .contains("Export does not save edits")
        );
    }
}
