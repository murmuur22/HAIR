//! Owned held-byte native slot, used only by the existing object worker.
//! Logical reservations, not a native heap/work/sandbox guarantee.
use hair::{Result, assets::Texture};
use std::sync::atomic::{AtomicBool, Ordering};

// No retained native root. If an owner disappears before tracked reclamation,
// preserve the logical reservation by refusing later worker admission for this
// process, rather than silently replacing its context/ledger or leaking roots.
static ABANDONED_RESERVATION: AtomicBool = AtomicBool::new(false);
pub(super) fn abandoned() -> bool {
    ABANDONED_RESERVATION.load(Ordering::Acquire)
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::{ffi::c_void, ptr::NonNull};

    #[repr(C)]
    #[derive(Default)]
    struct Pixels {
        pixels: *mut u8,
        length: usize,
        width: u32,
        height: u32,
        code: i32,
    }
    unsafe extern "C" {
        fn hair_still_slot_new() -> *mut c_void;
        fn hair_still_slot_available(slot: *mut c_void) -> i32;
        fn hair_still_slot_free(slot: *mut c_void);
        fn hair_still_decode(slot: *mut c_void, bytes: *const u8, len: usize, out: *mut Pixels);
        fn hair_still_pixels_free(pixels: *mut c_void);
    }
    pub(super) struct Handle(NonNull<c_void>);
    // All native slot access, including destruction, is serialized by the
    // outer Mutex. Framework callbacks access independently owned ObjC state,
    // not this handle or any Rust address.
    unsafe impl Send for Handle {}
    impl Handle {
        pub(super) fn new() -> Result<Self> {
            // SAFETY: creates one owned opaque ARC slot, no borrowed arguments.
            NonNull::new(unsafe { hair_still_slot_new() })
                .map(Self)
                .ok_or_else(|| "MP4 still native slot allocation refused".into())
        }
        pub(super) fn available(&self) -> bool {
            // SAFETY: owned handle; caller holds the sole access mutex.
            unsafe { hair_still_slot_available(self.0.as_ptr()) != 0 }
        }
        pub(super) fn decode(&mut self, bytes: &[u8]) -> Result<Texture> {
            let mut output = Pixels::default();
            // SAFETY: input/output live for the call only. Native copies input
            // before async registration; callbacks never retain either pointer.
            unsafe {
                hair_still_decode(self.0.as_ptr(), bytes.as_ptr(), bytes.len(), &mut output);
            }
            struct OwnedPixels(*mut u8);
            impl Drop for OwnedPixels {
                fn drop(&mut self) {
                    // SAFETY: pointer is independently malloc-owned by this
                    // one result, including every refusal/validation path.
                    unsafe { hair_still_pixels_free(self.0.cast()) };
                }
            }
            let pixels = OwnedPixels(output.pixels);
            if output.code != 0 {
                let reason = match output.code {
                    1 => "tracked native slot still occupied",
                    2 => "not a bounded MP4 ftyp source",
                    3 => "native owned-source allocation refused",
                    4 => "native reference/loader policy readback refused",
                    5 => "asset/track load refused or observation window elapsed",
                    6 => {
                        "only one H264 video track, identity transform, <=160x96 and <=2.02 seconds is supported"
                    }
                    7 => "image generator initialization refused",
                    8 => "first-frame observation window elapsed; cancellation is not a drain",
                    9 => "native first-frame image/error/time refused",
                    10 => "finite opaque sRGB RGBA raster refused",
                    11 => "held loader request/delivery budget refused",
                    _ => "unknown native refusal",
                };
                return Err(format!("MP4 still: {reason}; raw bytes retained"));
            }
            let expected = (output.width as usize)
                .checked_mul(output.height as usize)
                .and_then(|n| n.checked_mul(4));
            if pixels.0.is_null()
                || output.width == 0
                || output.width > 160
                || output.height == 0
                || output.height > 96
                || expected != Some(output.length)
            {
                return Err("MP4 still invalid owned RGBA boundary result".into());
            }
            // SAFETY: successful ABI result owns exactly length initialized
            // bytes. Copy into ordinary Texture, then free the native result.
            let rgba = unsafe { std::slice::from_raw_parts(pixels.0, output.length) }.to_vec();
            Ok(Texture {
                width: output.width,
                height: output.height,
                rgba,
            })
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.available() {
                ABANDONED_RESERVATION.store(true, Ordering::Release);
            }
            // SAFETY: transfer exactly this owner's opaque root. Native contexts,
            // loaders and backing deallocators independently own their ledger.
            unsafe { hair_still_slot_free(self.0.as_ptr()) };
        }
    }
}

// Lazily constructed and reused, never one replacement slot per thumbnail.
#[derive(Default)]
pub(super) struct Slot {
    #[cfg(target_os = "macos")]
    handle: Option<native::Handle>,
}
impl Slot {
    pub(super) fn occupied(&self) -> bool {
        if abandoned() {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            self.handle.as_ref().is_some_and(|h| !h.available())
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    pub(super) fn decode(&mut self, bytes: &[u8]) -> Result<Texture> {
        #[cfg(target_os = "macos")]
        {
            if self.occupied() {
                return Err("MP4 still tracked native reservation occupied; no replacement".into());
            }
            if self.handle.is_none() {
                self.handle = Some(native::Handle::new()?);
            }
            self.handle.as_mut().unwrap().decode(bytes)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = bytes;
            Err("MP4 still decoding requires macOS; raw bytes retained".into())
        }
    }
}
