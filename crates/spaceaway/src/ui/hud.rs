//! HUD overlay: context-sensitive crosshair icons.
//!
//! Draws a small icon at screen center that changes based on what the player
//! is looking at. Pure geometric shapes via egui's Painter API.

use super::HudState;
use sa_ship::InteractableKind;

/// Draw the HUD overlay with context-sensitive crosshair.
pub fn draw_hud(ctx: &egui::Context, state: &HudState) {
    let center = egui::pos2(
        state.screen_width as f32 / 2.0,
        state.screen_height as f32 / 2.0,
    );

    egui::Area::new(egui::Id::new("hud_crosshair"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
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
        });
}

/// Default crosshair: tiny dot.
fn draw_dot(painter: &egui::Painter, center: egui::Pos2) {
    painter.circle_filled(center, 2.0, egui::Color32::from_white_alpha(100));
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
