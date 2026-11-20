//! One bounded physical page. Raw entry bindings are not web identities or paths to reopen.
use crate::{
    Result, World,
    host_scene::{Anchor, HostScene, Wall, WallAperture},
    local_explorer::{Entry, Kind, Snapshot, escape_display},
    navigation::{Navigation, Row},
};
use std::collections::BTreeMap;

pub const PAGE_SIZE: usize = 12;
/// Required portal vocabulary where Next is absent, validated but not rendered.
pub const INERT_PORTAL: &str = "vocabulary-portal";
const NEXT_PORTAL: &str = "folder-next";
const PREVIOUS_PORTAL: &str = "folder-previous";
pub const PARENT_PORTAL: &str = "folder-parent";
pub const PARENT_LABEL: &str = "Parent - filesystem (U)";

// Canonical portal dimensions; side envelopes exchange world X/Z only.
// Mesh/frame orientation is a separate tracer. At the retained pack scale of 2,
// 1.45m width lets office fit above the 1.65m eye; archive needs its full 0.5m
// depth to avoid becoming a waist-high miniature. Keep uniform pack fitting
// and the separate normalized door-trim aperture rule. Width leaves 0.15m
// between 1.6m child slots; depth stays 0.2m beyond the front.
const PORTAL_SIZE: [f32; 3] = [1.45, 2.0, 0.6];

