//! HUD overlay: context-sensitive crosshair icons.

use super::HudState;

/// Draw the HUD overlay. Currently just a test label.
pub fn draw_hud(ctx: &egui::Context, _state: &HudState) {
    egui::Area::new(egui::Id::new("hud_test"))
        .fixed_pos(egui::pos2(10.0, 10.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("SpaceAway")
                    .color(egui::Color32::from_white_alpha(180))
                    .size(14.0),
            );
        });
}
