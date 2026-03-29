//! HUD overlay: context-sensitive crosshair icons, warning bars, vignette.
#![allow(dead_code)]
//!
//! Draws a small icon at screen center that changes based on what the player
//! is looking at. Also renders fuel/O2 warning bars and low-oxygen vignette.
//! Pure geometric shapes via egui's Painter API.

use super::HudState;
use sa_ship::InteractableKind;

/// Draw the HUD overlay with crosshair, warning bars, and vignette effects.
pub fn draw_hud(ctx: &egui::Context, state: &HudState) {
    let center = egui::pos2(
        state.screen_width as f32 / 2.0,
        state.screen_height as f32 / 2.0,
    );
    let w = state.screen_width as f32;
    let h = state.screen_height as f32;

    egui::Area::new(egui::Id::new("hud_crosshair"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();

            // --- Crosshair ---
            if state.gather_available {
                draw_gather(painter, center);
            } else {
                match &state.hovered_kind {
                    None => draw_dot(painter, center),
                    Some(kind) => match kind {
                        InteractableKind::Lever { .. } => draw_grab(painter, center),
                        InteractableKind::Button { .. } => draw_press(painter, center),
                        InteractableKind::Switch { .. } => draw_press(painter, center),
                        InteractableKind::HelmSeat => draw_sit(painter, center),
                        InteractableKind::Screen { .. } => draw_eye(painter, center),
                    },
                }
            }

            // --- Fuel warning bar (bottom-left, visible when <50%) ---
            if state.fuel < 0.5 {
                let bar_width = 200.0;
                let bar_height = 6.0;
                let margin = 20.0;
                let bar_x = margin;
                let bar_y = h - margin - bar_height;

                let fuel_color = if state.fuel < 0.2 {
                    egui::Color32::from_rgb(220, 50, 30)
                } else {
                    egui::Color32::from_rgb(220, 160, 30)
                };

                // Background
                let bg_rect = egui::Rect::from_min_size(
                    egui::pos2(bar_x, bar_y),
                    egui::vec2(bar_width, bar_height),
                );
                painter.rect_filled(bg_rect, 2.0, egui::Color32::from_rgba_premultiplied(20, 20, 30, 150));

                // Fill
                let fill_w = bar_width * state.fuel;
                if fill_w > 0.5 {
                    let fill_rect = egui::Rect::from_min_size(
                        egui::pos2(bar_x, bar_y),
                        egui::vec2(fill_w, bar_height),
                    );
                    painter.rect_filled(fill_rect, 2.0, fuel_color);
                }

                // Label
                painter.text(
                    egui::pos2(bar_x, bar_y - 14.0),
                    egui::Align2::LEFT_TOP,
                    format!("FUEL {:.0}%", state.fuel * 100.0),
                    egui::FontId::proportional(11.0),
                    fuel_color,
                );
            }

            // --- O2 warning bar (bottom-right, visible when <80%) ---
            if state.oxygen < 0.8 {
                let bar_width = 200.0;
                let bar_height = 6.0;
                let margin = 20.0;
                let bar_x = w - margin - bar_width;
                let bar_y = h - margin - bar_height;

                let o2_color = if state.oxygen < 0.3 {
                    egui::Color32::from_rgb(220, 50, 30)
                } else {
                    egui::Color32::from_rgb(80, 140, 220)
                };

                // Background
                let bg_rect = egui::Rect::from_min_size(
                    egui::pos2(bar_x, bar_y),
                    egui::vec2(bar_width, bar_height),
                );
                painter.rect_filled(bg_rect, 2.0, egui::Color32::from_rgba_premultiplied(20, 20, 30, 150));

                // Fill
                let fill_w = bar_width * state.oxygen;
                if fill_w > 0.5 {
                    let fill_rect = egui::Rect::from_min_size(
                        egui::pos2(bar_x, bar_y),
                        egui::vec2(fill_w, bar_height),
                    );
                    painter.rect_filled(fill_rect, 2.0, o2_color);
                }

                // Label
                painter.text(
                    egui::pos2(bar_x, bar_y - 14.0),
                    egui::Align2::LEFT_TOP,
                    format!("O2 {:.0}%", state.oxygen * 100.0),
                    egui::FontId::proportional(11.0),
                    o2_color,
                );
            }

            // --- Low O2 vignette (<30%) ---
            if state.oxygen < 0.3 {
                let intensity = ((0.3 - state.oxygen) / 0.3).clamp(0.0, 1.0);
                let alpha = (intensity * 200.0) as u8;
                let vignette_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, alpha);
                let edge_width = 80.0 + intensity * 120.0; // 80-200px edge darkening

                // Top edge
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, edge_width)),
                    0.0,
                    vignette_color,
                );
                // Bottom edge
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(0.0, h - edge_width), egui::vec2(w, edge_width)),
                    0.0,
                    vignette_color,
                );
                // Left edge
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(edge_width, h)),
                    0.0,
                    vignette_color,
                );
                // Right edge
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(w - edge_width, 0.0), egui::vec2(edge_width, h)),
                    0.0,
                    vignette_color,
                );
            }

            // --- Life support failure (O2 = 0) ---
            if state.oxygen <= 0.0 {
                // Full screen dark overlay
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(w, h)),
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 230),
                );
                // Failure text
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "LIFE SUPPORT FAILURE",
                    egui::FontId::proportional(32.0),
                    egui::Color32::from_rgb(220, 40, 30),
                );
            }
        });
}

