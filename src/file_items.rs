//! Shared file-item vocabulary. Embedded art is inert and never file authority.
use crate::{
    Result,
    assets::{Model, Texture, Vertex},
};
use std::{
    ffi::OsStr,
    path::Path,
    sync::{Arc, OnceLock},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Document,
    Video,
    Audio,
    Code,
    Image,
    Model,
    Other,
}
impl Class {
    pub fn of(name: &OsStr) -> Self {
        let ext = Path::new(name)
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "txt" | "md" | "markdown" | "pdf" | "doc" | "docx" | "odt" | "rtf" => Self::Document,
            "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" => Self::Video,
            "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "aiff" => Self::Audio,
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "json" | "toml" | "yaml" | "yml"
            | "html" | "css" | "c" | "h" | "cpp" | "java" | "swift" | "sh" | "wgsl" => Self::Code,
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "heic" | "bmp" | "tiff" => Self::Image,
            "glb" | "gltf" | "obj" | "fbx" | "stl" => Self::Model,
            _ => Self::Other,
        }
    }
}
pub struct Preview {
    pub model: Arc<Model>,
    pub warning: Option<String>,
    pub content_derived: bool,
}
pub struct Catalog {
    models: [Arc<Model>; 5],
}
impl Catalog {
    pub fn builtin() -> Result<&'static Self> {
        static C: OnceLock<Result<Catalog>> = OnceLock::new();
        C.get_or_init(|| {
            let sources: [&[u8]; 5] = [
                include_bytes!("../packs/file-items/document-paper.glb"),
                include_bytes!("../packs/file-items/video-reel.glb"),
                include_bytes!("../packs/file-items/audio-cassette.glb"),
                include_bytes!("../packs/file-items/code-tablet.glb"),
                include_bytes!("../packs/file-items/image-plane.glb"),
            ];
            let decoded = sources
                .iter()
                .map(|s| crate::assets::decode(s, 3.0).map(Arc::new))
                .collect::<Result<Vec<_>>>()?;
            if decoded.iter().any(|m| m.vertices.len() > 32766) {
                return Err("File-item mesh budget".into());
            }
            let catalog = Catalog {
                models: decoded.try_into().map_err(|_| "File-item count")?,
            };
            if catalog.retained_bytes() > 8 * 1024 * 1024 {
                return Err("File-item aggregate budget".into());
            }
            Ok(catalog)
        })
        .as_ref()
        .map_err(Clone::clone)
    }
    pub fn representation(&self, name: &OsStr) -> Arc<Model> {
        self.models[match Class::of(name) {
            Class::Video => 1,
            Class::Audio => 2,
            Class::Code => 3,
            Class::Image => 4,
            _ => 0,
        }]
        .clone()
    }
    pub fn models(&self) -> &[Arc<Model>; 5] {
        &self.models
    }
    pub fn retained_bytes(&self) -> usize {
        self.models
            .iter()
            .map(|m| {
                m.vertices.len() * std::mem::size_of::<Vertex>()
                    + m.textures.iter().map(|t| t.rgba.len()).sum::<usize>()
            })
            .sum()
    }
    /// Owned bytes only; failures keep explicit representative art, never execute.
    pub fn owned(&self, name: &OsStr, bytes: &[u8]) -> Preview {
        let result = match Class::of(name) {
            Class::Image => Some(
                crate::assets::decode_local_image(bytes).and_then(|t| image_plane(Arc::new(t))),
            ),
            Class::Model => Some(crate::assets::decode_local_preview_model(bytes)),
            _ => None,
        };
        match result {
            Some(Ok(model)) if model.vertices.len() <= 32766 => Preview {
                model: Arc::new(model),
                warning: None,
                content_derived: true,
            },
            Some(Ok(_)) => Preview {
                model: self.representation(name),
                warning: Some("Content mesh exceeds item budget; representative only".into()),
                content_derived: false,
            },
            Some(Err(e)) => Preview {
                model: self.representation(name),
                warning: Some(format!("Content preview unavailable: {e}")),
                content_derived: false,
            },
            None => Preview {
                model: self.representation(name),
                warning: None,
                content_derived: false,
            },
        }
    }
}
/// Fit only the file item above its stationary host base; no host mutation.
pub fn above_base(shape: &[Vertex], base: &crate::host_scene::Anchor) -> Vec<Vertex> {
    let Ok(mut shape) = crate::placement::grounded(shape, [0.0, 0.0], 0.0, [0.8, 0.85, 0.28])
    else {
        return vec![];
    };
    for v in &mut shape {
        v.position[0] += base.at[0];
        v.position[1] += base.at[1] + base.size[1] + 0.12;
        v.position[2] += base.at[2];
    }
    shape
}
pub fn image_plane(image: Arc<Texture>) -> Result<Model> {
    let length = (image.width as usize)
        .checked_mul(image.height as usize)
        .and_then(|n| n.checked_mul(4));
    if image.width == 0
        || image.height == 0
        || length != Some(image.rgba.len())
        || image.rgba.len() > 4 * 1024 * 1024
    {
        return Err("Image-item pixel budget".into());
    }
    let scale = 1.0 / (image.width.max(image.height) as f32);
    let (w, h) = (image.width as f32 * scale, image.height as f32 * scale);
    let v = |x, y, uv| Vertex {
        position: [x, y, 0.0],
        color: [1.0; 3],
        uv,
        texture: 0x8000_0001,
    };
    let (tl, tr, br, bl) = (
        v(-w * 0.5, h, [0.0, 0.0]),
        v(w * 0.5, h, [1.0, 0.0]),
        v(w * 0.5, 0.0, [1.0, 1.0]),
        v(-w * 0.5, 0.0, [0.0, 1.0]),
    );
    Ok(Model {
        vertices: vec![tl, tr, br, tl, br, bl],
        textures: vec![image],
        min: [-w * 0.5, 0.0, 0.0],
        max: [w * 0.5, h, 0.0],
    })
}
