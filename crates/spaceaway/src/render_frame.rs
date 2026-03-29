use super::App;
use crate::terrain_colliders;
use crate::terrain_integration;
use crate::ui;
use glam::{Mat4, Vec3};
use sa_math::WorldPos;
use sa_render::{DrawCommand, ScreenDrawCommand};
use std::time::Instant;

impl App {
    /// Render the current frame: terrain streaming, draw commands, HUD, present.
    pub(super) fn render_playing_frame(&mut self, dt: f32, frame_start: Instant) {
        // --- Render ---
        profiling::scope!("render");
        let t3 = Instant::now();

        // --- Terrain streaming (before immutable renderer borrow) ---
        profiling::scope!("terrain_update");
        // Deactivation check
        // Check deactivation separately to avoid borrow issues
        let should_deactivate_terrain = self.terrain.as_ref()
            .is_some_and(|t| t.should_deactivate(self.galactic_position));
        if should_deactivate_terrain {
            if let Some(sys) = &mut self.active_system {
                sys.hidden_body_index = None;
            }
            if let Some(mut t) = self.terrain.take() {
                t.cleanup(&mut self.physics);
            }
            self.terrain_gravity = None;
            log::info!("Terrain deactivated");
        }

        // Diagnostic: log planet distances every 60 frames to debug terrain activation
        if let Some(sys) = &self.active_system
            && self.time.frame_count().is_multiple_of(60)
        {
            let positions = sys.compute_positions_ly_pub();
            let ly_to_m = 9.461e15_f64;
            for (i, pos) in positions.iter().enumerate() {
                if let Some(r_m) = sys.body_radius_m(i)
                    && sys.planet_data(i).is_some()
                {
                    let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                    let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                    let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                    let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
                    let ratio = dist_m / r_m;
                    log::info!(
                        "DIAG body[{}]: dist={:.0}km, radius={:.0}km, ratio={:.2}x, \
                         terrain={}, hidden={:?}, galactic=({:.12},{:.12},{:.12}), \
                         planet=({:.12},{:.12},{:.12})",
                        i, dist_m / 1000.0, r_m / 1000.0, ratio,
                        self.terrain.is_some(), sys.hidden_body_index,
                        self.galactic_position.x, self.galactic_position.y, self.galactic_position.z,
                        pos.x, pos.y, pos.z,
                    );
                }
            }
        }

        // Activation check (only if no terrain active and in a solar system)
        if self.terrain.is_none()
            && let Some(sys) = &self.active_system
            && let Some((body_idx, planet_pos, config, surface_grav)) =
                terrain_integration::find_terrain_planet(sys, self.galactic_position)
        {
            log::info!(
                "Terrain activated for body {} (radius {:.0} km, g={:.2} m/s2, type={:?}, seed={})",
                body_idx,
                config.radius_m / 1000.0,
                surface_grav,
                config.sub_type,
                config.noise_seed,
            );
            self.terrain = Some(terrain_integration::TerrainManager::new(
                config, planet_pos, body_idx, surface_grav,
            ));

            // Unified physics origin rebase: on terrain activation,
            // shift all physics bodies so the ship is at the rapier origin.
            // This ensures terrain colliders (within ~600m) and the sphere
            // barrier are at reasonable f32 distances from the ship.
            if let Some(ship) = &self.ship
                && let Some(body) = self.physics.rigid_body_set.get(ship.body_handle)
            {
                let ship_pos = body.translation().clone_owned();
                let shift = -ship_pos;
                // Shift ship body
                if let Some(b) = self.physics.rigid_body_set.get_mut(ship.body_handle) {
                    let t = b.translation();
                    b.set_translation(
                        nalgebra::Vector3::new(t.x + shift.x, t.y + shift.y, t.z + shift.z),
                        true,
                    );
                }
                // Shift player body
                if let Some(player) = &self.player
                    && let Some(b) = self.physics.rigid_body_set.get_mut(player.body_handle)
                {
                    let t = b.translation();
                    b.set_translation(
                        nalgebra::Vector3::new(t.x + shift.x, t.y + shift.y, t.z + shift.z),
                        true,
                    );
                }
                // Interior body stays at origin (player controller handles offset).
                log::info!("Physics origin rebase on terrain activation: shifted by ({:.0},{:.0},{:.0})",
                    shift.x, shift.y, shift.z);
            }

            // Auto-disengage cruise/warp on terrain activation.
            // Without this, cruise speed (~55,000 km/s) overshoots the
            // planet surface in a fraction of a second, landing the camera
            // deep inside where only LOD 0 chunks are visible.
            if self.drive.mode() != sa_ship::DriveMode::Impulse {
                log::info!("Auto-disengage drive: terrain activation");
                self.drive.request_disengage();
            }
        }

        // Deactivate terrain if no solar system is active
        if self.active_system.is_none()
            && let Some(terrain_mgr) = &mut self.terrain
        {
            terrain_mgr.cleanup(&mut self.physics);
            self.terrain = None;
            self.terrain_gravity = None;
            log::info!("Terrain deactivated (no solar system)");
        }

        // Update terrain and collect draw commands (needs &mut renderer + &mut physics)
        // Ship "down" direction for gravity blending.
        let ship_down = self.ship.as_ref()
            .and_then(|s| self.physics.get_body(s.body_handle))
            .map(|body| {
                let down = body.rotation() * nalgebra::Vector3::new(0.0, -1.0, 0.0);
                [down.x, down.y, down.z]
            })
            .unwrap_or([0.0, -1.0, 0.0]);

        let terrain_commands: Vec<DrawCommand> = if let Some(terrain_mgr) = &mut self.terrain {
            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                let planet_pos = self.active_system.as_ref()
                    .and_then(|sys| {
                        let positions = sys.compute_positions_ly_pub();
                        positions.get(terrain_mgr.body_index()).copied()
                    })
                    .unwrap_or(WorldPos::ORIGIN);
                let rebase_bodies = terrain_colliders::RebaseBodies {
                    ship: self.ship.as_ref().map(|s| s.body_handle),
                    player: self.player.as_ref().map(|p| p.body_handle),
                };
                // Build VP matrix in planet-relative coordinates for frustum culling.
                let aspect = gpu.config.width as f32 / gpu.config.height as f32;
                let vp_f32 = self.camera.view_projection_matrix(aspect);
                // Convert camera-relative VP to planet-relative VP.
                // The camera's view matrix is in world space (near origin due to rebase).
                // Terrain coords are planet-relative meters: cam_rel_m = (cam_ly - planet_ly) * LY_TO_M.
                // We translate the VP by the camera→planet offset so frustum planes
                // are in the same coordinate system as terrain node centers.
                let planet_ly = terrain_mgr.frozen_planet_center_ly();
                let ly_to_m = 9.461e15_f64;
                let cam_offset = [
                    (self.galactic_position.x - planet_ly.x) * ly_to_m,
                    (self.galactic_position.y - planet_ly.y) * ly_to_m,
                    (self.galactic_position.z - planet_ly.z) * ly_to_m,
                ];
                // Build a translation matrix that shifts world origin to planet center.
                // VP_planet = VP_camera * T(cam_offset)
                // T shifts points from planet-relative → camera-relative.
                let tx = cam_offset[0] as f32;
                let ty = cam_offset[1] as f32;
                let tz = cam_offset[2] as f32;
                let translate = glam::Mat4::from_translation(Vec3::new(tx, ty, tz));
                let vp_planet = vp_f32 * translate;
                let cols = vp_planet.to_cols_array();
                let vp_f64: [f64; 16] = std::array::from_fn(|i| cols[i] as f64);

                let result = terrain_mgr.update(
                    self.galactic_position,
                    planet_pos,
                    &mut renderer.mesh_store,
                    &gpu.device,
                    &mut self.physics,
                    ship_down,
                    &rebase_bodies,
                    Some(vp_f64),
                );
                if let Some(sys) = &mut self.active_system {
                    if sys.hidden_body_index != result.hidden_body_index {
                        log::info!(
                            "Hidden body index: {:?}, system has {} bodies",
                            result.hidden_body_index,
                            sys.body_count(),
                        );
                    }
                    sys.hidden_body_index = result.hidden_body_index;
                }
                self.terrain_gravity = result.gravity;
                result.draw_commands
            } else {
                vec![]
            }
        } else {
            self.terrain_gravity = None;
            vec![]
        };

