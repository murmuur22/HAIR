//! Bounded binary v1 manifest. Little endian integers; raw OS name bytes. Fixed
//! magic/schema precede checksum so unsupported versions refuse actionably.
use super::*;
const MAGIC: &[u8;8] = b"HAIRCOLL";
#[derive(Clone)]
pub(super) struct Manifest { pub generation: u64, pub next: u64, pub items: Vec<Item> }
impl Manifest {
    pub fn empty() -> Self { Self { generation: 0, next: 1, items: Vec::new() } }
    pub fn usage(&self) -> u64 { self.items.iter().map(|i| i.bytes).sum() }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&self.generation.to_le_bytes());
        bytes.extend_from_slice(&self.next.to_le_bytes());
        bytes.extend_from_slice(&(self.items.len() as u32).to_le_bytes());
        for item in &self.items {
            valid_name(&item.name)?;
            bytes.extend_from_slice(&item.id.to_le_bytes());
            bytes.extend_from_slice(&item.bytes.to_le_bytes());
            bytes.push(match item.category { Category::Document => 1, Category::Decoration => 2 });
            bytes.extend_from_slice(&(item.name.as_bytes().len() as u16).to_le_bytes());
            bytes.extend_from_slice(&item.sha256);
            bytes.extend_from_slice(item.name.as_bytes());
        }
        let digest = Sha256::digest(&bytes);
        bytes.extend_from_slice(&digest);
        if bytes.len() as u64 > MANIFEST_BYTES { return Err("Collection manifest metadata budget exceeded".into()); }
        // Same validation on both directions; no publishing an unreadable manifest.
        Self::decode(&bytes)?;
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 || &bytes[..8] != MAGIC { return Err("Corrupt collection manifest header; preserve root for inspection".into()); }
        let version = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| "Manifest schema missing")?);
        if version != 1 { return Err(format!("Unsupported collection schema {version}; use a compatible HAIR build or choose a new empty root; existing cache unchanged")); }
        if bytes.len() < 64 || bytes.len() as u64 > MANIFEST_BYTES { return Err("Collection manifest truncated/oversized".into()); }
        let (body, digest) = bytes.split_at(bytes.len() - 32);
        if Sha256::digest(body).as_slice() != digest { return Err("Collection manifest checksum corrupt; preserve evidence".into()); }
        struct Cursor<'a>(&'a [u8]);
        impl<'a> Cursor<'a> {
            fn take(&mut self, n: usize) -> Result<&'a [u8]> {
                if n > self.0.len() { return Err("Collection manifest partial record".into()); }
                let (value, rest) = self.0.split_at(n); self.0 = rest; Ok(value)
            }
            fn u64(&mut self) -> Result<u64> { Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "Manifest integer")?)) }
        }
        let mut c = Cursor(&body[12..]);
        let generation = c.u64()?;
        let next = c.u64()?;
        let count = u32::from_le_bytes(c.take(4)?.try_into().map_err(|_| "Manifest count")?) as usize;
        if count > MAX_ITEMS || next == 0 { return Err("Collection manifest count/identity limit invalid".into()); }
        let mut items = Vec::new();
        items.try_reserve(count).map_err(|_| "Collection manifest allocation refused")?;
        let mut previous = 0;
        let mut total = 0_u64;
        for _ in 0..count {
            let id = c.u64()?;
            let bytes = c.u64()?;
            let category = match c.take(1)?[0] {
                1 => Category::Document,
                2 => return Err("Decoration collection metadata reserved; use a compatible future build, no mutation".into()),
                _ => return Err("Unsupported collection item metadata type".into()),
            };
            let name_len = u16::from_le_bytes(c.take(2)?.try_into().map_err(|_| "Manifest name length")?) as usize;
            if name_len == 0 || name_len > 255 { return Err("Collection manifest name allocation refused".into()); }
            let sha256 = c.take(32)?.try_into().map_err(|_| "Manifest digest length")?;
            let name = OsString::from_vec(c.take(name_len)?.to_vec());
            valid_name(&name)?;
            if id <= previous || id >= next || bytes > i64::MAX as u64 - 65536 { return Err("Collection manifest item identity/size invalid".into()); }
            total = total.checked_add(bytes).ok_or("Collection manifest usage overflow")?;
            if total > i64::MAX as u64 - 65536 { return Err("Collection manifest aggregate overflow".into()); }
            previous = id;
            items.push(Item { id, name, bytes, category, sha256 });
        }
        if !c.0.is_empty() { return Err("Collection manifest trailing data".into()); }
        Ok(Self { generation, next, items })
    }
}
