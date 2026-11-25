//! Shared heading and close control for app-owned modal dialogs.
use eframe::egui;

pub fn header(ui: &mut egui::Ui, title: &str, close_enabled: bool) -> bool {
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(close_enabled, |ui| {
                let style = crate::icon_button::unframed_style(ui);
                let response = craft_ui::buttons::IconButton::new(
                    "Close dialog",
                    egui::vec2(24.0, 24.0),
                    &style,
                )
                .show(ui, |ui, rect, color| {
                    let icon = rect.shrink(7.0);
                    let stroke = egui::Stroke::new(1.0, color);
                    ui.painter()
                        .line_segment([icon.left_top(), icon.right_bottom()], stroke);
                    ui.painter()
                        .line_segment([icon.right_top(), icon.left_bottom()], stroke);
                })
                .on_hover_text("Close dialog");
                response.clicked()
            })
            .inner
        })
        .inner
    })
    .inner
}
