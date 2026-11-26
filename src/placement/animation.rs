//! Deterministic presentation modes. No scene/physics/storage authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    None,
    Breathe,
    BreatheRotate,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self { Self::None => "None", Self::Breathe => "Breathe", Self::BreatheRotate => "Breathe + rotate" }
    }
    pub fn offset(self, seconds: f64) -> f32 {
        if self == Self::None || !seconds.is_finite() { return 0.; }
        (0.012 * (1. - ((seconds.rem_euclid(6.) / 6.) * std::f64::consts::TAU).cos())) as f32
    }
    pub fn angle(self, seconds: f64) -> f32 {
        if self != Self::BreatheRotate || !seconds.is_finite() { return 0.; }
        ((seconds.rem_euclid(12.) / 12.) * std::f64::consts::TAU) as f32
    }
    /// A cylinder inscribed in the saved XZ rectangle. Even its diagonal at
    /// arbitrary presentation yaw fits; reserve breath inside saved height.
    pub fn fit(self, size: [f32; 3]) -> [f32; 3] {
        if self != Self::BreatheRotate { return size; }
        let side = size[0].min(size[2]) * std::f32::consts::FRAC_1_SQRT_2 * 0.9999;
        [side, (size[1] - self.amplitude(size[1])).max(size[1] * 0.5), side]
    }
    pub fn amplitude(self, height: f32) -> f32 { if self == Self::None { 0. } else { 0.024_f32.min(height * 0.25) } }
}
