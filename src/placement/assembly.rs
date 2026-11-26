//! Shared file/base attributes and logical assembly contract. No file collider
//! exists in this schema. Theme roles provide art, never collision dimensions.
use crate::{Result, assets::Vertex, host_scene::Geometry, placement_store::Position};
pub const BASE_SIZE: [f32; 3] = [0.8, 0.8, 0.6];
pub const FILE_LIFT: f32 = 0.92;
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "RawProperties")]
pub struct Properties {
    pub base: bool,
    pub base_collision: bool,
    /// Presentation only; never changes Position's authoritative transform.
    pub animated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation: Option<super::animation::Mode>,
    pub label: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaling: Option<super::scaling::Scaling>,
}
impl Default for Properties {
    fn default() -> Self {
        Self {
            base: false,
            base_collision: true,
            animated: false,
            animation: None,
            label: true,
            scaling: None,
        }
    }
}
impl Properties {
    pub fn mode(&self) -> super::animation::Mode {
        self.animation.unwrap_or(if self.animated { super::animation::Mode::Breathe } else { super::animation::Mode::None })
    }
    pub fn set_mode(&mut self, mode: super::animation::Mode) {
        self.animated = mode != super::animation::Mode::None;
        self.animation = Some(mode);
    }
    pub fn validate(&self) -> Result<()> {
        if self.animation.is_some_and(|m| (m != super::animation::Mode::None) != self.animated) {
            return Err("Contradictory animated bool and explicit animation mode".into());
        }
        Ok(())
    }
    pub fn scale_factor(&self) -> f32 { self.scaling.map_or(1., |s| s.factor()) }
    pub fn scale_percent(&self) -> u16 { self.scaling.map_or(100, |s| s.percent) }
    pub fn scaled_size(&self, legacy: [f32; 3]) -> Result<[f32; 3]> {
        self.scaling.map_or(Ok(legacy), |s| s.size())
    }
    pub fn base_label(&self) -> &'static str {
        if self.base {
            "G: base ON"
        } else {
            "G: base OFF"
        }
    }
    pub fn collision_label(&self) -> &'static str {
        if !self.base {
            "H: base collision unavailable (base OFF)"
        } else if self.base_collision {
            "H: base collision ON"
        } else {
            "H: base collision OFF"
        }
    }
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    /// Six-second, at most 24 mm vertical breath. Invalid times are stationary.
    pub fn file_offset(&self, seconds: f64) -> f32 {
        self.mode().offset(seconds)
    }
    pub fn present_file(&self, mesh: &mut [Vertex], seconds: f64) {
        let offset = self.file_offset(seconds);
        if offset == 0. {
            return;
        }
        for v in mesh {
            v.position[1] += offset;
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawProperties {
    base: bool, base_collision: bool, animated: bool, label: bool,
    animation: Option<super::animation::Mode>,
    scaling: Option<super::scaling::Scaling>,
}
impl Default for RawProperties {
    fn default() -> Self { Self { base: false, base_collision: true, animated: false, label: true, animation: None, scaling: None } }
}
impl TryFrom<RawProperties> for Properties {
    type Error = String;
    fn try_from(p: RawProperties) -> Result<Self> {
        let p = Self { base: p.base, base_collision: p.base_collision, animated: p.animated,
            label: p.label, animation: p.animation, scaling: p.scaling };
        p.validate()?; Ok(p)
    }
}
impl Position {
    /// New rotation stays inside the saved local envelope, including its breath.
    /// Legacy Breathe keeps the historical 24mm presentation displacement.
    pub fn present_model(&self, model: &[Vertex], seconds: f64) -> Result<Vec<Vertex>> {
        self.validate()?;
        let mode = self.properties.mode();
        if mode != super::animation::Mode::BreatheRotate {
            let mut mesh = self.transform(model)?;
            self.properties.present_file(&mut mesh, seconds);
            return Ok(mesh);
        }
        let mut display = self.clone();
        if self.properties.base {
            display.surface = Some(crate::placement::surface::Contact {
                at: [self.at[0], self.surface.map_or(0., |s|s.at[1]) + FILE_LIFT * self.properties.scale_factor(), self.at[1]], normal: [0.,1.,0.],
            });
            display.properties.base = false;
        }
        display.properties.scaling = None;
        display.size = mode.fit(self.size);
        display.yaw = (self.yaw + mode.angle(seconds)).rem_euclid(std::f32::consts::TAU);
        let mut mesh = display.transform(model)?;
        let normal = self.surface.map_or([0.,1.,0.], |s|s.normal);
        let offset = mode.offset(seconds) * mode.amplitude(self.size[1]) / 0.024;
        for v in &mut mesh { for (i, component) in normal.iter().enumerate() { v.position[i] += component * offset; } }
        Ok(mesh)
    }
    pub fn base_bounds(&self) -> Result<Option<Geometry>> {
        self.validate()?;
        if !self.properties.base {
            return Ok(None);
        }
        let (s, c) = self.yaw.sin_cos();
        Ok(Some(Geometry {
            at: [self.at[0], self.surface.map_or(0., |s| s.at[1]), self.at[1]],
            size: [
                (c.abs() * BASE_SIZE[0] + s.abs() * BASE_SIZE[2]) * self.properties.scale_factor(),
                BASE_SIZE[1] * self.properties.scale_factor(),
                (s.abs() * BASE_SIZE[0] + c.abs() * BASE_SIZE[2]) * self.properties.scale_factor(),
            ],
        }))
    }
    /// Logical envelope for preview/clearance; never a file physics object.
    pub fn envelope_mesh(&self) -> Result<Vec<Vertex>> {
        let mut b = self.bounds()?;
        // Historical Breathe rises in world Y, bounded by 24mm. Charge that
        // swept visual clearance at admission, never enlarge the saved collider.
        if self.properties.mode() == super::animation::Mode::Breathe { b.size[1] += 0.024; }
        let mut mesh = vec![];
        crate::scene::cuboid(
            &mut mesh,
            [b.at[0], b.at[1] + b.size[1] * 0.5, b.at[2]],
            b.size,
            [1.; 3],
        );
        Ok(mesh)
    }
    pub fn base_transform(&self, model: &[Vertex]) -> Result<Vec<Vertex>> {
        if !self.properties.base {
            return Ok(vec![]);
        }
        self.validate()?;
        if model.is_empty() {
            let mut mesh = vec![];
            crate::scene::cuboid(&mut mesh, [0., 0.4, 0.], BASE_SIZE, [0.4, 0.48, 0.36]);
            return self.base_transform(&mesh);
        }
        let mut mesh = crate::placement::grounded(model, self.at, self.yaw, BASE_SIZE.map(|n| n * self.properties.scale_factor()))?;
        for v in &mut mesh {
            v.position[1] += self.surface.map_or(0., |s| s.at[1]);
        }
        Ok(mesh)
    }
}
