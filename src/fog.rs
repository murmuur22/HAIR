//! Presentation-only distance fog. No scene admission, discovery or camera clipping.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fog {
    enabled: bool,
    distance: f32,
    tint: [f32; 3],
}
const DISTANCES: [f32; 3] = [8.0, 20.0, 40.0];
const TINTS: [[f32; 3]; 3] = [[0.04, 0.06, 0.05], [0.16, 0.24, 0.32], [0.28, 0.18, 0.10]];
impl Default for Fog {
    fn default() -> Self {
        Self {
            enabled: true,
            distance: 40.0,
            tint: TINTS[0],
        }
    }
}
impl Fog {
    /// Invalid nonfinite input is rejected before it can reach a GPU uniform.
    pub fn validated(mut self) -> crate::Result<Self> {
        if !self.distance.is_finite() || !self.tint.iter().all(|x| x.is_finite()) {
            return Err("Nonfinite fog preference".into());
        }
        self.distance = self.distance.clamp(4.0, 48.0);
        self.tint = self.tint.map(|x| x.clamp(0.0, 1.0));
        Ok(self)
    }
    pub fn enabled(self) -> bool {
        self.enabled
    }
    pub fn distance(self) -> f32 {
        self.distance
    }
    pub fn tint(self) -> [f32; 3] {
        self.tint
    }
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
    pub fn step_distance(&mut self, forward: bool) {
        let i = DISTANCES
            .iter()
            .position(|v| *v == self.distance)
            .unwrap_or(2);
        self.distance = DISTANCES[(i + if forward { 1 } else { 2 }) % 3];
    }
    pub fn step_tint(&mut self, forward: bool) {
        let i = TINTS.iter().position(|v| *v == self.tint).unwrap_or(0);
        self.tint = TINTS[(i + if forward { 1 } else { 2 }) % 3];
    }
    pub fn distance_label(self) -> String {
        format!("FOG DISTANCE: {:.0} M", self.distance)
    }
    pub fn tint_label(self) -> String {
        let name = match TINTS.iter().position(|v| *v == self.tint) {
            Some(0) => "NEUTRAL",
            Some(1) => "COOL",
            Some(2) => "WARM",
            _ => "CUSTOM",
        };
        format!("FOG TINT: {name}")
    }
    pub(crate) fn uniforms(self, eye: [f32; 3]) -> [[f32; 4]; 3] {
        let safe = self.validated().unwrap_or_default();
        [
            [eye[0], eye[1], eye[2], if safe.enabled { 1.0 } else { 0.0 }],
            [safe.distance * 0.5, safe.distance, 0.0, 0.0],
            [safe.tint[0], safe.tint[1], safe.tint[2], 0.0],
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_clamp_and_nonfinite_rejection() {
        let f = Fog {
            enabled: true,
            distance: 900.0,
            tint: [-2.0, 3.0, 0.5],
        }
        .validated()
        .unwrap();
        assert_eq!(f.distance(), 48.0);
        assert_eq!(f.tint(), [0.0, 1.0, 0.5]);
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(Fog { distance: x, ..f }.validated().is_err());
            assert!(
                Fog {
                    tint: [0.0, x, 0.0],
                    ..f
                }
                .validated()
                .is_err()
            );
            assert!(
                Fog { distance: x, ..f }
                    .uniforms([0.0; 3])
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite())
            );
        }
    }
}
