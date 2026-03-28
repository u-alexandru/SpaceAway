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
    pub drive_mode: sa_ship::DriveMode,
    pub drive_status: sa_ship::DriveStatus,
    pub drive_speed_c: f64,
    pub exotic_fuel: f32,
    /// Solar system overview lines (if in a system).
    pub system_info: Option<Vec<String>>,
    /// Navigation target: (name, distance_ly, eta_seconds).
    pub target_info: Option<(String, f64, f64)>,
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

            ui.add_space(8.0);

            // --- Drive mode section ---
            let (drive_label, drive_color) = match data.drive_mode {
                sa_ship::DriveMode::Impulse => ("IMPULSE", egui::Color32::from_rgb(120, 120, 140)),
                sa_ship::DriveMode::Cruise => ("CRUISE", egui::Color32::from_rgb(40, 200, 220)),
                sa_ship::DriveMode::Warp => ("WARP", egui::Color32::from_rgb(200, 80, 220)),
            };

            let status_text = match data.drive_status {
                sa_ship::DriveStatus::Idle => String::new(),
                sa_ship::DriveStatus::Spooling(p) => format!("  SPOOL {:.0}%", p / sa_ship::drive::WARP_SPOOL_TIME * 100.0),
                sa_ship::DriveStatus::Engaged => {
                    if data.drive_speed_c > 1000.0 {
                        format!("  {:.0}kc", data.drive_speed_c / 1000.0)
                    } else if data.drive_speed_c > 1.0 {
                        format!("  {:.0}c", data.drive_speed_c)
                    } else {
                        String::new()
                    }
                }
            };

            ui.label(
                egui::RichText::new(format!("{drive_label}{status_text}"))
                    .color(drive_color)
                    .size(14.0)
                    .strong(),
            );

            // Exotic fuel gauge (only show if not at 100%)
            if data.exotic_fuel < 0.999 {
                let exotic_color = if data.exotic_fuel < 0.15 {
                    egui::Color32::from_rgb(220, 50, 30)
                } else {
                    egui::Color32::from_rgb(200, 80, 220)
                };
                ui.label(
                    egui::RichText::new(format!("EXOTIC  {:.0}%", data.exotic_fuel * 100.0))
                        .color(exotic_color)
                        .size(12.0),
                );
                let bar_width = 120.0;
                let bar_height = 5.0;
                let (exotic_rect, _) = ui.allocate_exact_size(
                    egui::vec2(bar_width, bar_height),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    exotic_rect, 2.0, egui::Color32::from_rgb(30, 30, 40),
                );
                let ew = bar_width * data.exotic_fuel;
                if ew > 0.5 {
                    let fill = egui::Rect::from_min_size(
                        exotic_rect.left_top(), egui::vec2(ew, bar_height),
                    );
                    ui.painter().rect_filled(fill, 2.0, exotic_color);
                }
            }

            // --- Navigation target ---
            if let Some((ref name, dist, eta)) = data.target_info {
                ui.add_space(8.0);
                let eta_str = if eta.is_infinite() {
                    "---".to_string()
                } else if eta > 3600.0 {
                    format!("{:.1}h", eta / 3600.0)
                } else if eta > 60.0 {
                    format!("{:.0}m", eta / 60.0)
                } else {
                    format!("{:.0}s", eta)
                };
                ui.label(
                    egui::RichText::new(format!("TGT {name}"))
                        .color(egui::Color32::from_rgb(40, 200, 220))
                        .size(11.0),
                );
                ui.label(
                    egui::RichText::new(format!("{:.2} ly  ETA {eta_str}", dist))
                        .color(egui::Color32::from_rgb(40, 200, 220))
                        .size(11.0),
                );
            }

            // --- Solar system overview ---
            if let Some(ref bodies) = data.system_info {
                ui.add_space(8.0);
                // Separator
                let rect = ui.available_rect_before_wrap();
                let y = rect.top();
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() + 10.0, y),
                        egui::pos2(rect.right() - 10.0, y),
                    ],
                    egui::Stroke::new(1.0, HELM_BLUE),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("SYSTEM")
                        .color(HELM_BLUE)
                        .size(14.0)
                        .strong(),
                );
                for line in bodies {
                    ui.label(
                        egui::RichText::new(line)
                            .color(egui::Color32::from_white_alpha(180))
                            .size(10.0),
                    );
                }
            }
        });
    });
}
