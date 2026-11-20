//! Visited first-window snapshots only; never World, Theme, materials or scene vertices.
use hair::web::Page;
use std::{collections::VecDeque, mem::size_of, sync::Arc};

pub(crate) const MAX_VISITED_PAGES: usize = 4;
pub(crate) const MAX_VISITED_BYTES: usize = 128 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Accounting {
    pub entries: usize,
    pub page_bytes: usize,
    pub source_bytes: usize,
    pub logical_bytes: usize,
}
struct Entry {
    page: Arc<Page>,
    page_bytes: usize,
    source_bytes: usize,
}
#[derive(Default)]
pub(crate) struct VisitedCache {
    entries: VecDeque<Entry>,
}
impl VisitedCache {
    pub fn accounting(&self) -> Accounting {
        let mut a = Accounting::default();
        for e in &self.entries {
            a.entries += 1;
            a.page_bytes += e.page_bytes;
            a.source_bytes += e.source_bytes;
        }
        a.logical_bytes = a.page_bytes + a.source_bytes;
        a
    }
    /// Exact original URL, with no new parsing/normalization. Touch on requested Follow only.
    pub fn get(&mut self, original: &str) -> Option<Arc<Page>> {
        let i = self
            .entries
            .iter()
            .position(|e| e.page.address.original == original)?;
        let e = self.entries.remove(i)?;
        let page = Arc::clone(&e.page);
        self.entries.push_back(e);
        Some(page)
    }
    /// Call only after a successful active-page transaction. A successful replacement
    /// that cannot fit drops the old same-key version, never the active Page/history.
    pub fn admit(&mut self, page: &Page) -> bool {
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.page.address.original == page.address.original)
        {
            // Retain the original Arc<Page> on a cache-hit arrival; no duplicate projection.
            if page
                .source
                .as_ref()
                .zip(self.entries[i].page.source.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
                && page_charge(page) == Some(self.entries[i].page_bytes)
            {
                return true;
            }
            self.entries.remove(i);
        }
        let Some(source) = &page.source else {
            return false;
        };
        if source.address() != &page.address
            || page.window.as_ref().is_none_or(|w| w.index() != 0)
            || page.next_cursor().is_err()
        {
            return false;
        }
        let source_bytes = source.retained_charge();
        let Some(estimated) = page_charge(page).and_then(|n| n.checked_add(source_bytes)) else {
            return false;
        };
        if estimated > MAX_VISITED_BYTES {
            return false;
        }
        let saved = Arc::new(page.clone());
        let Some(page_bytes) = page_charge(&saved) else {
            return false;
        };
        let Some(total) = page_bytes.checked_add(source_bytes) else {
            return false;
        };
        if total > MAX_VISITED_BYTES {
            return false;
        }
        while self.entries.len() >= MAX_VISITED_PAGES
            || self.accounting().logical_bytes > MAX_VISITED_BYTES - total
        {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            page: saved,
            page_bytes,
            source_bytes,
        });
        true
    }
}
/// Logical admission estimate, NOT RSS/allocator enforcement. Charge owned capacities
/// for projection vectors/strings, fixed Arc/entry metadata, and URL serialization +
/// 256 bytes for URL internals. Source reservation is charged separately once per
/// entry: exact source/address binding and one entry per URL preclude shared-source
/// duplication within this cache. Current/history/worker aliases add no source copy;
/// this charge overlaps (does not expand or replace) the global source ledger.
fn page_charge(p: &Page) -> Option<usize> {
    use hair::content::Destination;
    // Charge a full four-entry queue allowance for each retained Page, even at
    // low occupancy; this conservatively covers VecDeque spare allocation too.
    let fixed = size_of::<Entry>() * MAX_VISITED_PAGES + size_of::<VisitedCache>();
    let mut bytes = fixed.checked_add(size_of::<Page>() + 2 * size_of::<usize>() + 256)?;
    let mut add = |n: usize| -> Option<()> {
        bytes = bytes.checked_add(n)?;
        Some(())
    };
    add(p.address.original.capacity())?;
    add(p.address.url.as_str().len())?;
    add(p.title.capacity())?;
    add(p.text.capacity())?;
    add(p
        .sections
        .capacity()
        .checked_mul(size_of::<hair::web::Section>())?)?;
    for section in &p.sections {
        add(section.title.capacity())?;
        add(section.text.capacity())?;
    }
    for list in [&p.headings, &p.warnings] {
        add(list.capacity().checked_mul(size_of::<String>())?)?;
        for s in list {
            add(s.capacity())?;
        }
    }
    for list in [&p.links, &p.images] {
        add(list
            .capacity()
            .checked_mul(size_of::<hair::content::Link>())?)?;
        for link in list {
            add(link.label.capacity())?;
            add(link.raw.capacity())?;
            let (Destination::Page(s)
            | Destination::Image(s)
            | Destination::External(s)
            | Destination::Unsupported(s)
            | Destination::Invalid(s)) = &link.destination;
            add(s.capacity())?;
        }
    }
    Some(bytes)
}
