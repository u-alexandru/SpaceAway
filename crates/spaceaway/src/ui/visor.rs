//! Suit visor HUD: holographic display projected on helmet glass.
//!
//! All drawing uses egui Painter with semi-transparent blue-green tint.
//! Orbitron font for all text. Glow effects via double-draw.

use sa_ship::InteractableKind;

/// Visor color palette.
const VISOR_R: u8 = 120;
const VISOR_G: u8 = 220;
const VISOR_B: u8 = 210;
const WARN_R: u8 = 220;
const WARN_G: u8 = 160;
const WARN_B: u8 = 80;
const CRIT_R: u8 = 220;
const CRIT_G: u8 = 80;
const CRIT_B: u8 = 60;

/// Visor font family name (must match what was loaded in UiSystem).
pub const VISOR_FONT: &str = "orbitron";

/// All state needed to draw the visor HUD.
pub struct VisorState {
    pub screen_width: f32,
    pub screen_height: f32,
    pub font_scale: f32,
    pub cursor_grabbed: bool,
    pub hovered_kind: Option<InteractableKind>,
    /// Suit oxygen (0.0 to 1.0).
    pub suit_o2: f32,
    /// Suit power (0.0 to 1.0).
    pub suit_power: f32,
    /// Target screen position (None = no target or behind camera).
    pub target_screen_pos: Option<[f32; 2]>,
    /// Angle for edge chevron when target is off-screen.
    pub target_off_screen_angle: Option<f32>,
    /// Target catalog name.
    pub target_name: Option<String>,
    /// Distance to target in light-years.
    pub target_distance_ly: Option<f64>,
    /// Time in seconds for animations.
    pub time: f32,
    /// Whether gather is available (resource deposit in range).
    pub gather_available: bool,
}

fn visor_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(VISOR_FONT.into()))
}

fn visor_color(alpha: f32) -> egui::Color32 {
    let a = (alpha * 255.0).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(VISOR_R, VISOR_G, VISOR_B, a)
}

fn warn_color(alpha: f32) -> egui::Color32 {
    let a = (alpha * 255.0).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(WARN_R, WARN_G, WARN_B, a)
}

fn crit_color(alpha: f32) -> egui::Color32 {
    let a = (alpha * 255.0).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(CRIT_R, CRIT_G, CRIT_B, a)
}

/// Draw the complete visor HUD.
pub fn draw_visor(ctx: &egui::Context, state: &VisorState) {
    let w = state.screen_width;
    let h = state.screen_height;
    let s = state.font_scale;
    let center = egui::pos2(w / 2.0, h / 2.0);

    let (visible, jx, jy, alpha_mult) = compute_degradation(state);
    if !visible {
        return;
    }

    egui::Area::new(egui::Id::new("visor_hud"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();

            // Crosshair
            if state.cursor_grabbed {
                draw_crosshair(
                    painter, center, s, alpha_mult, jx, jy,
                    &state.hovered_kind, state.gather_available,
                );
            }

            // Suit O2 (bottom-left)
            draw_suit_vital(
                painter, "O2", state.suit_o2,
                egui::pos2(40.0 * s + jx, h - 45.0 * s + jy),
                s, alpha_mult, true, state.time,
            );

            // Suit Power (bottom-right)
            draw_suit_vital(
                painter, "PWR", state.suit_power,
                egui::pos2(w - 40.0 * s + jx, h - 45.0 * s + jy),
                s, alpha_mult, false, state.time,
            );

            // Locked target reticle
            if let Some([tx, ty]) = state.target_screen_pos {
                draw_target_reticle(
                    painter,
                    egui::pos2(tx + jx, ty + jy),
                    state.target_name.as_deref(),
                    state.target_distance_ly,
                    s, alpha_mult,
                );
            } else if let Some(angle) = state.target_off_screen_angle {
                draw_edge_chevron(painter, center, angle, w, h, s, alpha_mult);
            }

            // Warning vignette
            let min_vital = state.suit_o2.min(state.suit_power);
            if min_vital < 0.8 {
                draw_warning_vignette(painter, w, h, min_vital);
            }

            // Life support failure (O2 = 0)
            if state.suit_o2 <= 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h)),
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 230),
                );
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "LIFE SUPPORT FAILURE",
                    visor_font(32.0 * s),
                    crit_color(0.9),
                );
            }
        });
}