/// Default crosshair: tiny dot.
fn draw_dot(painter: &egui::Painter, center: egui::Pos2) {
    painter.circle_filled(center, 2.0, egui::Color32::from_white_alpha(100));
}

/// Gather icon: diamond shape (resource pickup available).
fn draw_gather(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_rgb(80, 220, 120);
    let stroke = egui::Stroke::new(2.0, color);
    let size = 10.0;
    // Diamond shape
    let points = [
        egui::pos2(center.x, center.y - size),
        egui::pos2(center.x + size, center.y),
        egui::pos2(center.x, center.y + size),
        egui::pos2(center.x - size, center.y),
        egui::pos2(center.x, center.y - size),
    ];
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
    // Inner dot
    painter.circle_filled(center, 3.0, color);
}

/// Grab icon for levers: two vertical parallel bars.
fn draw_grab(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_white_alpha(220);
    let stroke = egui::Stroke::new(2.0, color);
    let gap = 4.0;
    let half_h = 8.0;
    // Left bar
    painter.line_segment(
        [
            egui::pos2(center.x - gap, center.y - half_h),
            egui::pos2(center.x - gap, center.y + half_h),
        ],
        stroke,
    );
    // Right bar
    painter.line_segment(
        [
            egui::pos2(center.x + gap, center.y - half_h),
            egui::pos2(center.x + gap, center.y + half_h),
        ],
        stroke,
    );
    // Small horizontal connectors at top and bottom
    painter.line_segment(
        [
            egui::pos2(center.x - gap, center.y - half_h),
            egui::pos2(center.x + gap, center.y - half_h),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(120)),
    );
    painter.line_segment(
        [
            egui::pos2(center.x - gap, center.y + half_h),
            egui::pos2(center.x + gap, center.y + half_h),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(120)),
    );
}

/// Press icon for buttons/switches: circle with inner dot.
fn draw_press(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_white_alpha(220);
    let stroke = egui::Stroke::new(1.5, color);
    painter.circle_stroke(center, 8.0, stroke);
    painter.circle_filled(center, 3.0, color);
}

/// Sit icon for helm seat: downward chevron (like a chair seat).
fn draw_sit(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_white_alpha(220);
    let stroke = egui::Stroke::new(2.0, color);
    let w = 8.0;
    let h = 6.0;
    // V shape (seat)
    painter.line_segment(
        [
            egui::pos2(center.x - w, center.y - h),
            egui::pos2(center.x, center.y + h),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center.x + w, center.y - h),
            egui::pos2(center.x, center.y + h),
        ],
        stroke,
    );
    // Back rest (small vertical line at center top)
    painter.line_segment(
        [
            egui::pos2(center.x, center.y - h - 2.0),
            egui::pos2(center.x, center.y - h + 2.0),
        ],
        egui::Stroke::new(1.5, egui::Color32::from_white_alpha(150)),
    );
}

/// Eye icon for screens: oval outline with center dot.
fn draw_eye(painter: &egui::Painter, center: egui::Pos2) {
    let color = egui::Color32::from_white_alpha(220);
    let stroke = egui::Stroke::new(1.5, color);
    // Approximate an eye shape with two arcs (top and bottom eyelid).
    // We draw a diamond/eye shape using line segments.
    let w = 10.0;
    let h = 5.0;
    let points = [
        egui::pos2(center.x - w, center.y),
        egui::pos2(center.x - w * 0.5, center.y - h),
        egui::pos2(center.x, center.y - h * 1.1),
        egui::pos2(center.x + w * 0.5, center.y - h),
        egui::pos2(center.x + w, center.y),
        egui::pos2(center.x + w * 0.5, center.y + h),
        egui::pos2(center.x, center.y + h * 1.1),
        egui::pos2(center.x - w * 0.5, center.y + h),
        egui::pos2(center.x - w, center.y),
    ];
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
    // Pupil
    painter.circle_filled(center, 3.0, color);
}
