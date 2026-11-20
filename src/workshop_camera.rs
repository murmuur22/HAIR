//! Workshop-only session camera. No project, host, collision or persistence data.
#[derive(Debug)]
pub struct InspectionCamera {
    pub pose: crate::Pose,
    pub height: f32,
    pub free_fly: bool,
    pub captured: bool,
    held: [bool; 6],
}
impl Default for InspectionCamera {
    fn default() -> Self {
        Self {
            pose: crate::Pose {
                position: [0.0, 3.5],
                yaw: 0.0,
                pitch: -0.2,
            },
            height: 1.65,
            free_fly: false,
            captured: false,
            held: [false; 6],
        }
    }
}
impl InspectionCamera {
    pub fn capture(&mut self) {
        if self.free_fly {
            self.release();
            self.captured = true;
        }
    }
    pub fn release(&mut self) {
        self.captured = false;
        self.held.fill(false);
    }
    pub fn frame(&mut self) {
        let mode = self.free_fly;
        *self = Self::default();
        self.free_fly = mode;
    }
    pub fn key(&mut self, key: egui::Key, pressed: bool) {
        let index = match key {
            egui::Key::W => 0,
            egui::Key::S => 1,
            egui::Key::A => 2,
            egui::Key::D => 3,
            egui::Key::Q => 4,
            egui::Key::E => 5,
            _ => return,
        };
        self.held[index] = pressed && self.captured;
    }
    pub fn look(&mut self, dx: f32, dy: f32) {
        if self.captured && dx.is_finite() && dy.is_finite() {
            self.pose.yaw = (self.pose.yaw + dx.clamp(-1000.0, 1000.0) * 0.0025)
                .rem_euclid(std::f32::consts::TAU);
            self.pose.pitch =
                (self.pose.pitch - dy.clamp(-1000.0, 1000.0) * 0.0025).clamp(-1.5, 1.5);
        }
    }
    pub fn step(&mut self, dt: f32) {
        if !self.captured || !dt.is_finite() {
            return;
        }
        let axis = |p: usize, n: usize| (i32::from(self.held[p]) - i32::from(self.held[n])) as f32;
        let local = glam::Vec3::new(axis(3, 2), axis(5, 4), axis(1, 0)).normalize_or_zero();
        let (s, c) = self.pose.yaw.sin_cos();
        let d = glam::Vec3::new(
            c * local.x - s * local.z,
            local.y,
            s * local.x + c * local.z,
        ) * dt.clamp(0.0, 0.1)
            * 2.5;
        self.pose.position[0] = (self.pose.position[0] + d.x).clamp(-100.0, 100.0);
        self.pose.position[1] = (self.pose.position[1] + d.z).clamp(-100.0, 100.0);
        self.height = (self.height + d.y).clamp(-100.0, 100.0);
    }
}