/// Crosshair: context-sensitive icon with glow effect.
fn draw_crosshair(
    painter: &egui::Painter,
    center: egui::Pos2,
    s: f32,
    alpha: f32,
    jx: f32,
    jy: f32,
    hovered: &Option<InteractableKind>,
    gather_available: bool,
) {
    let c = egui::pos2(center.x + jx, center.y + jy);
    let base_alpha = 0.6 * alpha;

    if gather_available {
        draw_gather_visor(painter, c, s, base_alpha);
        return;
    }

    match hovered {
        None => {
            // Thin ring + center dot with glow
            let outer = visor_color(base_alpha * 0.3);
            let inner = visor_color(base_alpha);
            painter.circle_stroke(c, 6.0 * s, egui::Stroke::new(3.0 * s, outer));
            painter.circle_stroke(c, 4.0 * s, egui::Stroke::new(1.5 * s, inner));
            painter.circle_filled(c, 1.5 * s, inner);
        }
        Some(kind) => match kind {
            InteractableKind::Lever { .. } => draw_grab_visor(painter, c, s, base_alpha),
            InteractableKind::Button { .. } => draw_press_visor(painter, c, s, base_alpha),
            InteractableKind::Switch { .. } => draw_press_visor(painter, c, s, base_alpha),
            InteractableKind::HelmSeat => draw_sit_visor(painter, c, s, base_alpha),
            InteractableKind::Screen { .. } => draw_eye_visor(painter, c, s, base_alpha),
        },
    }
}

/// Gather icon: diamond in visor green.
fn draw_gather_visor(painter: &egui::Painter, c: egui::Pos2, s: f32, alpha: f32) {
    let color = visor_color(alpha);
    let glow = visor_color(alpha * 0.4);
    let sz = 10.0 * s;
    let pts = [
        egui::pos2(c.x, c.y - sz),
        egui::pos2(c.x + sz, c.y),
        egui::pos2(c.x, c.y + sz),
        egui::pos2(c.x - sz, c.y),
        egui::pos2(c.x, c.y - sz),
    ];
    for pair in pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], egui::Stroke::new(3.0 * s, glow));
    }
    for pair in pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5 * s, color));
    }
    painter.circle_filled(c, 3.0 * s, color);
}

/// Grab icon for levers: two vertical bars in visor style.
fn draw_grab_visor(painter: &egui::Painter, c: egui::Pos2, s: f32, alpha: f32) {
    let color = visor_color(alpha);
    let glow = visor_color(alpha * 0.4);
    let gap = 4.0 * s;
    let hh = 8.0 * s;
    let stroke_g = egui::Stroke::new(3.0 * s, glow);
    let stroke_c = egui::Stroke::new(2.0 * s, color);
    for st in [stroke_g, stroke_c] {
        painter.line_segment(
            [egui::pos2(c.x - gap, c.y - hh), egui::pos2(c.x - gap, c.y + hh)],
            st,
        );
        painter.line_segment(
            [egui::pos2(c.x + gap, c.y - hh), egui::pos2(c.x + gap, c.y + hh)],
            st,
        );
    }
    let thin = egui::Stroke::new(1.0 * s, visor_color(alpha * 0.5));
    painter.line_segment(
        [egui::pos2(c.x - gap, c.y - hh), egui::pos2(c.x + gap, c.y - hh)],
        thin,
    );
    painter.line_segment(
        [egui::pos2(c.x - gap, c.y + hh), egui::pos2(c.x + gap, c.y + hh)],
        thin,
    );
}

/// Press icon for buttons: circle with inner dot, glow.
fn draw_press_visor(painter: &egui::Painter, c: egui::Pos2, s: f32, alpha: f32) {
    let glow = visor_color(alpha * 0.3);
    let color = visor_color(alpha);
    painter.circle_stroke(c, 10.0 * s, egui::Stroke::new(3.0 * s, glow));
    painter.circle_stroke(c, 8.0 * s, egui::Stroke::new(1.5 * s, color));
    painter.circle_filled(c, 3.0 * s, color);
}

