//! Helm monitor layout: compact, information-dense cockpit display.
//!
//! Rendered via egui to an offscreen 512×512 texture for display on the
//! cockpit's Speed Display screen mesh.

/// Helm station accent color.
const HELM_BLUE: egui::Color32 = egui::Color32::from_rgb(38, 89, 166);
const DIM: egui::Color32 = egui::Color32::from_rgb(100, 100, 120);
const BRIGHT: egui::Color32 = egui::Color32::from_rgb(200, 200, 220);
const GREEN: egui::Color32 = egui::Color32::from_rgb(50, 200, 80);
const RED: egui::Color32 = egui::Color32::from_rgb(220, 50, 30);
const AMBER: egui::Color32 = egui::Color32::from_rgb(220, 160, 30);
const CYAN: egui::Color32 = egui::Color32::from_rgb(40, 200, 220);

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
    pub system_info: Option<Vec<String>>,
    pub target_info: Option<(String, f64, f64)>,
    pub altitude_m: Option<f32>,
    pub planet_dist_km: Option<f32>,
}

/// Draw the helm monitor UI.
pub fn draw_helm_screen(ctx: &egui::Context, data: &HelmData) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(10, 10, 16))
        .inner_margin(egui::Margin::same(12));

    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);

        // ── Row 1: SURF/ALT distance (most important during approach) ──
        if let Some(dist_km) = data.planet_dist_km {
            let (label, value, color) = if let Some(alt) = data.altitude_m {
                let c = if alt < 50.0 { RED } else if alt < 200.0 { AMBER } else { CYAN };
                if alt < 1000.0 {
                    ("ALT", format!("{:.0}m", alt), c)
                } else {
                    ("ALT", format!("{:.1}km", alt / 1000.0), c)
                }
            } else if dist_km < 10.0 {
                ("SURF", format!("{:.1}km", dist_km), AMBER)
            } else {
                ("SURF", format!("{:.0}km", dist_km), CYAN)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).color(DIM).size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(value).color(color).size(22.0).strong());
                });
            });
            separator(ui);
        }

        // ── Row 2: Speed (large, central) ──
        let (speed_text, speed_unit) = format_speed(data);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(&speed_text).color(BRIGHT).size(38.0).strong());
            ui.label(egui::RichText::new(speed_unit).color(DIM).size(11.0));
        });

        ui.add_space(4.0);

        // ── Row 3: Throttle bar ──
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("THR {:.0}%", data.throttle * 100.0)).color(DIM).size(12.0));
        });
        draw_bar(ui, data.throttle, HELM_BLUE);

        ui.add_space(4.0);

        // ── Row 4: Engine + Drive (single row) ──
        ui.horizontal(|ui| {
            let (eng_text, eng_color) = if data.engine_on { ("ENG ON", GREEN) } else { ("ENG OFF", RED) };
            ui.label(egui::RichText::new(eng_text).color(eng_color).size(12.0).strong());

            ui.add_space(12.0);

            let (drive_label, drive_color) = match data.drive_mode {
                sa_ship::DriveMode::Impulse => ("IMP", DIM),
                sa_ship::DriveMode::Cruise => ("CRU", CYAN),
                sa_ship::DriveMode::Warp => ("WRP", egui::Color32::from_rgb(200, 80, 220)),
            };
            let status = match data.drive_status {
                sa_ship::DriveStatus::Idle => String::new(),
                sa_ship::DriveStatus::Spooling(p) => {
                    format!(" {:.0}%", p / sa_ship::drive::WARP_SPOOL_TIME * 100.0)
                }
                sa_ship::DriveStatus::Engaged => {
                    if data.drive_speed_c > 1000.0 {
                        format!(" {:.0}kc", data.drive_speed_c / 1000.0)
                    } else if data.drive_speed_c > 1.0 {
                        format!(" {:.0}c", data.drive_speed_c)
                    } else {
                        String::new()
                    }
                }
            };
            ui.label(egui::RichText::new(format!("{drive_label}{status}")).color(drive_color).size(12.0).strong());
        });

        ui.add_space(4.0);

        // ── Row 5: Fuel bars (compact, side by side) ──
        ui.horizontal(|ui| {
            let fuel_color = if data.fuel < 0.2 { RED } else if data.fuel < 0.5 { AMBER } else { HELM_BLUE };
            ui.label(egui::RichText::new(format!("H₂ {:.0}%", data.fuel * 100.0)).color(fuel_color).size(11.0));
            if data.exotic_fuel < 0.999 {
                ui.add_space(8.0);
                let ex_color = if data.exotic_fuel < 0.15 { RED } else { egui::Color32::from_rgb(200, 80, 220) };
                ui.label(egui::RichText::new(format!("EX {:.0}%", data.exotic_fuel * 100.0)).color(ex_color).size(11.0));
            }
        });
        draw_bar(ui, data.fuel, if data.fuel < 0.2 { RED } else { HELM_BLUE });

        separator(ui);

        // ── Row 6: Navigation target (if locked) ──
        if let Some((ref name, dist, eta)) = data.target_info {
            let eta_str = if eta.is_infinite() {
                "---".to_string()
            } else if eta > 3600.0 {
                format!("{:.1}h", eta / 3600.0)
            } else if eta > 60.0 {
                format!("{:.0}m", eta / 60.0)
            } else {
                format!("{:.0}s", eta)
            };
            ui.label(egui::RichText::new(format!("▸ {name}")).color(CYAN).size(11.0));
            ui.label(
                egui::RichText::new(format!("  {} ETA {eta_str}", super::visor::format_distance_ly(dist)))
                    .color(CYAN).size(10.0),
            );
            ui.add_space(2.0);
        }

        // ── Row 7: System bodies (compact list) ──
        if let Some(ref bodies) = data.system_info {
            separator(ui);
            for line in bodies.iter().take(6) {
                ui.label(egui::RichText::new(line).color(DIM).size(9.0));
            }
        }
    });
}

// ── Helpers ──

fn format_speed(data: &HelmData) -> (String, &'static str) {
    match data.drive_mode {
        sa_ship::DriveMode::Warp if matches!(data.drive_status, sa_ship::DriveStatus::Engaged) => {
            let ly_s = data.drive_speed_c * 3.169e-8;
            (format!("{:.3}", ly_s), "ly/s")
        }
        sa_ship::DriveMode::Cruise if matches!(data.drive_status, sa_ship::DriveStatus::Engaged) => {
            if data.drive_speed_c >= 10.0 {
                (format!("{:.0}", data.drive_speed_c), "c")
            } else {
                (format!("{:.1}", data.drive_speed_c), "c")
            }
        }
        _ => (format!("{:.1}", data.speed), "m/s"),
    }
}

fn draw_bar(ui: &mut egui::Ui, fraction: f32, fill_color: egui::Color32) {
    let width = ui.available_width().min(200.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, egui::Color32::from_rgb(25, 25, 35));
    let fw = width * fraction.clamp(0.0, 1.0);
    if fw > 0.5 {
        let fill = egui::Rect::from_min_size(rect.left_top(), egui::vec2(fw, 4.0));
        ui.painter().rect_filled(fill, 1.0, fill_color);
    }
}

fn separator(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [egui::pos2(rect.left() + 4.0, rect.top()), egui::pos2(rect.right() - 4.0, rect.top())],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 35, 50)),
    );
    ui.add_space(3.0);
}
