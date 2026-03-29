use super::App;

impl App {
    /// Update player physics, helm/walk mode, and camera for the current frame.
    /// Called once per frame during the Playing phase.
    pub(super) fn update_player_physics(&mut self, dt: f32) {
        if self.fly_mode {
            // Fly mode: move galactic position in light-years, bypass physics.
            // camera.position tracks galactic_position in fly mode.
            let (dx, dy) = self.input.mouse.delta();
            self.camera.rotate(dx * 0.003, -dy * 0.003);

            let fwd = self.camera.forward();
            let right = self.camera.right();
            let speed = self.fly_speed * dt as f64;

            use winit::keyboard::KeyCode as KC;
            if self.input.keyboard.is_pressed(KC::KeyW) {
                self.galactic_position.x += fwd.x as f64 * speed;
                self.galactic_position.y += fwd.y as f64 * speed;
                self.galactic_position.z += fwd.z as f64 * speed;
            }
            if self.input.keyboard.is_pressed(KC::KeyS) {
                self.galactic_position.x -= fwd.x as f64 * speed;
                self.galactic_position.y -= fwd.y as f64 * speed;
                self.galactic_position.z -= fwd.z as f64 * speed;
            }
            if self.input.keyboard.is_pressed(KC::KeyA) {
                self.galactic_position.x -= right.x as f64 * speed;
                self.galactic_position.z -= right.z as f64 * speed;
            }
            if self.input.keyboard.is_pressed(KC::KeyD) {
                self.galactic_position.x += right.x as f64 * speed;
                self.galactic_position.z += right.z as f64 * speed;
            }
            if self.input.keyboard.is_pressed(KC::Space) {
                self.galactic_position.y += speed;
            }
            if self.input.keyboard.is_pressed(KC::ShiftLeft) {
                self.galactic_position.y -= speed;
            }
            // Camera follows galactic position in fly mode
            self.camera.position = self.galactic_position;
        } else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
            self.update_helm_mode(dt);
        } else {
            self.update_walk_mode(dt);
        }
    }
}
