//! Orbiting camera with VTK's trackball behaviour (azimuth/elevation around the focal point).

use glam::{DQuat, DVec3, Mat4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: DVec3,
    pub focal: DVec3,
    pub up: DVec3,
    /// Vertical field of view in degrees (VTK's default is 30).
    pub fov_y: f64,
}

impl Default for Camera {
    fn default() -> Self {
        let mut camera = Self { position: DVec3::ZERO, focal: DVec3::ZERO, up: DVec3::Z, fov_y: 30.0 };
        camera.view_from(DVec3::new(1.0, 1.0, 1.0), DVec3::Z, 100.0);
        camera
    }
}

impl Camera {
    pub fn distance(&self) -> f64 {
        (self.position - self.focal).length()
    }

    /// Unit vector from the camera toward the focal point.
    pub fn direction(&self) -> DVec3 {
        (self.focal - self.position).normalize_or(-DVec3::Z)
    }

    fn right(&self) -> DVec3 {
        self.direction().cross(self.up).normalize_or(DVec3::X)
    }

    /// Rotate around the focal point by `azimuth` (about the view-up) and `elevation` (about the right
    /// axis), in degrees, then re-orthogonalise the view-up — VTK's trackball camera.
    pub fn orbit(&mut self, azimuth: f64, elevation: f64) {
        let offset = self.position - self.focal;
        let yaw = DQuat::from_axis_angle(self.up.normalize(), azimuth.to_radians());
        let offset = yaw * offset;
        let right = (-offset).cross(self.up).normalize_or(DVec3::X);
        let pitch = DQuat::from_axis_angle(right, elevation.to_radians());
        let offset = pitch * offset;
        self.position = self.focal + offset;
        self.up = (pitch * self.up).normalize();
        self.orthogonalize_up();
    }

    fn orthogonalize_up(&mut self) {
        let direction = self.direction();
        self.up = self.right().cross(direction).normalize_or(DVec3::Z);
    }

    /// Move camera and focal point in the view plane by a number of pixels of a viewport `height` high.
    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64, height: f64) {
        let per_pixel = 2.0 * self.distance() * (self.fov_y.to_radians() / 2.0).tan() / height.max(1.0);
        let shift = (-dx_pixels * self.right() + dy_pixels * self.up) * per_pixel;
        self.position += shift;
        self.focal += shift;
    }

    /// Move toward (factor > 1) or away from (factor < 1) the focal point.
    pub fn dolly(&mut self, factor: f64) {
        let distance = (self.distance() / factor.max(1e-6)).max(1e-6);
        self.position = self.focal - self.direction() * distance;
    }

    /// Look at the focal point from `from` (a direction, need not be unit), with the given view-up and
    /// distance.
    pub fn view_from(&mut self, from: DVec3, up: DVec3, distance: f64) {
        self.position = self.focal + from.normalize() * distance;
        self.up = up;
        self.orthogonalize_up();
    }

    /// Centre on a box and back off until it fits, keeping the viewing direction.
    pub fn frame(&mut self, low: DVec3, high: DVec3) {
        let radius = ((high - low).length() / 2.0).max(1e-3);
        let direction = self.direction();
        self.focal = (low + high) / 2.0;
        let distance = radius / (self.fov_y.to_radians() / 2.0).sin();
        self.position = self.focal - direction * distance;
    }

    /// View and projection matrices; the clipping range is fitted to `scene` (min, max) when given.
    pub fn matrices(&self, aspect: f64, scene: Option<(DVec3, DVec3)>) -> (Mat4, Mat4) {
        let view = glam::dcamera::rh::view::look_at_mat4(self.position, self.focal, self.up.normalize());
        let direction = self.direction();
        let (mut near, mut far) = (self.distance() * 0.01, self.distance() * 100.0);
        if let Some((low, high)) = scene {
            let corners = (0..8).map(|i| {
                DVec3::new(
                    if i & 1 == 0 { low.x } else { high.x },
                    if i & 2 == 0 { low.y } else { high.y },
                    if i & 4 == 0 { low.z } else { high.z },
                )
            });
            let depths: Vec<f64> = corners.map(|c| (c - self.position).dot(direction)).collect();
            let farthest = depths.iter().copied().fold(f64::MIN, f64::max).max(1e-3) * 1.02;
            let nearest = depths.iter().copied().fold(f64::MAX, f64::min) * 0.98;
            far = farthest;
            near = nearest.max(farthest * 1e-4);
        }
        let projection =
            glam::dcamera::rh::proj::directx::perspective(self.fov_y.to_radians(), aspect.max(1e-6), near, far);
        (view.as_mat4(), projection.as_mat4())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_keeps_distance_and_up_orthogonal() {
        let mut camera = Camera::default();
        let distance = camera.distance();
        camera.orbit(37.0, -21.0);
        assert!((camera.distance() - distance).abs() < 1e-9);
        assert!(camera.up.dot(camera.direction()).abs() < 1e-9);
    }

    #[test]
    fn frame_centres_the_box() {
        let mut camera = Camera::default();
        camera.frame(DVec3::splat(10.0), DVec3::splat(30.0));
        assert!((camera.focal - DVec3::splat(20.0)).length() < 1e-9);
    }

    #[test]
    fn dolly_halves_distance() {
        let mut camera = Camera::default();
        let distance = camera.distance();
        camera.dolly(2.0);
        assert!((camera.distance() - distance / 2.0).abs() < 1e-9);
    }
}
