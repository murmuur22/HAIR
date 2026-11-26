//! Fixed proportional scale relative to explicit original dimensions. Legacy
//! records without this metadata mean 100%; reads never synthesize a rewrite.
use crate::Result;
pub const STOPS: [u16; 5] = [50, 75, 100, 125, 150];
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scaling {
    pub reference: [f32; 3],
    pub percent: u16,
}
impl Scaling {
    pub fn new(reference: [f32; 3], percent: u16) -> Result<Self> {
        let s = Self { reference, percent };
        s.size()?;
        Ok(s)
    }
    pub fn factor(&self) -> f32 { f32::from(self.percent) / 100. }
    pub fn size(&self) -> Result<[f32; 3]> {
        if !STOPS.contains(&self.percent) || self.reference.iter().any(|n| !n.is_finite() || *n <= 0. || *n > 16.) {
            return Err("Invalid proportional scale reference or percentage".into());
        }
        let size = self.reference.map(|n| n * self.factor());
        if size.iter().any(|n| !n.is_finite() || *n <= 0. || *n > 16.) {
            return Err("Scaled dimensions exceed placement bounds".into());
        }
        Ok(size)
    }
    pub fn validate_size(&self, size: [f32; 3]) -> Result<()> {
        if self.size()? != size { return Err("Scale dimensions disagree with original reference".into()); }
        Ok(())
    }
}