/// Sit icon for helm seat: downward chevron.
fn draw_sit_visor(painter: &egui::Painter, c: egui::Pos2, s: f32, alpha: f32) {
    let color = visor_color(alpha);
    let glow = visor_color(alpha * 0.4);
    let w = 8.0 * s;
    let h = 6.0 * s;
    for st in [egui::Stroke::new(3.0 * s, glow), egui::Stroke::new(2.0 * s, color)] {
        painter.line_segment(
            [egui::pos2(c.x - w, c.y - h), egui::pos2(c.x, c.y + h)],
            st,
        );
        painter.line_segment(
            [egui::pos2(c.x + w, c.y - h), egui::pos2(c.x, c.y + h)],
            st,
        );
    }
    painter.line_segment(
        [egui::pos2(c.x, c.y - h - 2.0 * s), egui::pos2(c.x, c.y - h + 2.0 * s)],
        egui::Stroke::new(1.5 * s, visor_color(alpha * 0.5)),
    );
}

/// Eye icon for screens: oval outline with pupil.
fn draw_eye_visor(painter: &egui::Painter, c: egui::Pos2, s: f32, alpha: f32) {
    let color = visor_color(alpha);
    let glow = visor_color(alpha * 0.4);
    let w = 10.0 * s;
    let h = 5.0 * s;
    let pts = [
        egui::pos2(c.x - w, c.y),
        egui::pos2(c.x - w * 0.5, c.y - h),
        egui::pos2(c.x, c.y - h * 1.1),
        egui::pos2(c.x + w * 0.5, c.y - h),
        egui::pos2(c.x + w, c.y),
        egui::pos2(c.x + w * 0.5, c.y + h),
        egui::pos2(c.x, c.y + h * 1.1),
        egui::pos2(c.x - w * 0.5, c.y + h),
        egui::pos2(c.x - w, c.y),
    ];
    for pair in pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], egui::Stroke::new(3.0 * s, glow));
    }
    for pair in pts.windows(2) {
        painter.line_segment([pair[0], pair[1]], egui::Stroke::new(1.5 * s, color));
    }
    painter.circle_filled(c, 3.0 * s, color);
}

/// Suit vital text: `O2 98%` or `PWR 100%`.
/// Dim when full, bright when draining, pulsing when critical.
fn draw_suit_vital(
    painter: &egui::Painter,
    label: &str,
    value: f32,
    pos: egui::Pos2,
    scale: f32,
    alpha_mult: f32,
    left_aligned: bool,
    time: f32,
) {
    let pct = (value * 100.0).round() as i32;
    let text = format!("{label} {pct}%");

    let (color, text_alpha) = if value >= 1.0 {
        (visor_color(1.0), 0.15 * alpha_mult)
    } else if value > 0.5 {
        (visor_color(1.0), 0.3 * alpha_mult)
    } else if value > 0.2 {
        (warn_color(1.0), 0.5 * alpha_mult)
    } else {
        // Critical: pulsing red
        let pulse = 0.5 + 0.5 * (time * 4.0).sin();
        let a = (0.5 + 0.4 * pulse) * alpha_mult;
        (crit_color(1.0), a)
    };

    let final_color = egui::Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (text_alpha * 255.0).clamp(0.0, 255.0) as u8,
    );

    let align = if left_aligned {
        egui::Align2::LEFT_BOTTOM
    } else {
        egui::Align2::RIGHT_BOTTOM
    };

    let font_size = 14.0 * scale;
    painter.text(pos, align, &text, visor_font(font_size), final_color);
}

/// Target reticle: thin ring at projected position + name/distance text.
fn draw_target_reticle(
    painter: &egui::Painter,
    pos: egui::Pos2,
    name: Option<&str>,
    distance_ly: Option<f64>,
    scale: f32,
    alpha_mult: f32,
) {
    let base = 0.5 * alpha_mult;
    let glow = visor_color(base * 0.3);
    let color = visor_color(base);

    // Glow ring
    painter.circle_stroke(pos, 14.0 * scale, egui::Stroke::new(3.0 * scale, glow));
    // Sharp ring
    painter.circle_stroke(pos, 12.0 * scale, egui::Stroke::new(1.5 * scale, color));

    // Text below
    if let Some(n) = name {
        let dist_str = distance_ly
            .map(|d| format!(" / {d:.1} ly"))
            .unwrap_or_default();
        let label = format!("{n}{dist_str}");
        let text_pos = egui::pos2(pos.x, pos.y + 18.0 * scale);
        let font_size = 10.0 * scale;
        painter.text(
            text_pos,
            egui::Align2::CENTER_TOP,
            label,
            visor_font(font_size),
            visor_color(base * 0.8),
        );
    }
}

