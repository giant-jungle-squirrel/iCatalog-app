//! Joined search input and submit button; the caller owns filtering.
use eframe::egui;

pub fn show(ui: &mut egui::Ui, search: &mut String) -> bool {
    let height = ui.spacing().interact_size.y.max(24.0);
    let width = ui.available_width().min(254.0);
    let mut submit = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let id = ui.make_persistent_id("search_input");
                // Resolve the normal egui field frame, including its current
                // hover/focus state, then join its right corners to Go.
                let mut frame = ui
                    .widget_style::<egui::widget_style::TextEditStyle>(id, &Default::default())
                    .atom_layout
                    .frame;
                frame.corner_radius = egui::CornerRadius {
                    nw: 3,
                    sw: 3,
                    ne: 0,
                    se: 0,
                };
                // Reserve the clear target inside the frame so long text and
                // its caret scroll before reaching the cross.
                frame.inner_margin = egui::Margin {
                    left: 5,
                    right: 25,
                    top: 2,
                    bottom: 2,
                };
                let response = ui.add(
                    egui::TextEdit::singleline(search)
                        .id(id)
                        .hint_text("Search name or path")
                        .frame(frame)
                        .min_size(egui::vec2(0.0, height))
                        .desired_width((width - 34.0).max(24.0)),
                );
                response.widget_info(|| {
                    let mut info = egui::WidgetInfo::text_edit(
                        ui.is_enabled(),
                        search.as_str(),
                        search.as_str(),
                        "Search name or path",
                    );
                    info.label = Some("Search name or path".into());
                    info
                });
                submit |= response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if !search.is_empty() {
                    let rect = egui::Rect::from_center_size(
                        egui::pos2(response.rect.right() - 13.0, response.rect.center().y),
                        egui::vec2(20.0, 20.0),
                    );
                    // A separate child places the button over the reserved
                    // padding without advancing the toolbar's layout cursor.
                    let mut clear_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("search_clear")
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    let style = crate::icon_button::toolbar_style(&clear_ui);
                    let clear =
                        craft_ui::buttons::IconButton::new("Clear search", rect.size(), &style)
                            .show(&mut clear_ui, |ui, rect, color| {
                                let icon = rect.shrink(6.0);
                                let stroke = egui::Stroke::new(1.3, color);
                                ui.painter()
                                    .line_segment([icon.left_top(), icon.right_bottom()], stroke);
                                ui.painter()
                                    .line_segment([icon.right_top(), icon.left_bottom()], stroke);
                            })
                            .on_hover_text("Clear search")
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if clear.clicked() {
                        search.clear();
                        response.request_focus();
                        submit = true;
                    }
                }
                submit |= ui
                    .add_sized(
                        egui::vec2(34.0, height),
                        egui::Button::new("Go").corner_radius(egui::CornerRadius {
                            nw: 0,
                            sw: 0,
                            ne: 3,
                            se: 3,
                        }),
                    )
                    .clicked();
            },
        );
    });
    submit
}
