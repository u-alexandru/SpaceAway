//! Sensors monitor layout: nearby resource deposits.
//!
//! Rendered via egui to an offscreen texture for display on the
//! cockpit's sensors station screen mesh.

/// Sensors station accent color: purple.
const SENSORS_PURPLE: egui::Color32 = egui::Color32::from_rgb(140, 60, 200);

/// A single contact on the sensors display.
pub struct SensorContact {
    /// Resource type label (e.g. "Fuel Asteroid").
    pub label: String,
    /// Icon character for the type.
    pub icon: String,
    /// Distance in meters from the ship.
    pub distance: f32,
    /// Whether this deposit has already been gathered.
    pub gathered: bool,
}

/// Data to display on the sensors monitor.
pub struct SensorsData {
    pub contacts: Vec<SensorContact>,
    pub ship_fuel: f32,
}

/// Draw the sensors monitor UI. Called within an egui context that targets
/// the offscreen sensors texture.
pub fn draw_sensors_screen(ctx: &egui::Context, data: &SensorsData) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 13, 20))
        .inner_margin(egui::Margin::same(8));

    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);

            // Title
            ui.label(
                egui::RichText::new("SENSORS")
                    .color(SENSORS_PURPLE)
                    .size(16.0)
                    .strong(),
            );

            ui.add_space(2.0);

            // Separator
            let rect = ui.available_rect_before_wrap();
            let y = rect.top();
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 10.0, y),
                    egui::pos2(rect.right() - 10.0, y),
                ],
                egui::Stroke::new(1.0, SENSORS_PURPLE),
            );
            ui.add_space(6.0);

            // Contact list (sorted by distance, closest first)
            let mut sorted: Vec<_> = data.contacts.iter().enumerate().collect();
            sorted.sort_by(|a, b| a.1.distance.partial_cmp(&b.1.distance).unwrap());

            let active_count = sorted.iter().filter(|(_, c)| !c.gathered).count();

            for (_idx, contact) in &sorted {
                if contact.gathered {
                    continue;
                }

                let dist_text = if contact.distance >= 1000.0 {
                    format!("{:.1}km", contact.distance / 1000.0)
                } else {
                    format!("{:.0}m", contact.distance)
                };

                let color = if contact.distance < 500.0 {
                    egui::Color32::from_rgb(80, 220, 120) // green = in range
                } else {
                    egui::Color32::from_white_alpha(180)
                };

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&contact.icon)
                            .color(SENSORS_PURPLE)
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(&contact.label)
                            .color(color)
                            .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(dist_text)
                                .color(color)
                                .size(11.0),
                        );
                    });
                });
            }

            ui.add_space(8.0);

            // Contact count
            ui.label(
                egui::RichText::new(format!("[{} contacts]", active_count))
                    .color(egui::Color32::from_white_alpha(100))
                    .size(10.0),
            );

            ui.add_space(8.0);

            // Low fuel warning on sensors
            if data.ship_fuel < 0.2 {
                ui.label(
                    egui::RichText::new("!! LOW FUEL !!")
                        .color(egui::Color32::from_rgb(255, 80, 40))
                        .size(12.0)
                        .strong(),
                );
            }
        });
    });
}
