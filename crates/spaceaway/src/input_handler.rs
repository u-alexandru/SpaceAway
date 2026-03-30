use super::{App, GamePhase};
use crate::mesh_utils::{all_ship_parts, assemble_ship, meshgen_to_render};
use crate::solar_system;
use sa_math::WorldPos;
use sa_universe::MasterSeed;
use winit::keyboard::KeyCode;

impl App {
    /// Handle keyboard input events (pressed keys during Playing phase).
    /// Returns true if the event was consumed.
    pub(super) fn handle_keyboard(&mut self, code: KeyCode, pressed: bool) -> bool {
        if !pressed {
            return false;
        }

        // Menu keyboard navigation
        if self.phase == GamePhase::Menu {
            if let Some(menu) = &mut self.menu {
                match code {
                    KeyCode::ArrowUp | KeyCode::KeyW => menu.nav_up(),
                    KeyCode::ArrowDown | KeyCode::KeyS => menu.nav_down(),
                    KeyCode::Enter | KeyCode::Space => {
                        self.handle_menu_select();
                    }
                    _ => {}
                }
            }
            return true;
        }

        // Debug teleport: 0 = 1km above nearest planet
        if code == KeyCode::Digit0 && self.phase == GamePhase::Playing {
            self.handle_planet_teleport();
            return true;
        }

        // Remaining keys: only when NOT seated at helm
        let is_seated = self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false);
        if !is_seated && self.phase == GamePhase::Playing {
            match code {
                KeyCode::Digit1 if self.fly_mode => self.teleport_to(0),
                KeyCode::Digit2 if self.fly_mode => self.teleport_to(1),
                KeyCode::Digit3 if self.fly_mode => self.teleport_to(2),
                KeyCode::Digit4 if self.fly_mode => self.teleport_to(3),
                KeyCode::Digit5 if self.fly_mode => self.teleport_to(4),
                KeyCode::KeyF => {
                    self.fly_mode = !self.fly_mode;
                    log::info!("Fly mode: {}", if self.fly_mode {
                        "ON (WASD to fly, scroll to change speed)"
                    } else { "OFF" });
                }
                KeyCode::KeyV => {
                    if let Some(gpu) = &mut self.gpu {
                        let vsync = gpu.toggle_vsync();
                        log::info!("VSync: {}", if vsync {
                            "ON (60 FPS cap)"
                        } else {
                            "OFF (uncapped — benchmark mode)"
                        });
                    }
                }
                KeyCode::F3 => self.toggle_profiler(),
                KeyCode::Digit6 => self.cycle_ship_part(),
                KeyCode::Digit7 => self.show_full_ship(),
                KeyCode::Tab => {
                    self.wants_star_lock = true;
                }
                KeyCode::Digit8 => self.handle_digit8(),
                KeyCode::Digit9 => self.handle_digit9(),
                KeyCode::Backquote => self.handle_find_interesting_planet(),
                KeyCode::Equal => {
                    self.fly_speed *= 2.0;
                    log::info!("Fly speed: {:.0} ly/s", self.fly_speed);
                }
                KeyCode::Minus => {
                    self.fly_speed = (self.fly_speed / 2.0).max(1.0);
                    log::info!("Fly speed: {:.0} ly/s", self.fly_speed);
                }
                _ => return false,
            }
            return true;
        }

