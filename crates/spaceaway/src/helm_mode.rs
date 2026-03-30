use super::App;
use crate::drive_integration;
use crate::landing;
use crate::navigation;
use crate::ship_colliders;
use crate::solar_system;
use sa_math::WorldPos;
use sa_universe::MasterSeed;
use winit::keyboard::KeyCode;

impl App {
    /// Update physics and camera for the seated helm mode.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn update_helm_mode(&mut self, dt: f32) {
        if let Some(ship) = &self.ship {
            // Reset forces each frame so only current input applies
            ship.reset_forces(&mut self.physics);

            let wants_stand = if let Some(helm) = &self.helm {
                helm.update_seated(ship, &mut self.physics, &self.input, dt)
            } else {
                false
            };

            // Drive mode selection (1/2/3) while seated at helm
            if self.input.keyboard.just_pressed(KeyCode::Digit1) {
                self.drive.request_disengage();
                log::info!("Drive: IMPULSE");
            }
            if self.input.keyboard.just_pressed(KeyCode::Digit2) {
                // Block cruise inside the atmosphere (blend > 0.1).
                // Above atmosphere with terrain active: cruise is allowed
                // for approach but auto-disengages at atmosphere boundary.
                let in_atmosphere = self.terrain_gravity
                    .as_ref()
                    .is_some_and(|g| g.blend > 0.1);
                if in_atmosphere {
                    log::warn!("Cannot engage cruise: inside atmosphere");
                } else {
                    let ship_speed = self.ship.as_ref()
                        .map(|s| s.speed(&self.physics))
                        .unwrap_or(0.0);
                    if self.drive.request_engage_with_speed(sa_ship::DriveMode::Cruise, ship_speed) {
                        log::info!("Drive: CRUISE engaged");
                    } else if ship_speed > 100.0 {
                        log::warn!("Cannot engage cruise: ship moving at {:.0} m/s (slow down first)", ship_speed);
                    }
                }
            }

            // Tab: lock nearest target to crosshair.
            // In a solar system: lock nearest PLANET. Otherwise: lock nearest STAR.
            if self.input.keyboard.just_pressed(KeyCode::Tab)
                && let Some(gpu) = &self.gpu {
                    let sw = gpu.config.width as f32;
                    let sh = gpu.config.height as f32;
                    let cx = sw / 2.0;
                    let cy = sh / 2.0;
                    let aspect = sw / sh;
                    let vp = self.camera.view_projection_matrix(aspect);
                    let margin = 40.0_f32;

                    let locked = if let Some(sys) = &self.active_system {
                        // In a solar system — lock nearest planet to crosshair
                        let positions = sys.compute_positions_ly_pub();
                        let mut best_dist = f32::MAX;
                        let mut best: Option<(usize, f32)> = None;

                        for (i, pos) in positions.iter().enumerate() {
                            if i == 0 { continue; } // skip star
                            let Some(radius_m) = sys.body_radius_m(i) else { continue };
                            let Some((_, _sub_type, _, _, _)) = sys.planet_data(i) else { continue };
                            // Skip non-planets (atmosphere, rings)
                            if radius_m < 100_000.0 { continue; }

                            let dx = (pos.x - self.galactic_position.x) as f32;
                            let dy = (pos.y - self.galactic_position.y) as f32;
                            let dz = (pos.z - self.galactic_position.z) as f32;
                            let len = (dx*dx + dy*dy + dz*dz).sqrt();
                            if len < 1e-10 { continue; }
                            let dir_norm = glam::Vec3::new(dx/len, dy/len, dz/len);
                            let dir = dir_norm * 90000.0;
                            let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                            if clip.w <= 0.0 { continue; }
                            let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                            let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                            // Only consider planets within screen bounds + margin
                            if sx < -margin || sx > sw + margin
                                || sy < -margin || sy > sh + margin { continue; }
                            let sd = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                            if sd < best_dist {
                                best_dist = sd;
                                best = Some((i, sd));
                            }
                        }

                        if let Some((idx, px_dist)) = best {
                            let pos = positions[idx];
                            let dist_ly = self.galactic_position.distance_to(pos);
                            let radius_m = sys.body_radius_m(idx).unwrap_or(0.0);
                            let (_, sub_type, _, _, _) = sys.planet_data(idx).unwrap();
                            let name = format!("Planet {} ({:?}, {:.0}km)",
                                idx, sub_type, radius_m / 1000.0);
                            let nav = navigation::NavStar {
                                id: sa_universe::ObjectId(0),
                                galactic_pos: pos,
                                catalog_name: name.clone(),
                                distance_ly: dist_ly,
                                color: [1.0, 1.0, 1.0],
                                spectral_class: sa_universe::SpectralClass::G,
                                luminosity: 1.0,
                            };
                            self.navigation.lock_star(nav);
                            log::info!("LOCKED PLANET: {} ({:.6} ly, {:.0}px from center)",
                                name, dist_ly, px_dist);
                            self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // Fallback: lock nearest star if no planet was locked.
                    // Search ALL rendered stars (not just 15 nearest nav stars).
                    if !locked {
                        let visible = self.star_streaming.visible_stars(self.galactic_position);
                        let mut best_screen_dist = f32::MAX;
                        let mut best_star: Option<(usize, f32)> = None;
                        for (i, (placed, _dist_ly)) in visible.iter().enumerate() {
                            let dx = (placed.position.x - self.galactic_position.x) as f32;
                            let dy = (placed.position.y - self.galactic_position.y) as f32;
                            let dz = (placed.position.z - self.galactic_position.z) as f32;
                            let len = (dx*dx + dy*dy + dz*dz).sqrt();
                            if len < 0.001 { continue; }
                            let dir_norm = glam::Vec3::new(dx/len, dy/len, dz/len);
                            let dir = dir_norm * 90000.0;
                            let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                            if clip.w <= 0.0 { continue; }
                            let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                            let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                            if sx < -margin || sx > sw + margin
                                || sy < -margin || sy > sh + margin { continue; }
                            let screen_dist = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                            if screen_dist < best_screen_dist {
                                best_screen_dist = screen_dist;
                                best_star = Some((i, screen_dist));
                            }
                        }
                        if let Some((idx, px_dist)) = best_star {
                            let (placed, dist_ly) = &visible[idx];
                            let nav = navigation::NavStar {
                                id: placed.id,
                                galactic_pos: placed.position,
                                catalog_name: navigation::catalog_name_from_id(placed.id),
                                distance_ly: *dist_ly,
                                color: placed.star.color,
                                spectral_class: placed.star.spectral_class,
                                luminosity: placed.star.luminosity,
                            };
                            log::info!("LOCKED STAR: {} ({:.2} ly, {:.0}px from center)",
                                nav.catalog_name, nav.distance_ly, px_dist);
                            self.navigation.lock_star(nav);
                            self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                        } else {
                            log::warn!("No target visible on screen");
                        }
                    }
            }
            if self.input.keyboard.just_pressed(KeyCode::Digit3) {
                // Block warp near planets (always — warp is for star-to-star).
                let in_atmosphere = self.terrain_gravity
                    .as_ref()
                    .is_some_and(|g| g.blend > 0.5);
                if self.terrain.is_some() || in_atmosphere {
                    log::warn!("Cannot engage warp: too close to planet");
                } else if self.ship_resources.exotic_fuel > 0.0 {
                    let ship_speed = self.ship.as_ref()
                        .map(|s| s.speed(&self.physics))
                        .unwrap_or(0.0);
                    if self.drive.request_engage_with_speed(sa_ship::DriveMode::Warp, ship_speed) {
                        log::info!("Drive: WARP spooling...");
                        self.audio.announce(sa_audio::VoiceId::EngagingWarp);
                        // Unload active system when entering warp
                        if self.active_system.is_some() {
                            if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                            self.terrain = None;
                            self.terrain_gravity = None;
                            self.active_system = None;
                            log::info!("Left solar system — entering warp");
                        }
                    } else if ship_speed > 10.0 {
                        log::warn!("Cannot engage warp: ship moving at {:.0} m/s (stop first)", ship_speed);
                    }
                } else {
                    log::warn!("Cannot engage warp: no exotic fuel");
                }
            }

            // Update drive spool progress
            self.drive.update(dt);

            // Map throttle lever to drive speed when in cruise/warp
            if self.drive.mode() != sa_ship::DriveMode::Impulse
                && let Some(ship) = &self.ship
            {
                self.drive.set_speed_fraction(ship.throttle);
            }

            // Apply continuous thrust from throttle lever + engine button
            ship.apply_thrust(&mut self.physics);

            // Vertical thrust: Space = up, Shift = down (ship-local).
            // 2× main thrust so the pilot can decelerate from
            // terminal velocity (~270 m/s) against full gravity.
            let vert_up = self.input.keyboard.is_pressed(KeyCode::Space);
            let vert_down = self.input.keyboard.is_pressed(KeyCode::ShiftLeft);
            if vert_up || vert_down {
                let sign = if vert_up { 1.0_f32 } else { -1.0 };
                if let Some(body) = self.physics.get_body(ship.body_handle) {
                    let rot = *body.rotation();
                    let up = rot * nalgebra::Vector3::new(0.0, sign, 0.0);
                    let force = up * ship.max_thrust * 2.0;
                    if let Some(body) = self.physics.get_body_mut(ship.body_handle) {
                        body.add_force(force, true);
                    }
                }
            }

            if wants_stand
                && let Some(helm) = &mut self.helm
            {
                self.drive.request_disengage();
                // Set player yaw to ship's WORLD heading so they face the
                // ship's nose direction. Player.yaw is WORLD space.
                if let Some(body) = self.physics.get_body(ship.body_handle) {
                    let rot = body.rotation();
                    let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                    if let Some(player) = &mut self.player {
                        player.yaw = fwd.x.atan2(-fwd.z);
                        player.pitch = 0.0;
                    }
                }
                helm.stand_up();
                // Re-enable player and teleport to ship's current position
                // (ship may have moved while seated)
                if let Some(player) = &self.player {
                    // Get ship state before mutable borrow
                    let ship_pos = ship.position(&self.physics);
                    let ship_vel = ship.speed_vector(&self.physics);
                    // Compute rotated stand-up offset before mutable borrow
                    let ship_rot = self.physics.get_body(ship.body_handle)
                        .map(|b| *b.rotation())
                        .unwrap_or(nalgebra::UnitQuaternion::identity());
                    let local_offset = nalgebra::Vector3::new(0.0, -0.1, 2.8);
                    let world_offset = ship_rot * local_offset;

                    if let (Some((sx, sy, sz)), Some(body)) = (
                        ship_pos,
                        self.physics.get_body_mut(player.body_handle),
                    ) {
                        body.set_enabled(true);
                        body.set_translation(
                            nalgebra::Vector3::new(sx + world_offset.x, sy + world_offset.y, sz + world_offset.z),
                            true,
                        );
                        // Match ship velocity so player doesn't slide on stand-up
                        body.set_linvel(nalgebra::Vector3::new(ship_vel.0, ship_vel.1, ship_vel.2), true);
                    }
                }
                log::info!("Left helm seated mode — player teleported to ship");
            }
        }

        // Apply terrain gravity + atmospheric drag to ship body before step.
        // Skip when LANDED — ship is frozen on the surface.
        if self.landing.state() != landing::LandingState::Landed
            && let Some(ref grav) = self.terrain_gravity
            && grav.blend > 0.01
            && let Some(ship) = &self.ship
            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
        {
            let helm_dt = dt.min(1.0 / 30.0);
            let grav_accel = nalgebra::Vector3::new(
                grav.direction[0] * grav.magnitude,
                grav.direction[1] * grav.magnitude,
                grav.direction[2] * grav.magnitude,
            );
            let vel = body.linvel() + grav_accel * helm_dt;
            // Atmospheric drag: terminal velocity ~200 m/s at surface
            let atmo_drag = 0.05 * grav.blend;
            let vel = vel * (1.0 - atmo_drag * helm_dt).max(0.0);
            body.set_linvel(vel, true);
        }

        // Record ship position before physics step (for galactic tracking).
        let ship_pos_pre_helm = self.ship.as_ref()
            .and_then(|s| self.physics.get_body(s.body_handle))
            .map(|b| b.translation().clone_owned());

        // Physics step
        let physics_dt = dt.min(1.0 / 30.0);
        if physics_dt > 0.0 {
            profiling::scope!("physics_step");
            self.physics.step(physics_dt);
        }

        // Track ship physics into galactic_position (impulse only).
        // When terrain is active, compute exactly from anchor + ship rapier pos.
        // When terrain is NOT active, use delta-based tracking.
        if self.drive.mode() == sa_ship::DriveMode::Impulse {
            if let Some(terrain_mgr) = &self.terrain
                && let Some(ship) = &self.ship
                && let Some(body) = self.physics.get_body(ship.body_handle)
            {
                // Exact galactic position: frozen_planet_center + (anchor + ship_rapier) / LY_TO_M
                let anchor = terrain_mgr.anchor_f64();
                let post = body.translation();
                let planet = terrain_mgr.frozen_planet_center_ly();
                let ly_to_m = 9.461e15_f64;
                self.galactic_position.x = planet.x + (anchor[0] + post.x as f64) / ly_to_m;
                self.galactic_position.y = planet.y + (anchor[1] + post.y as f64) / ly_to_m;
                self.galactic_position.z = planet.z + (anchor[2] + post.z as f64) / ly_to_m;
            } else if let Some(pre) = ship_pos_pre_helm
                && let Some(ship) = &self.ship
                && let Some(body) = self.physics.get_body(ship.body_handle)
            {
                let post = body.translation();
                let m_to_ly = 1.0 / 9.461e15_f64;
                self.galactic_position.x += (post.x as f64 - pre.x as f64) * m_to_ly;
                self.galactic_position.y += (post.y as f64 - pre.y as f64) * m_to_ly;
                self.galactic_position.z += (post.z as f64 - pre.z as f64) * m_to_ly;
            }
        }

        // Auto-orient ship toward locked target in cruise/warp.
        // Smoothly slerps the ship body rotation so its forward (-Z)
        // aligns with the direction to the target.
        if self.drive.mode() != sa_ship::DriveMode::Impulse
            && let Some(target) = &self.navigation.locked_target
            && let Some(ship) = &self.ship
            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
        {
            // Compute direction in f64 to handle both star targets (ly scale)
            // and planet targets (AU scale = ~1e-5 ly). The old f32 threshold
            // of 0.001 ly filtered out all in-system planets.
            let dx_f64 = target.galactic_pos.x - self.galactic_position.x;
            let dy_f64 = target.galactic_pos.y - self.galactic_position.y;
            let dz_f64 = target.galactic_pos.z - self.galactic_position.z;
            let len_f64 = (dx_f64 * dx_f64 + dy_f64 * dy_f64 + dz_f64 * dz_f64).sqrt();
            if len_f64 > 1e-15 { // ~9.5 meters in ly — practically any nonzero distance
                let to_target = glam::Vec3::new(
                    (dx_f64 / len_f64) as f32,
                    (dy_f64 / len_f64) as f32,
                    (dz_f64 / len_f64) as f32,
                );
                // Ship forward is -Z, so desired rotation takes -Z to to_target
                let desired = glam::Quat::from_rotation_arc(
                    glam::Vec3::new(0.0, 0.0, -1.0),
                    to_target,
                );
                let current_rot = body.rotation();
                let current = glam::Quat::from_xyzw(
                    current_rot.i, current_rot.j, current_rot.k, current_rot.w,
                );
                // Fast slerp: 95% of the way per second (exponential smoothing)
                let t = (1.0 - (-3.0 * dt).exp()).clamp(0.0, 1.0);
                let new_rot = current.slerp(desired, t).normalize();
                let q = nalgebra::UnitQuaternion::from_quaternion(
                    nalgebra::Quaternion::new(new_rot.w, new_rot.x, new_rot.y, new_rot.z),
                );
                body.set_rotation(q, true);
                // Zero angular velocity so manual steering doesn't fight
                body.set_angvel(nalgebra::Vector3::zeros(), true);
            }
        }

        // Update galactic position based on drive speed (with deceleration)
        let was_ftl = self.drive.mode() != sa_ship::DriveMode::Impulse;
        if self.drive.mode() != sa_ship::DriveMode::Impulse {
            let direction = if let Some(ship) = &self.ship {
                if let Some(body) = self.physics.get_body(ship.body_handle) {
                    let rot = body.rotation();
                    let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                    [fwd.x as f64, fwd.y as f64, fwd.z as f64]
                } else {
                    [0.0, 0.0, -1.0]
                }
            } else {
                [0.0, 0.0, -1.0]
            };

            // Save position before warp movement
            let pos_before = self.galactic_position;

            // Compute target distance for deceleration.
            // In cruise mode, check ALL planets in the active system (not just
            // terrain-active ones) so deceleration starts before terrain activates.
            // At 5000c the ship can cross an entire terrain zone in one frame.
            let target_dist = if self.drive.mode() == sa_ship::DriveMode::Cruise {
                let planet_alt_ly = self.nearest_planet_altitude_ly();
                if let Some(alt) = planet_alt_ly {
                    Some(alt)
                } else {
                    self.navigation.locked_target.as_ref()
                        .map(|t| self.galactic_position.distance_to(t.galactic_pos))
                }
            } else {
                self.navigation.locked_target.as_ref()
                    .map(|t| self.galactic_position.distance_to(t.galactic_pos))
            };

            // Apply warp/cruise movement with deceleration
            let (mut delta, effective_speed) = drive_integration::galactic_position_delta_decel(
                &self.drive,
                direction,
                dt as f64,
                target_dist,
            );

            // Cruise planet flythrough prevention: before moving, check if
            // the delta would cross any planet's 100km boundary. At 5000c
            // one frame = 25M km — post-hoc clamping doesn't work because
            // the ship ends up on the far side of the planet.
            if self.drive.mode() == sa_ship::DriveMode::Cruise
                && let Some((planet_pos, planet_radius_m)) = self.nearest_planet_info()
            {
                let ly_to_m = 9.461e15_f64;
                let atmo_boundary_m = planet_radius_m + 100_000.0;
                let atmo_boundary_ly = atmo_boundary_m / ly_to_m;

                // Current distance from planet center
                let dx = self.galactic_position.x - planet_pos.x;
                let dy = self.galactic_position.y - planet_pos.y;
                let dz = self.galactic_position.z - planet_pos.z;
                let dist_before = (dx * dx + dy * dy + dz * dz).sqrt();

                // Distance after proposed move
                let nx = self.galactic_position.x + delta[0] - planet_pos.x;
                let ny = self.galactic_position.y + delta[1] - planet_pos.y;
                let nz = self.galactic_position.z + delta[2] - planet_pos.z;
                let dist_after = (nx * nx + ny * ny + nz * nz).sqrt();

                // If we'd cross the boundary (or closest approach is inside)
                if dist_before > atmo_boundary_ly && dist_after < atmo_boundary_ly {
                    // Scale delta so we stop at exactly the boundary
                    // Binary search for the fraction t where dist = boundary
                    let mut lo = 0.0_f64;
                    let mut hi = 1.0_f64;
                    for _ in 0..20 {
                        let mid = (lo + hi) * 0.5;
                        let mx = self.galactic_position.x + delta[0] * mid - planet_pos.x;
                        let my = self.galactic_position.y + delta[1] * mid - planet_pos.y;
                        let mz = self.galactic_position.z + delta[2] * mid - planet_pos.z;
                        let md = (mx * mx + my * my + mz * mz).sqrt();
                        if md < atmo_boundary_ly { hi = mid; } else { lo = mid; }
                    }
                    delta[0] *= lo;
                    delta[1] *= lo;
                    delta[2] *= lo;
                    // Apply truncated delta then disengage
                    self.galactic_position.x += delta[0];
                    self.galactic_position.y += delta[1];
                    self.galactic_position.z += delta[2];
                    self.drive.request_disengage();
                    if let Some(terrain_mgr) = &mut self.terrain
                        && let Some(renderer) = &mut self.renderer
                    {
                        terrain_mgr.flush_for_teleport(&mut renderer.terrain_slab);
                    }
                    log::info!("Cruise auto-disengage: 100km above planet (flythrough prevented)");
                } else if dist_after < atmo_boundary_ly && dist_before < atmo_boundary_ly {
                    // Already inside boundary — clamp to boundary
                    let dist_m = dist_before * ly_to_m;
                    if dist_m > 1.0 {
                        let scale = atmo_boundary_m / dist_m;
                        self.galactic_position.x = planet_pos.x + dx * scale;
                        self.galactic_position.y = planet_pos.y + dy * scale;
                        self.galactic_position.z = planet_pos.z + dz * scale;
                    }
                    self.drive.request_disengage();
                    log::info!("Cruise auto-disengage: clamped to 100km boundary");
                } else {
                    // Normal movement — not crossing boundary
                    self.galactic_position.x += delta[0];
                    self.galactic_position.y += delta[1];
                    self.galactic_position.z += delta[2];
                }
            } else {
                self.galactic_position.x += delta[0];
                self.galactic_position.y += delta[1];
                self.galactic_position.z += delta[2];
            }

            // Sync rapier body AFTER planet clamp so it never ends up inside.
            if let Some(terrain_mgr) = &self.terrain
                && let Some(ship) = &self.ship
                && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
            {
                let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                let anchor = terrain_mgr.anchor_f64();
                let new_rapier = [
                    (cam_rel[0] - anchor[0]) as f32,
                    (cam_rel[1] - anchor[1]) as f32,
                    (cam_rel[2] - anchor[2]) as f32,
                ];
                body.set_translation(
                    nalgebra::Vector3::new(new_rapier[0], new_rapier[1], new_rapier[2]),
                    true,
                );
            }

            let pos_after = self.galactic_position;

            // Approach voice + cascade auto-disengage
            if let Some(dist) = target_dist {
                let prev = self.prev_target_dist;

                // Voice: "Approaching destination" when deceleration begins (~1 ly)
                if dist < 1.0 && prev.is_none_or(|d| d >= 1.0)
                    && self.drive.mode() == sa_ship::DriveMode::Warp
                {
                    self.audio.announce(sa_audio::VoiceId::ApproachingDestination);
                    log::info!("Approaching destination — warp deceleration engaged");
                }

                // Warp → Impulse: auto-disengage at 0.01 ly
                // Player can manually engage cruise to continue approach.
                if self.drive.mode() == sa_ship::DriveMode::Warp
                    && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                    && dist < drive_integration::WARP_DISENGAGE_LY
                {
                    self.drive.request_disengage();
                    self.audio.announce(sa_audio::VoiceId::AllSystemsReady);
                    log::info!("Warp disengaged at {:.4} ly ({:.0} AU) from target",
                        dist, dist / 1.581e-5);
                }

                // Cruise → Impulse: auto-disengage near locked star target.
                // Skip when near a planet — the planet boundary handler
                // (above) manages cruise disengage at 100km from surface.
                if self.drive.mode() == sa_ship::DriveMode::Cruise
                    && dist < drive_integration::CRUISE_DISENGAGE_LY
                    && self.nearest_planet_info().is_none()
                {
                    self.drive.request_disengage();
                    log::info!("Cruise disengaged at {:.6} ly ({:.0} AU) from target",
                        dist, dist / 1.581e-5);
                }

                self.prev_target_dist = Some(dist);
            }

            // Free warp proximity warning (3 seconds lookahead)
            if self.drive.mode() == sa_ship::DriveMode::Warp
                && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                && self.navigation.locked_target.is_none()
                && !self.proximity_warned
            {
                let lookahead = effective_speed * 3.0;
                if self.navigation.check_proximity_warning(
                    self.galactic_position, direction, lookahead,
                ).is_some() {
                    self.audio.announce(sa_audio::VoiceId::Alert);
                    self.proximity_warned = true;
                    log::info!("PROXIMITY ALERT — star ahead");
                }
            }

            // Predictive gravity well auto-drop (ray-segment, catches flythroughs)
            if self.drive.mode() == sa_ship::DriveMode::Warp
                && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                && self.active_system.is_none()
                && let Some((nav_star, drop_pos)) =
                    self.navigation.check_gravity_well_predictive(pos_before, pos_after)
            {
                    // Place ship at well boundary, not inside
                    self.galactic_position = drop_pos;
                    self.drive.request_disengage();
                    self.proximity_warned = false;
                    self.prev_target_dist = None;
                    log::info!("GRAVITY WELL — entering system: {}", nav_star.catalog_name);

                    // Load the solar system
                    let nav_id = nav_star.id;
                    let nav_name = nav_star.catalog_name.clone();
                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                        let sector_coord = sa_universe::SectorCoord::new(
                            nav_id.sector_x().into(),
                            nav_id.sector_y().into(),
                            nav_id.sector_z().into(),
                        );
                        let sector = sa_universe::sector::generate_sector(
                            MasterSeed(42),
                            sector_coord,
                        );
                        if let Some(placed) = sector.stars.iter().find(|s| s.id == nav_id) {
                            let system = solar_system::ActiveSystem::load(
                                nav_id,
                                placed,
                                &mut renderer.mesh_store,
                                &gpu.device,
                            );
                            log::info!("System loaded: {} bodies ({})", system.body_count(), nav_name);
                            self.audio.announce(sa_audio::VoiceId::AllSystemsReady);
                            // Clean up terrain before switching system
                            if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                            self.terrain = None;
                            self.terrain_gravity = None;
                            self.active_system = Some(system);
                        }
                    }

                    // Keep the navigation lock — marker stays visible
                    // after arrival. Gravity well won't re-trigger
                    // because it's guarded by active_system.is_none().
            }
        } else {
            // Not in cruise/warp — reset proximity warning and approach tracking
            self.proximity_warned = false;
            self.prev_target_dist = None;
        }

        // Zero ship velocity when drive disengages (cruise/warp → impulse).
        // Without this the ship retains its last rapier velocity and drifts.
        if was_ftl && self.drive.mode() == sa_ship::DriveMode::Impulse
            && let Some(ship) = &self.ship
            && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
        {
            body.set_linvel(nalgebra::Vector3::zeros(), true);
            body.set_angvel(nalgebra::Vector3::zeros(), true);
        }

        // Sync interior collider rotation to match ship (same as walk mode).
        // Needed so colliders are correct when the player stands up.
        if let (Some(ship_ref), Some(ih)) = (&self.ship, ship_colliders::interior_body_handle()) {
            let ship_rot = self.physics.get_body(ship_ref.body_handle)
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

        // --- Landing system update (helm mode) ---
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
                // Ship gravity_scale is always 0 (gravity applied manually).
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

        // Camera: ship quaternion + helm mouse look offset.
        // ONLY runs while still seated — must not run on the stand-up frame
        // or orientation_override gets re-set after being cleared.
        if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
            let (dx, dy) = self.input.mouse.delta();
            self.helm_look_yaw += dx * 0.003;
            self.helm_look_pitch -= dy * 0.003;
            let max_p = std::f32::consts::FRAC_PI_2 - 0.01;
            self.helm_look_pitch = self.helm_look_pitch.clamp(-max_p, max_p);

            if let Some(ship) = &self.ship
                && let Some(body) = self.physics.get_body(ship.body_handle)
            {
                let r = body.rotation();
                let ship_quat = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);

                let look_offset = glam::Quat::from_rotation_y(-self.helm_look_yaw)
                    * glam::Quat::from_rotation_x(self.helm_look_pitch);

                self.camera.orientation_override = Some(ship_quat * look_offset);
            }
        }

