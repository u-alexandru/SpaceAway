use super::App;
use crate::landing;
use crate::ship_colliders;
use std::time::Instant;

impl App {
    /// Update physics and camera for the walk mode (kinematic character controller).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn update_walk_mode(&mut self, dt: f32) {
        // --- Walk mode: kinematic character controller ---
        //
        // CORRECT ORDER (prevents drift and teleporting):
        // 1. Apply forces to ship
        // 2. Run physics step (ship moves to final position)
        // 3. Update query pipeline (colliders at final position)
        // 4. Read ship's POST-STEP velocity
        // 5. Move player using post-step velocity + move_shape
        //
        // This ensures the player moves relative to where the ship
        // ACTUALLY IS after the step, not where it WAS before.

        // Sync interior collider rotation on first walk-mode frame.
        // When transitioning from fly mode or helm mode, the interior
        // body may be stale. Syncing at the top ensures the very first
        // move_shape sweep uses correct collider positions.
        if let (Some(ship), Some(ih)) = (&self.ship, ship_colliders::interior_body_handle()) {
            let ship_rot = self.physics.get_body(ship.body_handle)
                .map(|b| *b.rotation());
            if let (Some(rot), Some(ib)) = (ship_rot, self.physics.get_body_mut(ih)) {
                ib.set_position(
                    nalgebra::Isometry3::from_parts(
                        nalgebra::Translation3::identity(),
                        rot,
                    ),
                    true,
                );
            }
        }

        // MANUAL ship integration — no physics.step() needed.
        // physics.step() is extremely expensive with many colliders
        // (47ms at 345 m/s). Since the player is kinematic (uses
        // move_shape, not contacts) and the ship is the only dynamic
        // body, we integrate the ship manually: v += a*dt, p += v*dt.
        let physics_dt = dt.min(1.0 / 30.0);
        let t_phys = Instant::now();

        // Record ship transform BEFORE integration (needed to carry player)
        let (ship_pos_before, ship_rot_before) = self.ship.as_ref()
            .and_then(|s| self.physics.get_body(s.body_handle))
            .map(|b| (b.translation().clone_owned(), *b.rotation()))
            .unwrap_or((nalgebra::Vector3::zeros(), nalgebra::UnitQuaternion::identity()));

