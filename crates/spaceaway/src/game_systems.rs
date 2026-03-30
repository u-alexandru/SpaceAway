use super::App;
use crate::mesh_utils::meshgen_to_render;
use crate::navigation;
use sa_ship::ship::Ship;
use std::time::Instant;

impl App {
    /// Update game systems: star lock-on, interaction, survival, gathering,
    /// star streaming, drive visuals, and audio.
    pub(super) fn update_game_systems(&mut self, dt: f32) {
        // --- Deferred star lock-on (Tab) ---
        // Runs AFTER camera orientation is fully set for this frame.
        // Searches ALL rendered stars (not just 15 nearest), so the
        // player can lock any visible star on screen.
        if self.wants_star_lock {
            self.wants_star_lock = false;
            if let Some(gpu) = &self.gpu {
                let sw = gpu.config.width as f32;
                let sh = gpu.config.height as f32;
                let cx = sw / 2.0;
                let cy = sh / 2.0;
                let aspect = sw / sh;
                let vp = self.camera.view_projection_matrix(aspect);

                let visible = self.star_streaming.visible_stars(self.galactic_position);
                let mut best_screen_dist = f32::MAX;
                let mut best: Option<(usize, f32)> = None; // (index into visible, px)
                // Margin around screen edges (pixels) — allows locking stars
                // slightly outside the viewport for a forgiving feel.
                let margin = 40.0_f32;

                for (i, (placed, _dist_ly)) in visible.iter().enumerate() {
                    let dx = (placed.position.x - self.galactic_position.x) as f32;
                    let dy = (placed.position.y - self.galactic_position.y) as f32;
                    let dz = (placed.position.z - self.galactic_position.z) as f32;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt();
                    if len < 0.001 { continue; }
                    let dir_norm = glam::Vec3::new(dx / len, dy / len, dz / len);
                    let dir = dir_norm * 90000.0;
                    let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                    if clip.w <= 0.0 { continue; } // behind camera
                    let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                    let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                    // Only consider stars within the screen bounds (+ margin)
                    if sx < -margin || sx > sw + margin || sy < -margin || sy > sh + margin {
                        continue;
                    }
                    let screen_dist = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                    if screen_dist < best_screen_dist {
                        best_screen_dist = screen_dist;
                        best = Some((i, screen_dist));
                    }
                }

                if let Some((idx, px_dist)) = best {
                    let (placed, dist_ly) = &visible[idx];
                    let nav_star = navigation::NavStar {
                        id: placed.id,
                        galactic_pos: placed.position,
                        catalog_name: navigation::catalog_name_from_id(placed.id),
                        distance_ly: *dist_ly,
                        color: placed.star.color,
                        spectral_class: placed.star.spectral_class,
                        luminosity: placed.star.luminosity,
                    };
                    log::info!("LOCKED: {} ({:.2} ly, {:.0}px from center, {} candidates)",
                        nav_star.catalog_name, nav_star.distance_ly, px_dist, visible.len());
                    self.navigation.lock_star(nav_star);
                    self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                } else {
                    log::warn!("No star visible on screen");
                }
            }
        }

        // Write live debug state in ALL modes (walk, helm, fly)
        if self.time.frame_count().is_multiple_of(30) {
            self.write_debug_state();
        }

        // Query pipeline already updated in walk branch. Only update in
        // non-walk modes (helm/fly don't update it).
        if self.fly_mode || self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
            self.physics.update_query_pipeline();
        }

        let t_interaction = Instant::now();
        // --- Interaction (runs in both standing and seated modes) ---
        let is_seated = self.helm.as_ref()
            .map(|h| h.is_seated())
            .unwrap_or(false);

