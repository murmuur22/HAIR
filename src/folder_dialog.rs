//! Visitor-only directory picker. No Workshop/egui dependency or writer policy.
use hair::Result;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
pub fn present() -> Result<Option<PathBuf>> {
    present_with_purpose(false)
}
#[cfg(target_os = "macos")]
pub fn present_storage() -> Result<Option<PathBuf>> {
    present_with_purpose(true)
}
#[cfg(target_os = "macos")]
fn present_with_purpose(storage: bool) -> Result<Option<PathBuf>> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{MainThreadMarker, NSString};
    use std::{
        ffi::{CStr, OsStr},
        os::unix::ffi::OsStrExt,
    };
    let mtm = MainThreadMarker::new().ok_or("Folder chooser requires the main thread")?;
    autoreleasepool(|_| {
        // SAFETY: main thread verified, retained objects and their URLs never leave
        // this pool/thread. Only owned original filesystem bytes cross completion.
        unsafe {
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(false);
            panel.setCanChooseDirectories(true);
            panel.setAllowsMultipleSelection(false);
            panel.setCanCreateDirectories(false);
            panel.setResolvesAliases(false);
            panel.setTreatsFilePackagesAsDirectories(false);
            panel.setTitle(Some(&NSString::from_str(
                if storage { "Choose existing private Documents cache" } else { "Choose starting folder (read-only)" },
            )));
            panel.setPrompt(Some(&NSString::from_str(if storage { "Adopt cache" } else { "Explore folder" })));
            panel.setMessage(Some(&NSString::from_str(if storage { "Existing owned private (0700) empty or compatible versioned cache only. Adopts location; no move/copy/merge/delete of previous cache or originals." } else { "Immediate entries only. U may go to Parent beyond this start. No files are changed." })));
            let response = panel.runModal();
            if response == NSModalResponseCancel {
                return Ok(None);
            }
            if response != NSModalResponseOK {
                return Err(format!("Unexpected folder panel response: {response}"));
            }
            let urls = panel.URLs();
            if urls.len() != 1 {
                return Err("Choose exactly one directory".into());
            }
            let url = panel.URL().ok_or("Folder panel returned no URL")?;
            if !url.isFileURL() {
                return Err("Folder panel returned a non-file URL".into());
            }
            let bytes = CStr::from_ptr(url.fileSystemRepresentation().as_ptr()).to_bytes();
            Ok(Some(PathBuf::from(OsStr::from_bytes(bytes))))
        }
    })
}
#[cfg(not(target_os = "macos"))]
pub fn present() -> Result<Option<PathBuf>> {
    Err("Native folder chooser requires macOS; use --folder PATH on this platform".into())
}
#[cfg(not(target_os = "macos"))]
pub fn present_storage() -> Result<Option<PathBuf>> {
    Err("Native cache directory chooser requires macOS".into())
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn worker_is_refused_before_panel_creation() {
        assert!(
            std::thread::spawn(super::present)
                .join()
                .unwrap()
                .unwrap_err()
                .contains("main thread")
        );
    }
}