        false
    }

    fn handle_menu_select(&mut self) {
        if let Some(menu) = &self.menu {
            match menu.selected {
                0 => {
                    self.phase = GamePhase::Playing;
                    self.menu = None;
                    self.star_streaming.force_load(self.galactic_position);
                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                        let verts = self.star_streaming.build_vertices(self.galactic_position);
                        renderer.star_field.update_star_buffer(&gpu.device, &verts);
                    }
                    self.audio.set_power(true);
                    log::info!("Starting game — entering ship");
                }
                3 => self.quit_requested = true,
                _ => {}
            }
        }
    }

    fn handle_planet_teleport(&mut self) {
        let ly_to_m = 9.461e15_f64;
        let m_to_ly = 1.0 / ly_to_m;
        let planet_info: Option<(WorldPos, f64)> = if let Some(terrain_mgr) = &self.terrain {
            Some((terrain_mgr.frozen_planet_center_ly(), terrain_mgr.planet_radius_m()))
        } else if let Some(sys) = &self.active_system {
            let positions = sys.compute_positions_ly_pub();
            let mut best: Option<(usize, f64)> = None;
            for (i, pos) in positions.iter().enumerate() {
                if sys.body_radius_m(i).is_some() && sys.planet_data(i).is_some() {
                    let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                    let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                    let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                    let d = (dx * dx + dy * dy + dz * dz).sqrt();
                    if best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                        best = Some((i, d));
                    }
                }
            }
            best.and_then(|(i, _)| {
                let r = sys.body_radius_m(i)?;
                Some((*positions.get(i)?, r))
            })
        } else {
            None
        };

        let Some((planet_pos, radius_m)) = planet_info else {
            log::warn!("No planet found — press 8 to enter a system first");
            return;
        };

        let dx = (self.galactic_position.x - planet_pos.x) * ly_to_m;
        let dy = (self.galactic_position.y - planet_pos.y) * ly_to_m;
        let dz = (self.galactic_position.z - planet_pos.z) * ly_to_m;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
        let target_dist = radius_m + 1000.0;
        let scale = target_dist / dist;
        let new_cam_rel = [dx * scale, dy * scale, dz * scale];
        self.galactic_position = WorldPos::new(
            planet_pos.x + new_cam_rel[0] * m_to_ly,
            planet_pos.y + new_cam_rel[1] * m_to_ly,
            planet_pos.z + new_cam_rel[2] * m_to_ly,
        );
        self.camera.position = self.galactic_position;

        if let Some(terrain_mgr) = &mut self.terrain {
            terrain_mgr.set_anchor(new_cam_rel);
            if let Some(renderer) = &mut self.renderer {
                terrain_mgr.flush_for_teleport(&mut renderer.terrain_slab);
            }
        }
        if let Some(ship) = &self.ship
            && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
        {
            body.set_translation(nalgebra::Vector3::zeros(), true);
            body.set_linvel(nalgebra::Vector3::zeros(), true);
            body.set_angvel(nalgebra::Vector3::zeros(), true);
            let len = (new_cam_rel[0] * new_cam_rel[0]
                + new_cam_rel[1] * new_cam_rel[1]
                + new_cam_rel[2] * new_cam_rel[2])
                .sqrt();
            let cam_rel_norm = nalgebra::Vector3::new(
                (new_cam_rel[0] / len) as f32,
                (new_cam_rel[1] / len) as f32,
                (new_cam_rel[2] / len) as f32,
            );
            let up = nalgebra::Vector3::new(0.0f32, 1.0, 0.0);
            if let Some(rot) = nalgebra::UnitQuaternion::rotation_between(&up, &cam_rel_norm) {
                body.set_rotation(rot, true);
            }
        }
        if let Some(player) = &self.player
            && let Some(body) = self.physics.rigid_body_set.get_mut(player.body_handle)
        {
            body.set_translation(nalgebra::Vector3::zeros(), true);
            body.set_linvel(nalgebra::Vector3::zeros(), true);
        }
        if let Some(player) = &mut self.player {
            player.pitch = -0.5;
            player.yaw = 0.0;
        }
        self.drive.request_disengage();
        self.camera.pitch = -0.3;
        self.helm_look_pitch = 0.0;
        self.helm_look_yaw = 0.0;
        log::info!("Teleported 1km above planet surface (radius {:.0}km, alt 1km)", radius_m / 1000.0);
    }

    fn toggle_profiler(&mut self) {
        self.show_profiler = !self.show_profiler;
        puffin::set_scopes_on(self.show_profiler);
        if self.show_profiler && self.puffin_server.is_none() {
            match puffin_http::Server::new("0.0.0.0:8585") {
                Ok(server) => {
                    log::info!("Puffin profiler ON — connect puffin_viewer to 127.0.0.1:8585");
                    self.puffin_server = Some(server);
                }
                Err(e) => log::warn!("Failed to start puffin server: {e}"),
            }
        } else if !self.show_profiler {
            self.puffin_server = None;
            log::info!("Puffin profiler OFF");
        }
    }

    fn cycle_ship_part(&mut self) {
        let parts = all_ship_parts();
        self.ship_part_index = if self.view_mode == 6 {
            (self.ship_part_index + 1) % parts.len()
        } else {
            0
        };
        self.view_mode = 6;
        let (name, mesh) = &parts[self.ship_part_index];
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let mesh_data = meshgen_to_render(mesh);
            let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
            self.ship_part_mesh = Some(handle);
            log::info!("Showing ship part: {} ({} verts, {} tris)",
                name, mesh.vertices.len(), mesh.triangle_count());
        }
    }

    fn show_full_ship(&mut self) {
        self.view_mode = 7;
        let ship_mesh = assemble_ship();
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let mesh_data = meshgen_to_render(&ship_mesh);
            let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
            self.ship_part_mesh = Some(handle);
            log::info!("Assembled full ship: {} verts, {} tris",
                ship_mesh.vertices.len(), ship_mesh.triangle_count());
        }
    }

    fn handle_digit8(&mut self) {
        if let Some(system) = &mut self.active_system {
            let planets = &system.system.planets;
            if planets.is_empty() { return; }
            let idx = self.teleport_counter as usize % planets.len();
            self.teleport_counter += 1;
            let planet = &planets[idx];
            let meters_in_ly = 1.0 / 9.461e15_f64;
            let planet_radius_m = planet.radius_earth as f64 * 6_371_000.0;
            let view_dist_m = planet_radius_m * 3.0;
            let positions = system.compute_positions_ly_pub();
            let mut body_idx = 2;
            for pi in 0..idx {
                body_idx += 1;
                let pp = &system.system.planets[pi];
                if pp.atmosphere.is_some() { body_idx += 1; }
                if pp.has_rings { body_idx += 1; }
                body_idx += pp.moons.len();
            }
            let planet_pos = positions.get(body_idx)
                .copied()
                .unwrap_or(system.star_galactic_pos);
            let dx = planet_pos.x - system.star_galactic_pos.x;
            let dz = planet_pos.z - system.star_galactic_pos.z;
            let dist_ly = (dx * dx + dz * dz).sqrt();
            let (dir_x, dir_z) = if dist_ly > 1e-20 {
                (dx / dist_ly, dz / dist_ly)
            } else {
                (1.0, 0.0)
            };
            let above_ly = view_dist_m * 0.4 * meters_in_ly;
            self.galactic_position = WorldPos::new(
                planet_pos.x + dir_x * view_dist_m * meters_in_ly,
                planet_pos.y + above_ly,
                planet_pos.z + dir_z * view_dist_m * meters_in_ly,
            );
            self.camera.position = self.galactic_position;
            self.camera.orientation_override = None;
            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
            self.camera.pitch = -0.2;
            log::info!("→ Planet {} of {}: {:?} {:.0}km at {:.2}AU (viewing from {:.0}km)",
                idx + 1, planets.len(), planet.sub_type,
                planet.radius_earth * 6371.0, planet.orbital_radius_au,
                view_dist_m / 1000.0);
        } else {
            self.handle_jump_to_star();
        }
    }

    fn handle_jump_to_star(&mut self) {
        self.navigation.update_nearby(self.galactic_position);
        let Some(star) = self.navigation.nearby_stars.first().cloned() else { return };
        log::info!("Jumping to: {} ({:.2} ly)", star.catalog_name, star.distance_ly);
        let sector_coord = sa_universe::SectorCoord::new(
            star.id.sector_x().into(),
            star.id.sector_y().into(),
            star.id.sector_z().into(),
        );
        let sector = sa_universe::sector::generate_sector(MasterSeed(42), sector_coord);
        let Some(placed) = sector.stars.iter().find(|s| s.id == star.id) else { return };
        let system = sa_universe::generate_system(&placed.star, star.id.0);
        let au_in_ly = 1.581e-5_f64;
        let meters_in_ly = 1.0 / 9.461e15_f64;
        let (offset_ly, desc) = if let Some(p) = system.planets.first() {
            let r_m = p.radius_earth as f64 * 6_371_000.0;
            let view_m = r_m * 1.8;
            let orb_ly = p.orbital_radius_au as f64 * au_in_ly;
            (orb_ly + view_m * meters_in_ly, format!(
                "Planet 1: {:?} {:.0}km at {:.2}AU",
                p.sub_type, p.radius_earth * 6371.0, p.orbital_radius_au))
        } else {
            let sr = placed.star.radius as f64 * 696_000_000.0;
            (sr * 3.0 * meters_in_ly, "No planets — near star".into())
        };
        let above_ly = offset_ly * 0.3;
        self.galactic_position = WorldPos::new(
            star.galactic_pos.x + offset_ly,
            star.galactic_pos.y + above_ly,
            star.galactic_pos.z);
        self.camera.position = self.galactic_position;
        self.camera.orientation_override = None;
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = -0.15;
        self.drive.request_disengage();
        self.teleport_counter = 1;
        self.star_streaming.force_load(self.galactic_position);
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let verts = self.star_streaming.build_vertices(self.galactic_position);
            renderer.star_field.update_star_buffer(&gpu.device, &verts);
        }
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let sys = solar_system::ActiveSystem::load(
                star.id, placed, &mut renderer.mesh_store, &gpu.device);
            log::info!("System: {} bodies — {}", sys.body_count(), desc);
            for line in sys.body_summary() { log::info!("  {}", line); }
            if let Some(p) = system.planets.first() {
                let positions = sys.compute_positions_ly_pub();
                if let Some(&planet_pos) = positions.get(2) {
                    let r_m = p.radius_earth as f64 * 6_371_000.0;
                    let view_m = r_m * 1.8;
                    let dx = planet_pos.x - star.galactic_pos.x;
                    let dz = planet_pos.z - star.galactic_pos.z;
                    let d = (dx * dx + dz * dz).sqrt();
                    let (nx, nz) = if d > 1e-20 { (dx / d, dz / d) } else { (1.0, 0.0) };
                    self.galactic_position = WorldPos::new(
                        planet_pos.x + nx * view_m * meters_in_ly,
                        planet_pos.y + view_m * 0.4 * meters_in_ly,
                        planet_pos.z + nz * view_m * meters_in_ly,
                    );
                    self.camera.position = self.galactic_position;
                }
            }
            if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
            self.terrain = None;
            self.terrain_gravity = None;
            self.active_system = Some(sys);
        }
        self.navigation.clear_target();
    }

    fn handle_digit9(&mut self) {
        if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
        self.terrain = None;
        self.terrain_gravity = None;
        self.active_system = None;
        self.navigation.update_nearby(self.galactic_position);
        let skip = (self.teleport_counter as usize)
            % self.navigation.nearby_stars.len().max(1);
        self.teleport_counter += 1;
        if let Some(star) = self.navigation.nearby_stars.get(skip).cloned() {
            let au_in_ly = 1.581e-5_f64;
            self.galactic_position = WorldPos::new(
                star.galactic_pos.x + 30.0 * au_in_ly,
                star.galactic_pos.y, star.galactic_pos.z);
            self.camera.position = self.galactic_position;
            self.star_streaming.force_load(self.galactic_position);
            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                let verts = self.star_streaming.build_vertices(self.galactic_position);
                renderer.star_field.update_star_buffer(&gpu.device, &verts);
            }
            log::info!("Jumped near {} ({:.2} ly) — press 8 to enter system",
                star.catalog_name, star.distance_ly);
        }
    }

    fn handle_find_interesting_planet(&mut self) {
        log::info!("Searching for interesting planet...");
        self.navigation.update_nearby(self.galactic_position);
        let mut found = false;
        'search: for star_nav in self.navigation.nearby_stars.iter().cloned() {
            let sc = sa_universe::SectorCoord::new(
                star_nav.id.sector_x().into(),
                star_nav.id.sector_y().into(),
                star_nav.id.sector_z().into(),
            );
            let sector = sa_universe::sector::generate_sector(MasterSeed(42), sc);
            if let Some(placed) = sector.stars.iter().find(|s| s.id == star_nav.id) {
                let sys = sa_universe::generate_system(&placed.star, star_nav.id.0);
                for (pi, planet) in sys.planets.iter().enumerate() {
                    let has_feature = planet.has_rings || planet.atmosphere.is_some();
                    if !has_feature { continue; }

                    let meters_in_ly = 1.0 / 9.461e15_f64;
                    let au_in_ly = 1.581e-5_f64;
                    let r_m = planet.radius_earth as f64 * 6_371_000.0;
                    let view_m = r_m * 1.5;
                    let orb_ly = planet.orbital_radius_au as f64 * au_in_ly;

                    let above_ly = view_m * 0.4 * meters_in_ly;
                    self.galactic_position = WorldPos::new(
                        star_nav.galactic_pos.x + orb_ly + view_m * meters_in_ly,
                        star_nav.galactic_pos.y + above_ly,
                        star_nav.galactic_pos.z,
                    );
                    self.camera.position = self.galactic_position;
                    self.camera.orientation_override = None;
                    self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                    self.camera.pitch = 0.0;
                    self.drive.request_disengage();
                    self.star_streaming.force_load(self.galactic_position);
                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                        let verts = self.star_streaming.build_vertices(self.galactic_position);
                        renderer.star_field.update_star_buffer(&gpu.device, &verts);
                        let loaded = solar_system::ActiveSystem::load(
                            star_nav.id, placed, &mut renderer.mesh_store, &gpu.device);
                        log::info!("Found! {} — Planet {}: {:?} {:.0}km rings={} atmo={}",
                            star_nav.catalog_name, pi + 1, planet.sub_type,
                            planet.radius_earth * 6371.0,
                            planet.has_rings, planet.atmosphere.is_some());
                        log::info!("System: {} bodies total", loaded.body_count());
                        for line in loaded.body_summary() { log::info!("  {}", line); }
                        if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                        self.terrain = None;
                        self.terrain_gravity = None;
                        self.active_system = Some(loaded);
                    }
                    self.navigation.clear_target();
                    found = true;
                    break 'search;
                }
            }
        }
        if !found { log::warn!("No planets with rings/atmosphere found nearby"); }
    }
}