/// Local navigation is not a filesystem entry or a display path to reopen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Continuation {
    Next,
    Previous,
}
impl Continuation {
    pub fn label(self, view: &View) -> String {
        let direction = match self {
            Self::Next => "Next",
            Self::Previous => "Previous",
        };
        match self.page(view) {
            Some(page) => format!("{direction} {}/{} - same folder", page + 1, view.pages()),
            None => format!("{direction} unavailable - same folder"),
        }
    }
    pub fn page(self, view: &View) -> Option<usize> {
        if view.page >= view.pages() {
            return None;
        }
        match self {
            Self::Next => view.page.checked_add(1).filter(|page| *page < view.pages()),
            Self::Previous => view.page.checked_sub(1),
        }
    }
    pub fn reciprocal(self) -> Self {
        match self {
            Self::Next => Self::Previous,
            Self::Previous => Self::Next,
        }
    }
}
/// Existing bitmap renderer is ASCII: preserve non-ASCII scalars as explicit
/// Unicode escapes rather than ambiguous '?' glyphs. Raw identities stay in Entry.
pub fn display(value: &std::ffi::OsStr) -> String {
    let mut text = String::new();
    for c in escape_display(value).chars() {
        if c.is_ascii() {
            text.push(c);
        } else {
            text.extend(c.escape_unicode());
        }
    }
    text
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundEntry {
    pub index: usize,
    pub entry: Entry,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub bindings: BTreeMap<String, BoundEntry>,
    pub continuations: BTreeMap<String, Continuation>,
    /// A filesystem action, never a raw entry or same-folder continuation.
    pub parent: Option<String>,
    pub page: usize,
    pub count: usize,
    pub source: String,
    pub reader: String,
}
impl View {
    pub fn pages(&self) -> usize {
        self.count.div_ceil(PAGE_SIZE).max(1)
    }
    pub fn summary(&self) -> String {
        let first = if self.count == 0 {
            0
        } else {
            self.page * PAGE_SIZE + 1
        };
        format!(
            "Page {}/{} | entries {}-{} of {}{}",
            self.page + 1,
            self.pages(),
            first,
            ((self.page + 1) * PAGE_SIZE).min(self.count),
            self.count,
            if self.count == 0 {
                " | Empty folder"
            } else {
                ""
            }
        )
    }
}
pub fn kind(entry: &Entry) -> &'static str {
    if entry.kind != Kind::File {
        return entry.kind.label();
    }
    match entry
        .path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt" | "md" | "markdown") => {
            "text suffix (literal prefix or fallback; E reads bytes)"
        }
        Some("png") => "PNG suffix (lazy bounded preview or fallback; E checks source)",
        Some("jpg" | "jpeg") => "JPEG suffix (lazy bounded preview or fallback; E checks source)",
        Some("gif") => "image (preview unsupported)",
        Some("mkv") => "MKV video (playback unavailable)",
        Some("mp4") => "MP4 suffix (tiny H264 still or fallback; E reads bytes; no playback)",
        Some("wav") => {
            "WAV suffix (bounded PCM16LE audio or refusal; explicit Space playback; V reads bytes)"
        }
        Some("mp3" | "mov") => "media (playback unsupported)",
        Some("glb") => "GLB suffix (bounded static mesh or fallback; E reads bytes)",
        Some("gltf" | "obj") => "model (loading unsupported)",
        Some("pdf") => "PDF (preview unsupported)",
        _ => "regular file (stored bytes; mode determined per chunk)",
    }
}
pub fn metadata(entry: &Entry) -> String {
    let is_mp4 = entry.kind == Kind::File
        && std::path::Path::new(&entry.name)
            .extension()
            .is_some_and(|suffix| suffix.as_encoded_bytes().eq_ignore_ascii_case(b"mp4"));
    let policy = if is_mp4 {
        "No writes, execution or default-open. Held source acquisition is no-follow."
    } else {
        "No writes, execution, default-open or link following."
    };
    let native_scope = if is_mp4 {
        "MP4 still safety is scoped: no arbitrary-file/external-reference containment, native codec memory/work bound, interruptible cancellation or callback-drain guarantee. Source acquisition remains held/no-follow; E reads raw stored bytes.\n"
    } else {
        ""
    };
    format!(
        "NAME (escaped OS bytes):\n{}\nSOURCE PATH (display only; may become stale):\n{}\nTYPE: {}\n{}\n{}",
        display(&entry.name),
        display(entry.path.as_os_str()),
        kind(entry),
        policy,
        native_scope
    )
}
pub fn build(
    snapshot: &Snapshot,
    requested_page: usize,
    collapsed: bool,
) -> Result<(World, HostScene)> {
    let count = snapshot.entries.len();
    let pages = count.div_ceil(PAGE_SIZE).max(1);
    let page = requested_page.min(pages - 1);
    let has_next = page + 1 < pages;
    // Snapshot.folder is an absolute path obtained from the opened handle. This
    // only hides the root affordance; navigation never reopens this display path.
    // A non-root Parent may still be refused by ordinary OS permissions at use.
    let parent = snapshot.folder.parent().map(|_| PARENT_PORTAL.to_string());
    let source = display(snapshot.folder.as_os_str());
    // Keep the required role vocabulary validated on every page. The terminal
    // portal sentinel is invisible/inert; only a real continuation is labelled.
    let mut host = HostScene {
        version: 2,
        name: "Local folder page".into(),
        // Stable front child slots, rear entry and opposing side continuations.
        // Freestanding doors retain their host-owned basis; the platform has no shell.
        room: [-10.5, -6.0, 10.5, 6.0],
        spawn: [0.0, 5.65],
        height: 3.0,
        anchors: vec![
            Anchor {
                id: "vocabulary-document".into(),
                role: "document".into(),
                at: [9.5, 0.0, 4.0],
                size: [0.6, 0.8, 0.5],
                binding: "explicit-page".into(),
            },
            Anchor {
                id: if has_next { NEXT_PORTAL } else { INERT_PORTAL }.into(),
                role: "portal".into(),
                // An inert terminal sentinel stays outside the open front,
                // rather than cutting an unused hole in a closed side wall.
                at: if has_next {
                    [11.2, 0.0, 0.0]
                } else {
                    [10.8, 0.0, -6.5]
                },
                size: if has_next {
                    [PORTAL_SIZE[2], PORTAL_SIZE[1], PORTAL_SIZE[0]]
                } else {
                    PORTAL_SIZE
                },
                binding: "explicit-door".into(),
            },
            Anchor {
                id: "vocabulary-image".into(),
                role: "image".into(),
                at: [0.0, 0.0, -7.5],
                size: [0.6, 0.8, 0.1],
                binding: "current-image".into(),
            },
        ],
        geometry: Vec::new(),
        overhead: Vec::new(),
        wall_apertures: Vec::new(),
        open_platform: true,
    };
    if has_next {
        host.wall_apertures.push(WallAperture {
            anchor: NEXT_PORTAL.into(),
            wall: Wall::Right,
            width: 1.65,
            height: 2.2,
        });
    }
    if let Some(id) = &parent {
        host.anchors.push(Anchor {
            id: id.clone(),
            role: "portal".into(),
            at: [0.0, 0.0, 6.7],
            size: PORTAL_SIZE,
            binding: "explicit-door".into(),
        });
        host.wall_apertures.push(WallAperture {
            anchor: id.clone(),
            wall: Wall::Rear,
            width: 1.65,
            height: 2.2,
        });
    }
    if page > 0 {
        host.anchors.push(Anchor {
            id: PREVIOUS_PORTAL.into(),
            role: "portal".into(),
            at: [-11.2, 0.0, 0.0],
            size: [PORTAL_SIZE[2], PORTAL_SIZE[1], PORTAL_SIZE[0]],
            binding: "explicit-door".into(),
        });
        host.wall_apertures.push(WallAperture {
            anchor: PREVIOUS_PORTAL.into(),
            wall: Wall::Left,
            width: 1.65,
            height: 2.2,
        });
    }
    let mut bindings = BTreeMap::new();
    for (slot, (index, entry)) in snapshot
        .entries
        .iter()
        .enumerate()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .enumerate()
    {
        let folder = entry.kind == Kind::Folder;
        let anchor = format!("entry-{slot}");
        let at = if folder {
            // Twelve front-facing entry slots at 1.6m pitch, clear of the
            // opposing side doors. Keep capacity and human-scale envelopes.
            [-8.8 + slot as f32 * 1.6, 0.0, -6.5]
        } else {
            [
                -4.5 + (slot % 4) as f32 * 3.0,
                0.0,
                -3.0 + (slot / 4) as f32 * 3.0,
            ]
        };
        host.anchors.push(Anchor {
            id: anchor.clone(),
            role: if folder { "portal" } else { "document" }.into(),
            at,
            size: if folder { PORTAL_SIZE } else { [0.8, 0.8, 0.6] },
            binding: if folder {
                "explicit-door"
            } else {
                "explicit-page"
            }
            .into(),
        });
        bindings.insert(
            anchor,
            BoundEntry {
                index,
                entry: entry.clone(),
            },
        );
    }
    // Same envelope, collision and route checks as other production rooms.
    host = HostScene::parse(&toml::to_string(&host).map_err(|e| e.to_string())?)?;
    // Orientation is local: keep the literal source ID for highlight/inspection,
    // but spend the visible rows on this folder and the contents of this room.
    let mut rows = vec![Row {
        id: source.clone(),
        label: display(
            snapshot
                .folder
                .file_name()
                .unwrap_or_else(|| snapshot.folder.as_os_str()),
        ),
        depth: 0,
    }];
    for entry in snapshot
        .entries
        .iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
    {
        rows.push(Row {
            id: display(entry.path.as_os_str()),
            label: display(&entry.name),
            depth: 1,
        });
    }
    let mut continuations = BTreeMap::new();
    if has_next {
        continuations.insert(NEXT_PORTAL.into(), Continuation::Next);
    }
    if page > 0 {
        continuations.insert(PREVIOUS_PORTAL.into(), Continuation::Previous);
    }
    let view = View {
        bindings,
        continuations,
        parent,
        page,
        count,
        source: source.clone(),
        reader: format!("CURRENT DIRECTORY (display only; may become stale):\n{source}\n"),
    };
    let mut world = World::folder_room(
        source,
        Navigation {
            title: "Folder (visible children only)".into(),
            rows,
        },
    );
    world.tree_collapsed = collapsed;
    world.folder_view = Some(view);
    Ok((world, host))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_suffix_teaches_bounded_preview_not_unavailable() {
        for name in ["source.JPG", "source.jpeg"] {
            let entry = Entry {
                name: name.into(),
                path: name.into(),
                kind: Kind::File,
            };
            assert_eq!(
                kind(&entry),
                "JPEG suffix (lazy bounded preview or fallback; E checks source)"
            );
        }
    }
    #[test]
    fn finite_pages_retain_every_raw_binding_and_empty_has_no_targets() {
        let snapshot = Snapshot {
            folder: "/Fixture".into(),
            entries: (0..77)
                .map(|i| Entry {
                    name: format!("File {i:03}.bin").into(),
                    path: format!("/Fixture/File {i:03}.bin").into(),
                    kind: Kind::File,
                })
                .collect(),
        };
        let mut seen = std::collections::BTreeSet::new();
        for page in 0..7 {
            let (world, host) = build(&snapshot, page, true).unwrap();
            assert!(world.tree_collapsed);
            assert!(!host.blocked(host.spawn));
            let view = world.folder_view.unwrap();
            assert!(view.bindings.len() <= PAGE_SIZE);
            for bound in view.bindings.values() {
                assert_eq!(bound.entry, snapshot.entries[bound.index]);
                seen.insert(bound.index);
            }
        }
        assert_eq!(seen.len(), 77);
        let (world, host) = build(
            &Snapshot {
                folder: "/Empty".into(),
                entries: Vec::new(),
            },
            0,
            false,
        )
        .unwrap();
        assert!(
            world
                .folder_view
                .as_ref()
                .unwrap()
                .summary()
                .contains("Empty")
        );
        assert!(crate::interaction::resolve(&host, &crate::Pose::default(), &world).is_none());
    }
    #[test]
    fn folder_prompt_and_activation_binding_are_the_same_raw_entry() {
        let snapshot = Snapshot {
            folder: "/Fixture".into(),
            entries: vec![Entry {
                name: "Child\nCase".into(),
                path: "/Fixture/Child\nCase".into(),
                kind: Kind::Folder,
            }],
        };
        let (world, host) = build(&snapshot, 0, false).unwrap();
        let a = host.anchors.iter().find(|a| a.id == "entry-0").unwrap();
        let pose = crate::Pose {
            position: [a.at[0], a.at[2] + 1.0],
            yaw: 0.0,
            pitch: ((a.size[1] * 0.5 - 1.65) / 1.0).atan(),
        };
        let target = crate::interaction::resolve(&host, &pose, &world).unwrap();
        assert_eq!(target.anchor, "entry-0");
        assert!(target.prompt.contains("Child\\nCase"));
        assert_eq!(
            target.destination,
            Some(escape_display(snapshot.entries[0].path.as_os_str()))
        );
    }
}
