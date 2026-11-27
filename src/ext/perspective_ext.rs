use super::*;

/// Extension helpers to read and apply camera perspective (`F5` / `CameraType`).
pub trait PerspectiveExt {
    /// Returns the active camera perspective (`0 = FirstPerson`, `1 = ThirdPersonBack`, `2 = ThirdPersonFront`).
    fn rreturn_camera_type(&self) -> u8;
    /// Sets the camera perspective through Horizon's own transition helper.
    fn rreturn_set_camera_type(&mut self, mode: u8);
}

impl PerspectiveExt for Game<'_> {
    fn rreturn_camera_type(&self) -> u8 {
        match self.requested_camera_type.get().unwrap_or(self.camera_type) {
            horizon_client::camera::CameraType::FirstPerson => 0,
            horizon_client::camera::CameraType::ThirdPersonBack => 1,
            horizon_client::camera::CameraType::ThirdPersonFront => 2,
        }
    }

    fn rreturn_set_camera_type(&mut self, mode: u8) {
        let target = match mode {
            1 => horizon_client::camera::CameraType::ThirdPersonBack,
            2 => horizon_client::camera::CameraType::ThirdPersonFront,
            _ => horizon_client::camera::CameraType::FirstPerson,
        };
        if self.requested_camera_type.get().unwrap_or(self.camera_type) != target {
            self.apply_camera_type(target);
        } else {
            self.requested_camera_type.take();
        }
    }
}
