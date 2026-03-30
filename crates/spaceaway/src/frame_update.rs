use super::App;
use crate::constants::LY_TO_M;

impl App {
    /// Update approach state BEFORE helm_mode runs, so cruise speed cap
    /// and flythrough prevention have current data.
    pub(super) fn update_approach_state(&mut self) {
        let find_planet = self.active_system.as_ref().and_then(|sys| {
            let positions = sys.compute_positions_ly_pub();
            let ly_to_m = LY_TO_M;
            let mut best: Option<(usize, sa_math::WorldPos, f64, f64)> = None;
            for (i, pos) in positions.iter().enumerate() {
                let r = match sys.body_radius_m(i) {
                    Some(r) => r,
                    None => continue,
                };
                if sys.planet_data(i).is_none() { continue; }
                let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if best.as_ref().is_none_or(|b| dist < b.3) {
                    best = Some((i, *pos, r, dist));
                }
            }
            best.map(|(i, pos, r, _)| (i, pos, r))
        });
        let landed = self.landing.state() == crate::landing::LandingState::Landed;
        let state = self.approach.update(self.galactic_position, find_planet, landed);
        self.approach_state = Some(state);
    }

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