/// Edge chevron when target is off-screen.
fn draw_edge_chevron(
    painter: &egui::Painter,
    center: egui::Pos2,
    angle: f32,
    w: f32,
    h: f32,
    scale: f32,
    alpha_mult: f32,
) {
    let margin = 30.0 * scale;
    let half_w = w / 2.0 - margin;
    let half_h = h / 2.0 - margin;

    // Position on screen edge
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Find intersection with screen edge rectangle
    let t_x = if cos_a.abs() > 1e-6 { half_w / cos_a.abs() } else { f32::MAX };
    let t_y = if sin_a.abs() > 1e-6 { half_h / sin_a.abs() } else { f32::MAX };
    let t = t_x.min(t_y);
    let px = center.x + cos_a * t;
    let py = center.y + sin_a * t;

    let color = visor_color(0.5 * alpha_mult);
    let sz = 6.0 * scale;

    // Chevron triangle pointing in direction of angle
    let tip = egui::pos2(px + cos_a * sz, py + sin_a * sz);
    let perp_x = -sin_a;
    let perp_y = cos_a;
    let left = egui::pos2(px - cos_a * sz + perp_x * sz * 0.6, py - sin_a * sz + perp_y * sz * 0.6);
    let right = egui::pos2(px - cos_a * sz - perp_x * sz * 0.6, py - sin_a * sz - perp_y * sz * 0.6);

    let stroke = egui::Stroke::new(2.0 * scale, color);
    painter.line_segment([left, tip], stroke);
    painter.line_segment([right, tip], stroke);
}

/// Warning vignette: red-orange screen edge rects.
fn draw_warning_vignette(painter: &egui::Painter, w: f32, h: f32, min_vital: f32) {
    // Intensity: 0.0 at 0.8, ~1.0 at 0.0
    let intensity = ((0.8 - min_vital) / 0.8).clamp(0.0, 1.0);
    let alpha = (intensity * 60.0) as u8; // subtle: max alpha 60/255
    if alpha == 0 {
        return;
    }
    let vignette_color =
        egui::Color32::from_rgba_premultiplied(CRIT_R / 2, WARN_G / 4, 0, alpha);
    let edge = 60.0 + intensity * 100.0;

    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, edge)),
        0.0, vignette_color,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(0.0, h - edge), egui::vec2(w, edge)),
        0.0, vignette_color,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(edge, h)),
        0.0, vignette_color,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(w - edge, 0.0), egui::vec2(edge, h)),
        0.0, vignette_color,
    );
}

/// Compute visor degradation from suit power level.
/// Returns (flicker_visible, jitter_x, jitter_y, alpha_multiplier).
fn compute_degradation(state: &VisorState) -> (bool, f32, f32, f32) {
    let p = state.suit_power;
    let t = state.time;

    if p <= 0.0 {
        return (false, 0.0, 0.0, 0.0);
    }
    if p > 0.2 {
        return (true, 0.0, 0.0, 1.0);
    }

    // Pseudo-random flicker using sin of time
    let flicker_val = (t * 73.0).sin();
    let jitter_base1 = (t * 137.0).sin();
    let jitter_base2 = (t * 211.0).sin();

    if p > 0.1 {
        // 0.1–0.2: occasional flicker (5% of frames)
        let visible = flicker_val > -0.9; // ~5% invisible
        (visible, 0.0, 0.0, 0.95)
    } else if p > 0.05 {
        // 0.05–0.1: frequent flicker (20%) + jitter 1-3px + alpha 0.7
        let visible = flicker_val > -0.6;
        let jx = jitter_base1 * 2.0;
        let jy = jitter_base2 * 2.0;
        (visible, jx, jy, 0.7)
    } else {
        // 0–0.05: heavy flicker (40%) + jitter 3-5px + alpha 0.4
        let visible = flicker_val > -0.2;
        let jx = jitter_base1 * 4.0;
        let jy = jitter_base2 * 4.0;
        (visible, jx, jy, 0.4)
    }
}