        if let (Some(interaction), Some(player)) = (&mut self.interaction, &self.player)
            && !self.fly_mode
        {
            // Interactable colliders are children of the ship body (NOT
            // the interior body). Use camera.position as ray origin —
            // it's in the same coordinate space as the ship body.
            //
            // The query pipeline must be refreshed first: between
            // physics.step() and this point, the ship body may have
            // moved (gravity, cruise sync, rebase). The AABB tree
            // becomes stale and the broadphase can't find the colliders
            // even though ray and colliders moved by the same offset.
            self.physics.sync_collider_positions();
            self.physics.update_query_pipeline();

            let (ray_origin, ray_dir) = if is_seated {
                let pos = self.camera.position;
                let fwd = self.camera.forward();
                (
                    [pos.x as f32, pos.y as f32, pos.z as f32],
                    [fwd.x, fwd.y, fwd.z],
                )
            } else {
                let eye_pos = player.position(&self.physics);
                let fwd = self.camera.forward();
                (
                    [eye_pos.x as f32, eye_pos.y as f32, eye_pos.z as f32],
                    [fwd.x, fwd.y, fwd.z],
                )
            };

            let (_, mouse_dy) = self.input.mouse.delta();

            // Collision groups handle filtering: exclude_solids()
            // limits to sensors, and only interactable sensors are
            // registered in the collider_to_id map.
            // Debug: log ray origin + ship position every 120 frames
            if self.time.frame_count().is_multiple_of(120) {
                let st = self.ship.as_ref()
                    .and_then(|s| self.physics.get_body(s.body_handle))
                    .map(|b| *b.translation())
                    .unwrap_or(nalgebra::Vector3::zeros());
                log::info!(
                    "INTERACT_DIAG: ray_origin=({:.2},{:.2},{:.2}), ship_pos=({:.1},{:.1},{:.1}), cam=({:.1},{:.1},{:.1}), seated={}",
                    ray_origin[0], ray_origin[1], ray_origin[2],
                    st.x, st.y, st.z,
                    self.camera.position.x, self.camera.position.y, self.camera.position.z,
                    is_seated,
                );
            }

            let helm_clicked = interaction.update(
                ray_origin,
                ray_dir,
                mouse_dy,
                self.input.mouse.left_just_pressed(),
                self.input.mouse.left_pressed(),
                self.input.mouse.left_just_released(),
                &self.physics,
            );

            // SFX: lever drag start
            if self.input.mouse.left_just_pressed()
                && let Some(ids) = &self.ship_ids
                && interaction.hovered() == Some(ids.throttle_lever)
            {
                self.audio.play_sfx(sa_audio::SfxId::LeverMove, None);
            }

            // Update debug ray visualization
            let debug = interaction.debug_ray();
            let max_range = 2.0_f32;
            let end_dist = debug.hit
                .map(|(_, toi)| toi.min(max_range))
                .unwrap_or(max_range);
            let _end = [
                ray_origin[0] + ray_dir[0] * end_dist,
                ray_origin[1] + ray_dir[1] * end_dist,
                ray_origin[2] + ray_dir[2] * end_dist,
            ];
            let _color = if debug.hit_id.is_some() {
                [0.0, 1.0, 0.0] // green = hit interactable
            } else if debug.hit.is_some() {
                [1.0, 1.0, 0.0] // yellow = hit something but not interactable
            } else {
                [1.0, 0.0, 0.0] // red = miss
            };
            // Debug ray visualization disabled — was generating mesh + uploading
            // to GPU every frame (24ms!). Enable only when actively debugging.

            // If helm seat was clicked while standing, enter seated mode
            if !is_seated && helm_clicked.is_some()
                && let Some(helm) = &mut self.helm
            {
                helm.sit_down();
                // Reset helm look offset (face ship forward direction)
                self.helm_look_yaw = 0.0;
                self.helm_look_pitch = 0.0;
                // Disable player physics body while seated
                if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                    body.set_enabled(false);
                }
                log::info!("Entered helm seated mode — camera facing forward");
            }
        }


        // Sync interactable state -> ship state
        if let (Some(interaction), Some(ship), Some(ids)) =
            (&self.interaction, &mut self.ship, &self.ship_ids)
        {
            // Throttle lever -> ship.throttle
            if let Some(lever) = interaction.get(ids.throttle_lever) {
                ship.throttle = lever.lever_position().unwrap_or(0.0);
            }
            // Engine button -> ship.engine_on
            if let Some(button) = interaction.get(ids.engine_button) {
                ship.engine_on = button.is_button_pressed().unwrap_or(false);
            }
        }
        // Update interactable meshes ONLY when state changes (not every frame).
        // Regenerating meshes + uploading to GPU every frame is extremely expensive.
        if let (Some(interaction), Some(gpu), Some(renderer), Some(ids)) =
            (&self.interaction, &self.gpu, &mut self.renderer, &self.ship_ids)
        {
            // Update lever mesh only if position changed
            if let Some(lever) = interaction.get(ids.throttle_lever) {
                let pos = lever.lever_position().unwrap_or(0.0);
                if interaction.is_dragging() {
                    let mesh = sa_meshgen::interactables::lever_mesh(pos);
                    let mesh_data = meshgen_to_render(&mesh);
                    let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                    if let Some(slot) = self.interactable_meshes.get_mut(ids.throttle_lever) {
                        *slot = handle;
                    }
                }
            }

            // Update button mesh only on click (toggle frame)
            if let Some(button) = interaction.get(ids.engine_button)
                && self.input.mouse.left_just_released() && interaction.hovered() == Some(ids.engine_button) {
                    let pressed = button.is_button_pressed().unwrap_or(false);
                    self.audio.play_sfx(sa_audio::SfxId::ButtonClick, None);
                    if pressed {
                        self.audio.announce(sa_audio::VoiceId::EnginesIgniting);
                    }
                    let mesh = sa_meshgen::interactables::button_mesh(pressed);
                    let mesh_data = meshgen_to_render(&mesh);
                    let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                    if let Some(slot) = self.interactable_meshes.get_mut(ids.engine_button) {
                        *slot = handle;
                    }
            }
        }

        // Landing lock button: toggle landing lock when clicked.
        if let (Some(interaction), Some(ids)) = (&self.interaction, &self.ship_ids)
            && self.input.mouse.left_just_released()
            && interaction.hovered() == Some(ids.landing_lock_button)
        {
            self.landing.request_lock_toggle();
            self.audio.play_sfx(sa_audio::SfxId::ButtonClick, None);
            log::debug!("Landing lock toggle requested");
        }

        // --- Survival resources update ---
        {
            let throttle = self.ship.as_ref().map(|s| s.throttle).unwrap_or(0.0);
            let engine_on = self.ship.as_ref().map(|s| s.engine_on).unwrap_or(false);
            let is_seated = self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false);
            if is_seated {
                self.ship_resources.update_with_drive(
                    dt,
                    throttle,
                    engine_on,
                    self.drive.mode(),
                    self.drive.speed_fraction(),
                );
            } else {
                self.ship_resources.update_with_drive(
                    dt,
                    throttle,
                    engine_on,
                    sa_ship::DriveMode::Impulse,
                    0.0,
                );
            }

            // Emergency warp drop on empty exotic fuel
            if self.drive.mode() == sa_ship::DriveMode::Warp
                && self.ship_resources.exotic_fuel <= 0.0
            {
                self.drive.request_disengage();
                log::warn!("WARP DRIVE FAILED — exotic fuel exhausted!");
                self.audio.announce(sa_audio::VoiceId::Danger);
            }

            // Low fuel consequence: reduce max thrust linearly below 20%
            if let Some(ship) = &mut self.ship {
                if self.ship_resources.fuel < 0.2 {
                    ship.max_thrust = self.ship_resources.fuel * 5.0 * Ship::DEFAULT_MAX_THRUST;
                } else {
                    ship.max_thrust = Ship::DEFAULT_MAX_THRUST;
                }
            }
        }

        // --- Suit resources: drain when ship fails, recharge when ship works ---
        self.suit.update(
            dt,
            self.ship_resources.oxygen > 0.1,
            self.ship_resources.power > 0.0,
        );

        // --- Gathering: check proximity to deposits ---
        {
            let ship_pos = self.ship.as_ref()
                .and_then(|s| s.position(&self.physics))
                .unwrap_or((0.0, 0.0, 0.0));

            let mut nearest: Option<(usize, f32)> = None;
            for (i, dep) in self.deposits.iter().enumerate() {
                if self.gathered.contains(&dep.id) {
                    continue;
                }
                let dx = dep.position[0] - ship_pos.0;
                let dy = dep.position[1] - ship_pos.1;
                let dz = dep.position[2] - ship_pos.2;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist < 500.0
                    && (nearest.is_none() || dist < nearest.unwrap().1)
                {
                    nearest = Some((i, dist));
                }
            }
            self.nearest_gatherable = nearest.map(|(i, _)| i);

            // Gather on click when in range (and not interacting with ship controls)
            if let Some(idx) = self.nearest_gatherable
                && self.input.mouse.left_just_pressed()
            {
                let dep = &self.deposits[idx];
                match dep.kind {
                    sa_survival::ResourceKind::FuelAsteroid => {
                        self.ship_resources.add_fuel(dep.amount);
                    }
                    sa_survival::ResourceKind::SupplyCache => {
                        self.ship_resources.add_oxygen(dep.amount);
                    }
                    sa_survival::ResourceKind::Derelict => {
                        self.ship_resources.add_fuel(dep.amount);
                        self.ship_resources.add_oxygen(dep.amount);
                    }
                }
                log::info!(
                    "Gathered {} (+{:.0}%) at [{:.0}, {:.0}, {:.0}]",
                    dep.kind.label(),
                    dep.amount * 100.0,
                    dep.position[0], dep.position[1], dep.position[2],
                );
                self.gathered.insert(dep.id);
                self.nearest_gatherable = None;
            }
        }

        // Update speed screen text
        if let (Some(interaction), Some(ship)) = (&mut self.interaction, &self.ship)
            && let Some(ids) = &self.ship_ids
        {
            let speed = ship.speed(&self.physics);
            if let Some(screen) = interaction.get_mut(ids.speed_screen) {
                let speed_line = match self.drive.mode() {
                    sa_ship::DriveMode::Warp if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                        let ly_s = self.drive.current_speed_c() * 3.169e-8;
                        format!("Speed: {:.3} ly/s", ly_s)
                    }
                    sa_ship::DriveMode::Cruise if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                        format!("Speed: {:.0}c", self.drive.current_speed_c())
                    }
                    _ => format!("Speed: {:.1} m/s", speed),
                };
                screen.set_screen_text(vec![
                    speed_line,
                    format!("Throttle: {:.0}%", ship.throttle * 100.0),
                    format!("Engine: {}", if ship.engine_on { "ON" } else { "OFF" }),
                ]);
            }
        }

        let interaction_us = t_interaction.elapsed().as_micros() as u64;
        self.perf.stars_us = interaction_us; // repurpose stars_us for interaction timing

        // --- Star regen ---
        let t2 = Instant::now();
        self.maybe_regenerate_stars();
        self.perf.stars_us = t2.elapsed().as_micros() as u64;

        // Update navigation (nearby stars for window markers + gravity well)
        self.navigation.update_nearby(self.galactic_position);

        // --- Drive visuals update (smooth shader parameter transitions) ---
        let drive_dir = if let Some(ship) = &self.ship {
            if let Some(body) = self.physics.get_body(ship.body_handle) {
                let rot = body.rotation();
                let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                [fwd.x, fwd.y, fwd.z]
            } else {
                [0.0, 0.0, -1.0]
            }
        } else {
            [0.0, 0.0, -1.0]
        };
        self.drive_visuals.update(&self.drive, drive_dir, dt);

        // --- Audio update ---
        {
            let fwd = self.camera.forward();
            let right = self.camera.right();
            let up = fwd.cross(right).normalize_or_zero();
            let cam_pos = self.camera.position;
            self.audio.set_listener(sa_audio::Listener {
                position: glam::Vec3::new(
                    cam_pos.x as f32,
                    cam_pos.y as f32,
                    cam_pos.z as f32,
                ),
                forward: fwd,
                up: if up.length() > 0.5 { up } else { glam::Vec3::Y },
            });

            // Engine state
            let engine_state = if let Some(ship) = &self.ship {
                if !ship.engine_on {
                    sa_audio::EngineState::Off
                } else if self.drive.mode() == sa_ship::DriveMode::Warp {
                    if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) {
                        sa_audio::EngineState::WarpEngaged
                    } else {
                        sa_audio::EngineState::WarpSpool
                    }
                } else if self.drive.mode() == sa_ship::DriveMode::Cruise {
                    sa_audio::EngineState::Cruise
                } else if ship.throttle > 0.01 {
                    sa_audio::EngineState::Impulse
                } else {
                    sa_audio::EngineState::Idle
                }
            } else {
                sa_audio::EngineState::Off
            };
            self.audio.set_engine_state(engine_state);

            // Music context
            let music_ctx = if self.drive.mode() == sa_ship::DriveMode::Warp
                && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
            {
                sa_audio::MusicContext::Warp
            } else if self.ship_resources.fuel < 0.2
                || self.ship_resources.exotic_fuel < 0.1
            {
                sa_audio::MusicContext::Tension
            } else if self.active_system.is_some() {
                sa_audio::MusicContext::Exploration
            } else {
                sa_audio::MusicContext::Idle
            };
            self.audio.set_music_context(music_ctx);

            // Power state
            self.audio.set_power(self.ship_resources.power > 0.0);

            // Fuel warnings
            if self.ship_resources.fuel < 0.2 && !self.fuel_low_announced {
                self.audio.announce(sa_audio::VoiceId::EnergyLow);
                self.fuel_low_announced = true;
            }
            if self.ship_resources.fuel >= 0.3 {
                self.fuel_low_announced = false; // reset when fuel recovers
            }

            // Altitude proximity beep: fires when descending within 100 m.
            // Beep rate scales with closeness to ground.
            self.altitude_beep_timer -= dt;
            if let Some(clearance) = self.last_clearance {
                if clearance < 100.0 && self.altitude_beep_timer <= 0.0 {
                    self.audio.play_sfx(sa_audio::SfxId::AltitudeBeep, None);
                    // Reset timer: interval shrinks as altitude drops.
                    self.altitude_beep_timer = if clearance < 5.0 {
                        0.05 // near-continuous (20/sec) — avoids 60 overlapping samples
                    } else if clearance < 10.0 {
                        1.0 / 8.0
                    } else if clearance < 20.0 {
                        1.0 / 4.0
                    } else if clearance < 50.0 {
                        1.0 / 2.0
                    } else {
                        1.0 // 1 beep/sec at 50–100 m
                    };
                }
            } else {
                // No clearance data — reset timer so beep starts promptly on entry.
                self.altitude_beep_timer = 0.0;
            }

            self.audio.update(dt);
        }

    }
}