        // Manually integrate ship: apply thrust as velocity change.
        // Skip all integration when landed — ship is frozen on the surface.
        if self.landing.state() == landing::LandingState::Landed {
            // Ship is locked on the ground — zero velocity and hold position.
            if let Some(ship) = &self.ship
                && let Some(body) = self.physics.get_body_mut(ship.body_handle)
            {
                body.set_linvel(nalgebra::Vector3::zeros(), true);
                body.set_angvel(nalgebra::Vector3::zeros(), true);
                body.set_gravity_scale(0.0, true);
            }
        } else if let Some(ship) = &self.ship
            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
        {
                // Apply thrust: F = throttle * max_thrust * engine_on
                if ship.engine_on && ship.throttle > 0.0 {
                    let rot = *body.rotation();
                    let forward = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                    let accel = forward * (ship.throttle * ship.max_thrust / body.mass());
                    let vel = body.linvel() + accel * physics_dt;
                    body.set_linvel(vel, true);
                }
                // Linear damping always applies (not just when engine on).
                // Without this, gravity accumulates velocity without bound.
                {
                    let vel = body.linvel() * (1.0 - 0.001 * physics_dt).max(0.0);
                    body.set_linvel(vel, true);
                }
                // Apply terrain gravity (planet surface pull).
                if let Some(ref grav) = self.terrain_gravity
                    && grav.blend > 0.01
                {
                    let grav_accel = nalgebra::Vector3::new(
                        grav.direction[0] * grav.magnitude,
                        grav.direction[1] * grav.magnitude,
                        grav.direction[2] * grav.magnitude,
                    );
                    let vel = body.linvel() + grav_accel * physics_dt;
                    body.set_linvel(vel, true);

                    // Atmospheric drag: increases with gravity blend.
                    // At full blend (surface), drag = 0.05/s → terminal velocity
                    // ~200 m/s under Earth gravity (9.81/0.05 = 196 m/s).
                    // At atmosphere edge (blend ~0), negligible drag.
                    let atmo_drag = 0.05 * grav.blend;
                    let vel = body.linvel() * (1.0 - atmo_drag * physics_dt).max(0.0);
                    body.set_linvel(vel, true);
                }
                // Ground contact response (walk-mode substitute for
                // rapier contact solving, since physics.step() is skipped).
                // When SLIDING, cancel downward velocity and apply friction.
                if self.landing.state() == landing::LandingState::Sliding
                    && let Some(ref grav) = self.terrain_gravity
                {
                    let gdir = nalgebra::Vector3::new(
                        grav.direction[0], grav.direction[1], grav.direction[2],
                    );
                    let vel = *body.linvel();
                    let down_speed = vel.dot(&gdir);
                    if down_speed > 0.0 {
                        // Cancel velocity into ground (normal reaction).
                        let corrected = vel - gdir * down_speed;
                        body.set_linvel(corrected, true);
                    }
                    // Ground friction: combined friction 0.6 * 0.8 = 0.48.
                    // Deceleration = friction * gravity.
                    let friction_decel = 0.48 * grav.magnitude;
                    let tangent_vel = *body.linvel();
                    let tangent_speed = tangent_vel.magnitude();
                    if tangent_speed > 0.01 {
                        let decel = (friction_decel * physics_dt).min(tangent_speed);
                        let braked = tangent_vel * (1.0 - decel / tangent_speed);
                        body.set_linvel(braked, true);
                    }
                }
                // Angular damping ALWAYS applies (not just when engine on).
                // Without this, Q/E roll torques create permanent spin.
                let angvel = body.angvel() * (1.0 - 5.0 * physics_dt).max(0.0);
                body.set_angvel(angvel, true);
                // Integrate position: p += v * dt
                let vel = *body.linvel();
                let pos = body.translation() + vel * physics_dt;
                let angvel = *body.angvel();
                let rot = *body.rotation();
                let drot = nalgebra::UnitQuaternion::new(angvel * physics_dt * 0.5);
                let new_rot = drot * rot;
                body.set_position(
                    nalgebra::Isometry3::from_parts(
                        nalgebra::Translation3::from(pos),
                        new_rot,
                    ),
                    true,
                );
        }
        let phys_step_us = t_phys.elapsed().as_micros();

        // Step 3: Get ship transform AFTER integration
        let (ship_pos_after, ship_rot_after) = self.ship.as_ref()
            .and_then(|s| self.physics.get_body(s.body_handle))
            .map(|b| (b.translation().clone_owned(), *b.rotation()))
            .unwrap_or((nalgebra::Vector3::zeros(), nalgebra::UnitQuaternion::identity()));

        // Track ship physics into galactic_position (impulse only).
        // When terrain is active, compute exactly from anchor + ship rapier pos.
        // When terrain is NOT active, use delta-based tracking.
        if self.drive.mode() == sa_ship::DriveMode::Impulse
            && self.ship.is_some()
        {
            if let Some(terrain_mgr) = &self.terrain {
                let anchor = terrain_mgr.anchor_f64();
                let planet = terrain_mgr.frozen_planet_center_ly();
                let ly_to_m = 9.461e15_f64;
                self.galactic_position.x = planet.x + (anchor[0] + ship_pos_after.x as f64) / ly_to_m;
                self.galactic_position.y = planet.y + (anchor[1] + ship_pos_after.y as f64) / ly_to_m;
                self.galactic_position.z = planet.z + (anchor[2] + ship_pos_after.z as f64) / ly_to_m;
            } else {
                let m_to_ly = 1.0 / 9.461e15_f64;
                self.galactic_position.x += (ship_pos_after.x as f64 - ship_pos_before.x as f64) * m_to_ly;
                self.galactic_position.y += (ship_pos_after.y as f64 - ship_pos_before.y as f64) * m_to_ly;
                self.galactic_position.z += (ship_pos_after.z as f64 - ship_pos_before.z as f64) * m_to_ly;
            }
        }