        // Camera position fixed at helm viewpoint (moves with ship)
        if let (Some(helm), Some(ship)) = (&self.helm, &self.ship)
            && let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship)
        {
            self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
        }
    }

    /// Find the nearest planet's altitude in light-years (for cruise deceleration).
    /// Works with the active solar system directly — no terrain required.
    fn nearest_planet_altitude_ly(&self) -> Option<f64> {
        let (planet_pos, radius_m) = self.nearest_planet_info()?;
        let ly_to_m = 9.461e15_f64;
        let dx = (self.galactic_position.x - planet_pos.x) * ly_to_m;
        let dy = (self.galactic_position.y - planet_pos.y) * ly_to_m;
        let dz = (self.galactic_position.z - planet_pos.z) * ly_to_m;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        let alt = dist - radius_m;
        Some(alt / ly_to_m)
    }

    /// Find the nearest planet's position and radius.
    fn nearest_planet_info(&self) -> Option<(sa_math::WorldPos, f64)> {
        let sys = self.active_system.as_ref()?;
        let ly_to_m = 9.461e15_f64;
        let positions = sys.compute_positions_ly_pub();
        let mut best: Option<(sa_math::WorldPos, f64, f64)> = None;
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
            if best.is_none_or(|(_, _, d)| dist < d) {
                best = Some((*pos, r, dist));
            }
        }
        best.map(|(pos, r, _)| (pos, r))
    }
}
