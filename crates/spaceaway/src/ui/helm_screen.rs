//! Helm monitor layout: speed, throttle, engine state.
//!
//! Rendered via egui to an offscreen texture for display on the
//! cockpit's Speed Display screen mesh.

/// Helm station accent color: blue [0.15, 0.35, 0.65] -> RGB 38, 89, 166.
const HELM_BLUE: egui::Color32 = egui::Color32::from_rgb(38, 89, 166);

/// Data to display on the helm monitor.
pub struct HelmData {
    pub speed: f32,
    pub throttle: f32,
    pub engine_on: bool,
    pub fuel: f32,
}

/// Draw the helm monitor UI. Called within an egui context that targets
/// the offscreen monitor texture.
pub fn draw_helm_screen(ctx: &egui::Context, data: &HelmData) {
    // Set dark background for the monitor
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 13, 20))
        .inner_margin(egui::Margin::same(8));

    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);

            // Title
            ui.label(
                egui::RichText::new("HELM")
                    .color(HELM_BLUE)
                    .size(18.0)
                    .strong(),
            );

            ui.add_space(4.0);

            // Separator line in blue
            let rect = ui.available_rect_before_wrap();
            let y = rect.top();
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 10.0, y),
                    egui::pos2(rect.right() - 10.0, y),
                ],
                egui::Stroke::new(1.0, HELM_BLUE),
            );
            ui.add_space(8.0);

            // Speed (large)
            ui.label(
                egui::RichText::new(format!("{:.1}", data.speed))
                    .color(HELM_BLUE)
                    .size(36.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("m/s")
                    .color(egui::Color32::from_white_alpha(140))
                    .size(12.0),
            );

            ui.add_space(12.0);

            // Throttle
            ui.label(
                egui::RichText::new(format!("THR  {:.0}%", data.throttle * 100.0))
                    .color(egui::Color32::from_white_alpha(200))
                    .size(16.0),
            );

            // Simple throttle bar
            let bar_width = 120.0;
            let bar_height = 8.0;
            let (bar_rect, _) = ui.allocate_exact_size(
                egui::vec2(bar_width, bar_height),
                egui::Sense::hover(),
            );
            // Background
            ui.painter().rect_filled(
                bar_rect,
                2.0,
                egui::Color32::from_rgb(30, 30, 40),
            );
            // Fill
            let fill_width = bar_width * data.throttle;
            if fill_width > 0.5 {
                let fill_rect = egui::Rect::from_min_size(
                    bar_rect.left_top(),
                    egui::vec2(fill_width, bar_height),
                );
                ui.painter().rect_filled(fill_rect, 2.0, HELM_BLUE);
            }

            ui.add_space(12.0);

            // Engine state
            let (engine_text, engine_color) = if data.engine_on {
                ("ENGINE  ON", egui::Color32::from_rgb(50, 200, 80))
            } else {
                ("ENGINE  OFF", egui::Color32::from_rgb(180, 40, 40))
            };
            ui.label(
                egui::RichText::new(engine_text)
                    .color(engine_color)
                    .size(16.0)
                    .strong(),
            );

            ui.add_space(12.0);

            // Fuel gauge
            let fuel_color = if data.fuel < 0.2 {
                egui::Color32::from_rgb(220, 50, 30)
            } else if data.fuel < 0.5 {
                egui::Color32::from_rgb(220, 160, 30)
            } else {
                HELM_BLUE
            };
            ui.label(
                egui::RichText::new(format!("FUEL  {:.0}%", data.fuel * 100.0))
                    .color(fuel_color)
                    .size(14.0),
            );

            // Fuel bar
            let bar_width = 120.0;
            let bar_height = 6.0;
            let (fuel_rect, _) = ui.allocate_exact_size(
                egui::vec2(bar_width, bar_height),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(
                fuel_rect,
                2.0,
                egui::Color32::from_rgb(30, 30, 40),
            );
            let fill_width = bar_width * data.fuel;
            if fill_width > 0.5 {
                let fill_rect = egui::Rect::from_min_size(
                    fuel_rect.left_top(),
                    egui::vec2(fill_width, bar_height),
                );
                ui.painter().rect_filled(fill_rect, 2.0, fuel_color);
            }
        });
    });
}