        // --- Landing system update (walk mode) ---
        if let Some(ship) = &self.ship {
            let ship_iso = self.physics.get_body(ship.body_handle)
                .map(|b| *b.position())
                .unwrap_or_else(nalgebra::Isometry3::identity);
            let ship_speed_ms = ship.speed(&self.physics);
            let ship_vel_raw = ship.speed_vector(&self.physics);
            let ship_velocity = nalgebra::Vector3::new(
                ship_vel_raw.0, ship_vel_raw.1, ship_vel_raw.2,
            );
            let gravity_raw = self.terrain_gravity.as_ref()
                .map(|g| g.direction)
                .unwrap_or([0.0, -1.0, 0.0]);
            let gravity_dir = nalgebra::Unit::new_normalize(
                nalgebra::Vector3::new(gravity_raw[0], gravity_raw[1], gravity_raw[2]),
            );
            let planet_gravity = self.terrain_gravity.as_ref()
                .map(|g| g.magnitude)
                .unwrap_or(0.0);
            let terrain_active = self.terrain.is_some();
            let landing_result = self.landing.update(landing::LandingParams {
                physics: &self.physics,
                ship_iso: &ship_iso,
                ship_speed_ms,
                ship_velocity,
                gravity_dir,
                planet_gravity,
                terrain_active,
                engine_on: ship.engine_on,
                throttle: ship.throttle,
            });
            self.last_clearance = landing_result.min_clearance;

            // Diagnostic logging when terrain is active (throttled).
            if terrain_active && self.time.frame_count().is_multiple_of(60) {
                log::info!(
                    "LANDING_DIAG: state={:?}, speed={:.1}m/s, vertical={:.1}m/s, clearance={:.1}m, skid_contact={}",
                    landing_result.state,
                    ship_speed_ms,
                    ship_velocity.dot(&gravity_dir).abs(),
                    landing_result.min_clearance.unwrap_or(100.0),
                    landing_result.skid_contact,
                );
            }

            // Freeze ship when LANDED; restore on unlock.
            if landing_result.state == landing::LandingState::Landed
                && let Some(body) = self.physics.get_body_mut(ship.body_handle)
            {
                body.set_linvel(nalgebra::Vector3::zeros(), true);
                body.set_angvel(nalgebra::Vector3::zeros(), true);
                body.set_gravity_scale(0.0, true);
            }
            // Restore gravity_scale when transitioning OUT of Landed.
            if landing_result.previous_state == landing::LandingState::Landed
                && landing_result.state != landing::LandingState::Landed
                && let Some(body) = self.physics.get_body_mut(ship.body_handle)
            {
                body.set_gravity_scale(0.0, true);
                body.set_linvel(nalgebra::Vector3::zeros(), true);
            }
            if let Some(ref impact) = landing_result.impact {
                log::info!(
                    "IMPACT: {:?} at {:.1} m/s",
                    impact.category,
                    impact.impact_speed_ms
                );
                match impact.category {
                    landing::ImpactCategory::Clean => {
                        self.audio.play_sfx(sa_audio::SfxId::ImpactSoft, None);
                    }
                    landing::ImpactCategory::Minor => {
                        self.audio.play_sfx(sa_audio::SfxId::ImpactHeavy, None);
                    }
                    landing::ImpactCategory::Major => {
                        self.audio.play_sfx(sa_audio::SfxId::ImpactCrash, None);
                        self.audio.play_alarm(sa_audio::AlarmId::StructuralDamage);
                    }
                    landing::ImpactCategory::Destroyed => {
                        self.audio.play_sfx(sa_audio::SfxId::ImpactExplosion, None);
                    }
                }
                self.events.emit(impact.clone());
            }
        }

