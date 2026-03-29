use sa_input::InputState;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::Camera;
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use std::io::Write;

/// Snapshot of App state needed to write the debug JSON file.
pub struct DebugSnapshot<'a> {
    pub frame_count: u64,
    pub view_mode: u8,
    pub helm: Option<&'a HelmController>,
    pub fly_mode: bool,
    pub cursor_grabbed: bool,
    pub player: Option<&'a PlayerController>,
    pub ship: Option<&'a Ship>,
    pub interaction: Option<&'a InteractionSystem>,
    pub camera: &'a Camera,
    pub input: &'a InputState,
    pub physics: &'a PhysicsWorld,
    pub perf_total_us: u64,
    pub perf_player_us: u64,
    pub perf_physics_us: u64,
    pub perf_stars_us: u64,
    pub perf_render_us: u64,
    pub perf_fps: f64,
}

/// Write live debug state to /tmp/spaceaway_debug.json for external inspection.
#[allow(clippy::collapsible_if)]
pub fn write_debug_state(s: &DebugSnapshot<'_>) {
    let mut lines = Vec::new();
    lines.push("{".to_string());
    lines.push(format!("  \"frame\": {},", s.frame_count));
    lines.push(format!("  \"view_mode\": \"{}\",", match s.view_mode {
        6 => "PART_PREVIEW",
        7 => "SHIP_PREVIEW",
        _ => if s.helm.as_ref().is_some_and(|h| h.is_seated()) { "HELM" }
             else if s.fly_mode { "FLY" }
             else { "WALK" },
    }));
    lines.push(format!("  \"cursor_grabbed\": {},", s.cursor_grabbed));

    // Player state
    if let Some(player) = s.player {
        if let Some(body) = s.physics.get_body(player.body_handle) {
            let p = body.translation();
            let v = body.linvel();
            lines.push("  \"player\": {".to_string());
            lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],", p.x, p.y, p.z));
            lines.push(format!("    \"vel\": [{:.3}, {:.3}, {:.3}],", v.x, v.y, v.z));
            lines.push(format!("    \"speed\": {:.3},", v.magnitude()));
            lines.push(format!("    \"sleeping\": {},", body.is_sleeping()));
            lines.push(format!("    \"grounded\": {},", player.grounded));
            lines.push(format!("    \"yaw\": {:.3}, \"pitch\": {:.3}", player.yaw, player.pitch));
            lines.push("  },".to_string());
        }
    }

    // Ship state
    if let Some(ship) = s.ship {
        if let Some(body) = s.physics.get_body(ship.body_handle) {
            let p = body.translation();
            let v = body.linvel();
            lines.push("  \"ship\": {".to_string());
            lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],", p.x, p.y, p.z));
            lines.push(format!("    \"vel\": [{:.3}, {:.3}, {:.3}],", v.x, v.y, v.z));
            lines.push(format!("    \"throttle\": {:.3},", ship.throttle));
            lines.push(format!("    \"engine_on\": {},", ship.engine_on));
            lines.push(format!("    \"mass\": {:.1}", body.mass()));
            lines.push("  },".to_string());
        }
    }

    // Interaction state
    if let Some(interaction) = s.interaction {
        lines.push("  \"interaction\": {".to_string());
        lines.push(format!("    \"hovered\": {:?},", interaction.hovered()));
        lines.push(format!("    \"dragging\": {},", interaction.is_dragging()));
        let dr = interaction.debug_ray();
        lines.push(format!(
            "    \"debug_ray\": {{\"origin\": [{:.3}, {:.3}, {:.3}], \"dir\": [{:.3}, {:.3}, {:.3}], \"hit\": {:?}, \"hit_id\": {:?}}},",
            dr.ray_origin[0], dr.ray_origin[1], dr.ray_origin[2],
            dr.ray_dir[0], dr.ray_dir[1], dr.ray_dir[2],
            dr.hit, dr.hit_id,
        ));
        let mut interactable_lines = Vec::new();
        for i in 0..10 {
            if let Some(inter) = interaction.get(i) {
                if let Some(col) = s.physics.collider_set.get(inter.collider_handle) {
                    let p = col.position().translation;
                    interactable_lines.push(format!(
                        "      {{\"id\": {}, \"label\": \"{}\", \"world_pos\": [{:.2}, {:.2}, {:.2}]}}",
                        i, inter.label, p.x, p.y, p.z));
                }
            } else { break; }
        }
        lines.push(format!("    \"interactables\": [{}]", interactable_lines.join(",")));
        lines.push("  },".to_string());
    }

    // Camera
    lines.push("  \"camera\": {".to_string());
    lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],",
        s.camera.position.x, s.camera.position.y, s.camera.position.z));
    lines.push(format!("    \"yaw\": {:.3}, \"pitch\": {:.3}", s.camera.yaw, s.camera.pitch));
    lines.push("  },".to_string());

    // Input
    lines.push("  \"input\": {".to_string());
    lines.push(format!("    \"mouse_delta\": [{:.1}, {:.1}],",
        s.input.mouse.delta().0, s.input.mouse.delta().1));
    lines.push(format!("    \"left_btn\": {}", s.input.mouse.left_pressed()));
    lines.push("  },".to_string());

    // Timing
    lines.push("  \"timing_ms\": {".to_string());
    lines.push(format!("    \"total\": {:.2},", s.perf_total_us as f64 / 1000.0));
    lines.push(format!("    \"player\": {:.2},", s.perf_player_us as f64 / 1000.0));
    let phys_step_ms = (s.perf_physics_us / 1000) as f64 / 1000.0;
    let qp_ms = (s.perf_physics_us % 1000) as f64 / 1000.0;
    let move_shape_ms = s.perf_stars_us as f64 / 1000.0;
    lines.push(format!("    \"phys_step\": {:.2},", phys_step_ms));
    lines.push(format!("    \"query_pipeline\": {:.2},", qp_ms));
    lines.push(format!("    \"move_shape\": {:.2},", move_shape_ms));
    lines.push(format!("    \"render\": {:.2},", s.perf_render_us as f64 / 1000.0));
    lines.push(format!("    \"fps\": {:.0}", s.perf_fps));
    lines.push("  },".to_string());

    // Player-to-ship relative position
    if let (Some(player), Some(ship)) = (s.player, s.ship) {
        if let (Some(pb), Some(sb)) = (
            s.physics.get_body(player.body_handle),
            s.physics.get_body(ship.body_handle),
        ) {
            let pp = pb.translation();
            let sp = sb.translation();
            lines.push(format!("  \"player_ship_offset\": [{:.3}, {:.3}, {:.3}],",
                pp.x - sp.x, pp.y - sp.y, pp.z - sp.z));
        }
    }

    lines.push("  \"physics\": {".to_string());
    lines.push(format!("    \"bodies\": {},", s.physics.rigid_body_set.len()));
    lines.push(format!("    \"colliders\": {},", s.physics.collider_set.len()));
    let grav = s.physics.gravity();
    lines.push(format!("    \"gravity\": [{:.1}, {:.1}, {:.1}]", grav.0, grav.1, grav.2));
    lines.push("  }".to_string());

    lines.push("}".to_string());

    let content = lines.join("\n");
    if let Ok(mut f) = std::fs::File::create("/tmp/spaceaway_debug.json") {
        let _ = f.write_all(content.as_bytes());
    }
}