        // After terrain update (which may rebase physics origin), re-sync
        // the camera position from the ship's current rapier position.
        // Without this, the camera reads the pre-rebase position while the
        // ship mesh uses the post-rebase position → one-frame flicker.
        if self.terrain.is_some() {
            if self.helm.as_ref().is_some_and(|h| h.is_seated()) {
                if let (Some(helm), Some(ship)) = (&self.helm, &self.ship)
                    && let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship)
                {
                    self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
                }
            } else if let Some(player) = &self.player
                && let Some(ship) = &self.ship
                && let Some(body) = self.physics.get_body(ship.body_handle)
            {
                let ship_rot = *body.rotation();
                self.camera.position = player.position_ship_up(&self.physics, ship_rot);
            }
        }

        // APPROACH_DIAG: consolidated approach diagnostic (every 60 frames)
        if self.active_system.is_some()
            && self.time.frame_count().is_multiple_of(60)
        {
            let drive_mode = match self.drive.mode() {
                sa_ship::DriveMode::Impulse => "impulse",
                sa_ship::DriveMode::Cruise => "cruise",
                sa_ship::DriveMode::Warp => "warp",
            };
            let terrain_active = self.terrain.is_some();
            let gravity_blend = self.terrain_gravity
                .as_ref()
                .map(|g| g.blend)
                .unwrap_or(0.0);
            let vertical_speed = self.ship.as_ref()
                .and_then(|s| self.physics.get_body(s.body_handle))
                .map(|b| {
                    let gdir = self.terrain_gravity
                        .as_ref()
                        .map(|g| nalgebra::Vector3::new(g.direction[0], g.direction[1], g.direction[2]))
                        .unwrap_or(nalgebra::Vector3::new(0.0, -1.0, 0.0));
                    b.linvel().dot(&gdir)
                })
                .unwrap_or(0.0);
            // Find nearest planet distance.
            // When terrain is active, use the frozen planet position
            // (matches what the terrain system actually uses) instead of
            // the orbiting position which diverges rapidly at TIME_SCALE=30.
            if let Some(terrain_mgr) = &self.terrain {
                let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                let dist_m = (cam_rel[0] * cam_rel[0]
                    + cam_rel[1] * cam_rel[1]
                    + cam_rel[2] * cam_rel[2]).sqrt();
                let ratio = dist_m / terrain_mgr.planet_radius_m();
                log::info!(
                    "APPROACH_DIAG: dist={:.0}km, ratio={:.2}x, drive={}, terrain={}, gravity_blend={:.2}, vertical_speed={:.1}m/s",
                    dist_m / 1000.0, ratio, drive_mode, terrain_active, gravity_blend, vertical_speed,
                );
            } else if let Some(sys) = &self.active_system {
                let positions = sys.compute_positions_ly_pub();
                let ly_to_m = 9.461e15_f64;
                let mut best_dist_km = f64::MAX;
                let mut best_ratio = f64::MAX;
                for (i, pos) in positions.iter().enumerate() {
                    if let Some(r_m) = sys.body_radius_m(i)
                        && sys.planet_data(i).is_some()
                    {
                        let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                        let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                        let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                        let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
                        let ratio = dist_m / r_m;
                        if ratio < best_ratio {
                            best_ratio = ratio;
                            best_dist_km = dist_m / 1000.0;
                        }
                    }
                }
                if best_dist_km < f64::MAX {
                    log::info!(
                        "APPROACH_DIAG: dist={:.0}km, ratio={:.2}x, drive={}, terrain={}, gravity_blend={:.2}, vertical_speed={:.1}m/s",
                        best_dist_km, best_ratio, drive_mode, terrain_active, gravity_blend, vertical_speed,
                    );
                }
            }
        }

        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            // 1. Render BOTH monitors to offscreen textures in ONE encoder
            // (avoids multiple queue.submit() calls per frame)
            if let Some(ui_sys) = &mut self.ui_system {
                let system_info = self.active_system.as_ref()
                    .map(|sys| sys.body_summary());
                let target_info = self.navigation.locked_target.as_ref().map(|t| {
                    let speed_ly_s = self.drive.current_speed_ly_s();
                    let (dist, eta) = self.navigation
                        .target_eta(self.galactic_position, speed_ly_s)
                        .unwrap_or((0.0, f64::INFINITY));
                    (t.catalog_name.clone(), dist, eta)
                });
                let helm_data = ui::helm_screen::HelmData {
                    speed: self.ship.as_ref()
                        .map(|s| s.speed(&self.physics))
                        .unwrap_or(0.0),
                    throttle: self.ship.as_ref()
                        .map(|s| s.throttle)
                        .unwrap_or(0.0),
                    engine_on: self.ship.as_ref()
                        .map(|s| s.engine_on)
                        .unwrap_or(false),
                    fuel: self.ship_resources.fuel,
                    drive_mode: self.drive.mode(),
                    drive_status: self.drive.status(),
                    drive_speed_c: self.drive.current_speed_c(),
                    exotic_fuel: self.ship_resources.exotic_fuel,
                    system_info,
                    target_info,
                    // Only show altitude when raycasts actually hit
                    // terrain (< 100m). At 100m (MAX_RAY_DIST) the rays
                    // missed — show SURF distance instead.
                    // Show raycast altitude only when actually hitting terrain
                    // (not the default 100m max). The surface barrier can be
                    // hit at distances close to 100m, so filter generously.
                    altitude_m: self.last_clearance.filter(|&c| c < 95.0),
                    planet_dist_km: {
                        // Show distance to nearest planet surface when < 1000km
                        // and heading toward it (dot product check).
                        let ly_to_m = 9.461e15_f64;
                        let mut best: Option<f32> = None;
                        // Always show when terrain is active (any altitude).
                        // In a system without terrain, show within 50,000km.
                        if let Some(terrain_mgr) = &self.terrain {
                            let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                            let dist_m = (cam_rel[0]*cam_rel[0] + cam_rel[1]*cam_rel[1] + cam_rel[2]*cam_rel[2]).sqrt();
                            let alt_km = ((dist_m - terrain_mgr.planet_radius_m()) / 1000.0) as f32;
                            best = Some(alt_km.max(0.0));
                        } else if let Some(sys) = &self.active_system {
                            let positions = sys.compute_positions_ly_pub();
                            for (i, pos) in positions.iter().enumerate() {
                                if let Some(r_m) = sys.body_radius_m(i)
                                    && sys.planet_data(i).is_some()
                                {
                                    let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                                    let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                                    let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                                    let dist_m = (dx*dx + dy*dy + dz*dz).sqrt();
                                    let alt_km = ((dist_m - r_m) / 1000.0) as f32;
                                    if (0.0..50_000.0).contains(&alt_km) && best.is_none_or(|b| alt_km < b) {
                                        best = Some(alt_km);
                                    }
                                }
                            }
                        }
                        best
                    },
                };

                // Placeholder: deposits are positioned in rapier space at
                // game start. After origin rebase they become stale. Skip
                // contacts when terrain is active (deposits are in space,
                // not on planet surfaces). TODO: replace with galactic-
                // coordinate deposits in the resource gathering system.
                let contacts: Vec<ui::sensors_screen::SensorContact> = if self.terrain.is_some() {
                    Vec::new()
                } else {
                    let ship_pos = self.ship.as_ref()
                        .and_then(|s| s.position(&self.physics))
                        .unwrap_or((0.0, 0.0, 0.0));
                    self.deposits.iter()
                        .map(|dep| {
                            let dx = dep.position[0] - ship_pos.0;
                            let dy = dep.position[1] - ship_pos.1;
                            let dz = dep.position[2] - ship_pos.2;
                            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                            ui::sensors_screen::SensorContact {
                                label: dep.kind.label().to_string(),
                                icon: dep.kind.icon().to_string(),
                                distance: dist,
                                gathered: self.gathered.contains(&dep.id),
                            }
                        })
                        .collect()
                };
                let sensors_data = ui::sensors_screen::SensorsData {
                    contacts,
                    ship_fuel: self.ship_resources.fuel,
                };

                // Single encoder for both monitors
                let mut monitor_encoder = gpu.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: Some("Monitor Encoder") },
                );
                ui_sys.render_helm_monitor(&gpu.device, &gpu.queue, &mut monitor_encoder, &helm_data);
                ui_sys.render_sensors_monitor(&gpu.device, &gpu.queue, &mut monitor_encoder, &sensors_data);
                gpu.queue.submit(std::iter::once(monitor_encoder.finish()));
            }

            // 2. Build draw commands
            let mut commands = if self.view_mode == 6 || self.view_mode == 7 {
                // Debug: ship part viewing at origin
                if let Some(ship_mesh) = self.ship_part_mesh {
                    vec![DrawCommand {
                        mesh: ship_mesh,
                        model_matrix: Mat4::IDENTITY,
                        pre_rebased: false,
                    }]
                } else {
                    vec![]
                }
            } else {
                // Normal gameplay: render ship hull + interactables
                let mut cmds = Vec::new();

                // Ship hull
                // Ship transform from physics body (position + rotation)
                let ship_transform = if let Some(ship) = &self.ship {
                    if let Some(body) = self.physics.get_body(ship.body_handle) {
                        let p = body.translation();
                        let r = body.rotation();
                        let rot = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                        Mat4::from_rotation_translation(rot, Vec3::new(p.x, p.y, p.z))
                    } else { Mat4::IDENTITY }
                } else { Mat4::IDENTITY };

                // Hull mesh at ship's world position/rotation
                if let Some(hull_handle) = self.ship_part_mesh {
                    cmds.push(DrawCommand {
                        mesh: hull_handle,
                        model_matrix: ship_transform,
                        pre_rebased: false,
                    });
                }

                // Interactable meshes in ship-local space (transformed by ship).
                // Skip the Speed Display mesh (id=3) — replaced by the egui monitor quad.
                let layout = sa_ship::station::cockpit_layout();
                for (i, handle) in self.interactable_meshes.iter().enumerate() {
                    if let Some(ids) = &self.ship_ids
                        && i == ids.speed_screen
                    {
                        continue;
                    }
                    if let Some(placement) = layout.interactables.get(i) {
                        let pos = placement.position;
                        cmds.push(DrawCommand {
                            mesh: *handle,
                            model_matrix: ship_transform * Mat4::from_translation(pos),
                            pre_rebased: false,
                        });
                    }
                }

                cmds
            };

            // Append solar system bodies (planets, moons, star) if in a system
            if let Some(system) = &mut self.active_system {
                let hidden = system.hidden_body_index;
                let total_bodies = system.body_count();
                let system_commands = system.update(
                    dt as f64,
                    self.galactic_position,
                );
                // Log every 60 frames when terrain is active
                if self.terrain.is_some() && self.time.frame_count().is_multiple_of(60) {
                    log::info!("RENDER: {} solar cmds (hidden={:?}, bodies={}), {} terrain cmds",
                        system_commands.len(), hidden, total_bodies, terrain_commands.len());
                }
                commands.extend(system_commands);
            }

            // Append terrain draw commands
            commands.extend(terrain_commands);

            // DIAGNOSTIC: dump every draw command's position once per second
            if self.terrain.is_some() && self.time.frame_count().is_multiple_of(60) {
                for (i, cmd) in commands.iter().enumerate() {
                    let t = cmd.model_matrix.col(3);
                    let dist = ((t.x * t.x + t.y * t.y + t.z * t.z) as f64).sqrt();
                    let mesh_info = renderer.mesh_store.get(cmd.mesh)
                        .map(|m| m.index_count)
                        .unwrap_or(0);
                    log::info!("CMD[{}]: pos=({:.0},{:.0},{:.0}) dist={:.0}m tris={} pre_rebased={}",
                        i, t.x, t.y, t.z, dist, mesh_info / 3, cmd.pre_rebased);
                }
            }

            // Unload system if player has cruised far away (> 100 AU from star)
            if let Some(system) = &self.active_system {
                let dist = self.galactic_position.distance_to(system.star_galactic_pos);
                let au_in_ly = 1.581e-5_f64;
                if dist > 100.0 * au_in_ly {
                    log::info!("Left system boundary — unloading");
                    if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                    self.terrain = None;
                    self.terrain_gravity = None;
                    self.active_system = None;
                }
            }

            // 3. Build screen draw commands for in-world monitors
            let screen_draws: Vec<ScreenDrawCommand<'_>> =
                if self.view_mode == 0 {
                    // In normal gameplay, place the screen at the Speed Display position
                    let ship_transform = if let Some(ship) = &self.ship {
                        if let Some(body) = self.physics.get_body(ship.body_handle) {
                            let p = body.translation();
                            let r = body.rotation();
                            let rot = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                            Mat4::from_rotation_translation(rot, Vec3::new(p.x, p.y, p.z))
                        } else { Mat4::IDENTITY }
                    } else { Mat4::IDENTITY };

                    let mut draws = Vec::new();
                    // Helm monitor at Speed Display position
                    if let (Some(quad), Some(bind_group)) =
                        (&self.screen_quad, &self.screen_bind_group)
                    {
                        let screen_pos = Vec3::new(0.0, 0.3, 0.8);
                        draws.push(ScreenDrawCommand {
                            quad,
                            model_matrix: ship_transform * Mat4::from_translation(screen_pos),
                            texture_bind_group: bind_group,
                        });
                    }
                    // Sensors monitor to the right of the helm
                    if let (Some(quad), Some(bind_group)) =
                        (&self.sensors_quad, &self.sensors_bind_group)
                    {
                        let screen_pos = Vec3::new(0.8, 0.3, 0.8);
                        draws.push(ScreenDrawCommand {
                            quad,
                            model_matrix: ship_transform * Mat4::from_translation(screen_pos),
                            texture_bind_group: bind_group,
                        });
                    }
                    draws
                } else {
                    vec![]
                };

            self.perf.draw_calls = commands.len() as u32 + screen_draws.len() as u32;
            self.perf.star_count = renderer.star_field.star_count;

            // 4. Render 3D scene + screen quads
            let drive_render = sa_render::DriveRenderParams {
                velocity_dir: self.drive_visuals.visuals.velocity_dir,
                beta: self.drive_visuals.visuals.beta,
                streak_factor: self.drive_visuals.visuals.streak_factor,
                warp_intensity: self.drive_visuals.visuals.warp_intensity,
                flash_intensity: self.drive_visuals.visuals.flash_intensity,
            };
            if let Some(mut frame_ctx) = renderer.render_frame(
                gpu,
                &self.camera,
                &commands,
                &screen_draws,
                Vec3::new(0.5, -0.8, -0.3),
                self.galactic_position,
                &drive_render,
            ) {
                // 5. Render HUD overlay via egui
                if let Some(ui_sys) = &mut self.ui_system {
                    let hovered_kind = self.interaction.as_ref()
                        .and_then(|inter| {
                            let id = inter.hovered()?;
                            Some(inter.get(id)?.kind.clone())
                        });
                    // Project locked target to screen space
                    let sw = gpu.config.width as f32;
                    let sh = gpu.config.height as f32;
                    let aspect = sw / sh;
                    let vp = self.camera.view_projection_matrix(aspect);
                    let (target_screen, target_angle, target_name, target_dist) =
                        if let Some(target) = &self.navigation.locked_target {
                            // Live distance (not stale lock-time value)
                            let live_dist = self.galactic_position.distance_to(target.galactic_pos);

                            // Convert galactic ly position to camera-relative meters
                            let ly_to_m: f64 = 9.461e15;
                            let dx = (target.galactic_pos.x - self.galactic_position.x) * ly_to_m;
                            let dy = (target.galactic_pos.y - self.galactic_position.y) * ly_to_m;
                            let dz = (target.galactic_pos.z - self.galactic_position.z) * ly_to_m;
                            let cam_rel = glam::Vec3::new(dx as f32, dy as f32, dz as f32)
                                - glam::Vec3::new(
                                    self.camera.position.x as f32,
                                    self.camera.position.y as f32,
                                    self.camera.position.z as f32,
                                );
                            let clip = vp * glam::Vec4::new(cam_rel.x, cam_rel.y, cam_rel.z, 1.0);
                            if clip.w > 0.0 {
                                let ndc_x = clip.x / clip.w;
                                let ndc_y = clip.y / clip.w;
                                let sx = (ndc_x * 0.5 + 0.5) * sw;
                                let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * sh;
                                if sx >= 0.0 && sx <= sw && sy >= 0.0 && sy <= sh {
                                    (Some([sx, sy]), None, Some(target.catalog_name.clone()), Some(live_dist))
                                } else {
                                    let angle = (sy - sh / 2.0).atan2(sx - sw / 2.0);
                                    (None, Some(angle), Some(target.catalog_name.clone()), Some(live_dist))
                                }
                            } else {
                                let angle = (-clip.y).atan2(-clip.x);
                                (None, Some(angle), Some(target.catalog_name.clone()), Some(live_dist))
                            }
                        } else {
                            (None, None, None, None)
                        };
                    let hud_state = ui::HudState {
                        hovered_kind,
                        screen_width: gpu.config.width,
                        screen_height: gpu.config.height,
                        fuel: self.ship_resources.fuel,
                        oxygen: self.ship_resources.oxygen,
                        gather_available: self.nearest_gatherable.is_some(),
                        suit_o2: self.suit.oxygen,
                        suit_power: self.suit.power,
                        cursor_grabbed: self.cursor_grabbed,
                        target_screen_pos: target_screen,
                        target_off_screen_angle: target_angle,
                        target_name,
                        target_distance_ly: target_dist,
                        time: self.time.total_seconds() as f32,
                    };
                    ui_sys.render_hud(
                        &gpu.device,
                        &gpu.queue,
                        &mut frame_ctx.encoder,
                        &frame_ctx.view,
                        &hud_state,
                    );
                }
                // 6. Submit and present
                renderer.submit_frame(gpu, frame_ctx);
            }
        }
        self.perf.render_us = t3.elapsed().as_micros() as u64;
        self.perf.total_us = frame_start.elapsed().as_micros() as u64;

        let dt_secs = self.time.delta_seconds();
        if dt_secs > 0.0 {
            self.perf.fps = 1.0 / dt_secs;
        }

        // Update window title with perf stats every 0.5s
        self.perf_update_timer += dt_secs;
        if self.perf_update_timer >= 0.5 {
            self.perf_update_timer = 0.0;
            if let Some(window) = &self.window {
                let helm_status = if self.fly_mode {
                    "FLY".to_string()
                } else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                    let speed = self.ship.as_ref()
                        .map(|s| s.speed(&self.physics))
                        .unwrap_or(0.0);
                    match self.drive.mode() {
                        sa_ship::DriveMode::Warp if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                            let ly_s = self.drive.current_speed_c() * 3.169e-8;
                            format!("WARP {:.3} ly/s", ly_s)
                        }
                        sa_ship::DriveMode::Cruise if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                            format!("CRUISE {:.0}c", self.drive.current_speed_c())
                        }
                        _ => format!("HELM {:.1}m/s", speed),
                    }
                } else {
                    "WALK".to_string()
                };
                // Interaction feedback in title
                let hover_info = if let Some(interaction) = &self.interaction {
                    if let Some(id) = interaction.hovered() {
                        if let Some(inter) = interaction.get(id) {
                            format!(" | [{}]", inter.label)
                        } else { String::new() }
                    } else { String::new() }
                } else { String::new() };
                let engine_info = if let Some(ship) = &self.ship {
                    format!(" | Engine:{} Throttle:{:.0}%",
                        if ship.engine_on { "ON" } else { "OFF" },
                        ship.throttle * 100.0)
                } else { String::new() };

                window.set_title(&format!(
                    "SpaceAway | {:.0} FPS | {}{}{} | draws {}",
                    self.perf.fps,
                    helm_status,
                    hover_info,
                    engine_info,
                    self.perf.draw_calls,
                ));
            }
        }
    }
}
