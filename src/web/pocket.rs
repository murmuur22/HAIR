//! Content-derived reading collections. Geometry and identities are appearance-independent.
use super::{CodeSpan, Page, Pocket, PocketDocument};
use crate::host_scene::{Anchor, HostScene};
use sha2::{Digest, Sha256};

/// Code ranges valid in `text`, moved to where `text` starts in a document.
/// Out-of-range or non-boundary spans are dropped: styling never invents text.
pub(super) fn shifted(code: &[CodeSpan], text: &str, offset: usize) -> Vec<CodeSpan> {
    code.iter()
        .filter(|c| text.get(c.range.clone()).is_some_and(|s| !s.is_empty()))
        .take(super::MAX_CODE_SPANS)
        .map(|c| CodeSpan {
            range: c.range.start + offset..c.range.end + offset,
            ..c.clone()
        })
        .collect()
}
pub(super) fn populate(page: &Page, host: &mut HostScene, disclosure: Option<&str>) -> Pocket {
    let mut pocket = Pocket {
        source: page.source.clone(),
        ..Pocket::default()
    };
    if page.sections.len() > 1 {
        // Two reading areas connected by a real 2.8 m opening. This is one
        // continuous pocket: no portal transaction, respawn or page change.
        // The threshold is deliberately coarse so adding further sections does
        // not reshuffle architecture. Plain pages retain their simple room.
        for z in [-2.575, 2.575] {
            host.geometry.push(crate::host_scene::Geometry {
                at: [-1.5, 0.0, z],
                size: [0.2, host.height, 2.35],
            });
        }
        host.name = "Generated section pocket (connected reading areas)".into();
    }
    // Prefer a separate object for each section; after eight objects, retain
    // overflow in explicitly labelled collections rather than dropping text.
    let mut sections: Vec<_> = page.sections.iter().enumerate().collect();
    sections.sort_by(|(ai, a), (bi, b)| a.title.cmp(&b.title).then(ai.cmp(bi)));
    for (index, section) in sections {
        let preferred = usize::from(Sha256::digest(section.title.as_bytes())[0] % 8);
        let slot = (0..8)
            .map(|i| (preferred + i) % 8)
            .find(|i| !pocket.documents.contains_key(&format!("web-reading-{i}")))
            .unwrap_or(preferred);
        let anchor = format!("web-reading-{slot}");
        let entry = pocket.documents.entry(anchor).and_modify(|d| d.title = "Multiple sections".into()).or_insert_with(|| PocketDocument {
            title: section.title.clone(),
            text: if let Some(disclosure) = disclosure {
                format!("{}\n{}\n\n{}\nReading collection (sections from this window). Full-page object retains all links for this window only.\n", page.title, page.address.original, disclosure)
            } else {
                format!("{}\n{}\n\nReading collection (sections from this page). Full-page object retains all page links.\n", page.title, page.address.original)
            },
            code: Vec::new(),
        });
        entry.text.push('\n');
        if let Some(code) = page.section_code.get(index) {
            let shifted = shifted(code, &section.text, entry.text.len());
            entry.code.extend(shifted);
        }
        entry.text.push_str(&section.text);
        entry.text.push('\n');
    }
    for anchor in pocket.documents.keys() {
        let slot: usize = anchor
            .strip_prefix("web-reading-")
            .unwrap()
            .parse()
            .unwrap();
        let x = [-6.0, -3.0, 3.0, 6.0][slot % 4];
        let z = if slot < 4 { -2.8 } else { 2.0 };
        host.anchors.push(Anchor {
            id: anchor.clone(),
            role: "document".into(),
            at: [x, 0.1, z],
            size: [0.8, 0.9, 0.6],
            binding: "explicit-page".into(),
        });
    }
    pocket
}