        // Step 4: Carry player with ship.
        // The player's world position is relative to the OLD ship position.
        // Transform to local space (using old ship), then back to world (using new ship).
        // This is an instant teleport — no collision sweep, no AABB cost.
        // For a non-rotating ship: equivalent to adding ship displacement.
        // For a rotating ship: also rotates the player around the ship origin.
        let ship_rot_before_inv = ship_rot_before.inverse();
        if let Some(player) = &self.player
            && let Some(body) = self.physics.get_body_mut(player.body_handle)
        {
            let p = body.translation().clone_owned();
            let local = ship_rot_before_inv * (p - ship_pos_before);
            let carried = ship_pos_after + ship_rot_after * local;
            body.set_translation(carried, true);
        }

        // Step 5: Sync interior collider ROTATION to match ship.
        // Interior colliders are on a fixed body at world origin.
        // We sync rotation (not position) so collision geometry matches
        // the ship's visual orientation after roll/pitch/yaw.
        // Position stays at origin — high-speed translation is handled by
        // the player controller's origin-offset transform.
        if let (Some(ship), Some(ih)) = (&self.ship, ship_colliders::interior_body_handle()) {
            // Read ship rotation first (immutable borrow)
            let ship_rot = self.physics.get_body(ship.body_handle)
                .map(|b| *b.rotation());
            // Then set interior rotation (mutable borrow)
            if let (Some(rot), Some(ib)) = (ship_rot, self.physics.get_body_mut(ih)) {
                ib.set_position(
                    nalgebra::Isometry3::from_parts(
                        nalgebra::Translation3::identity(),
                        rot,
                    ),
                    true,
                );
            }
        }

        // Sync child collider world positions + update query pipeline.
        self.physics.sync_collider_positions();
        let t_qp = Instant::now();
        self.physics.update_query_pipeline();
        let qp_us = t_qp.elapsed().as_micros();

        log::trace!("phys_step: {}us, query_pipeline: {}us", phys_step_us, qp_us);

        // Step 6: Move player in ship-local space.
        // Interior collision uses origin-centered sweep (GROUP_2).
        // When terrain is active, a second sweep at world position
        // checks GROUP_5 (TERRAIN) so the player doesn't fall through.
        let t_player = Instant::now();
        if let Some(player) = &mut self.player {
            let terrain_up = self.terrain_gravity.as_ref().map(|g| {
                nalgebra::Vector3::new(-g.direction[0], -g.direction[1], -g.direction[2])
            });
            if let Some(up) = terrain_up {
                player.update_with_terrain(
                    &mut self.physics,
                    &self.input,
                    physics_dt,
                    ship_pos_after,
                    ship_rot_after,
                    up,
                );
            } else {
                player.update(
                    &mut self.physics,
                    &self.input,
                    physics_dt,
                    ship_pos_after,
                    ship_rot_after,
                );
            }
        }
        let player_move_us = t_player.elapsed().as_micros();
        // Encode breakdown: physics_us = phys_step*1000000 + qp*1000 + move_shape
        // Read as: phys_step=X.XXms, qp=X.XXms, move=X.XXms
        self.perf.physics_us = phys_step_us as u64 * 1000 + qp_us as u64;
        self.perf.stars_us = player_move_us as u64;

        if let Some(player) = &self.player {
            // Eye position: offset along ship's up (correct after roll/pitch)
            self.camera.position = player.position_ship_up(&self.physics, ship_rot_after);
            // Camera: ship quaternion * (player look offset from ship heading).
            // Player.yaw is WORLD space. Subtract ship's world yaw to get
            // the ship-relative look offset. This way the camera follows
            // the ship's roll while showing the correct look direction.
            if let Some(ship) = &self.ship {
                if let Some(body) = self.physics.get_body(ship.body_handle) {
                    let r = body.rotation();
                    let ship_quat = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                    let fwd = r * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                    let ship_yaw = fwd.x.atan2(-fwd.z);
                    // Offset from ship heading
                    let yaw_offset = player.yaw - ship_yaw;
                    let look = glam::Quat::from_rotation_y(-yaw_offset)
                        * glam::Quat::from_rotation_x(player.pitch);
                    self.camera.orientation_override = Some(ship_quat * look);
                }
            } else {
                self.camera.yaw = player.yaw;
                self.camera.pitch = player.pitch;
                self.camera.orientation_override = None;
            }
        }
    }
}
