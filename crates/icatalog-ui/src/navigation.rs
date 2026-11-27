//! Folder-tree presentation. Expansion, selection and filtering stay in the shell.
use eframe::egui;

pub const DISCLOSURE_WIDTH: f32 = 18.0;
pub const INDENT: f32 = 14.0;

#[derive(Clone, Copy)]
pub enum Icon {
    Library,
    Unassigned,
    Device,
    Folder { open: bool },
}

/// An unboxed chevron with a full-height keyboard and pointer target.
pub fn disclosure(ui: &mut egui::Ui, open: bool, name: &str) -> egui::Response {
    let label = format!("{} {name}", if open { "Collapse" } else { "Expand" });
    let mut style = crate::icon_button::toolbar_style(ui);
    style.normal.icon = ui.visuals().weak_text_color();
    let response = craft_ui::buttons::IconButton::new(
        &label,
        egui::vec2(DISCLOSURE_WIDTH, ui.spacing().interact_size.y),
        &style,
    )
    .show(ui, |ui, rect, color| {
        let points = if open {
            [
                egui::vec2(-3.5, -2.0),
                egui::vec2(0.0, 1.5),
                egui::vec2(3.5, -2.0),
            ]
        } else {
            [
                egui::vec2(-1.5, -3.5),
                egui::vec2(2.0, 0.0),
                egui::vec2(-1.5, 3.5),
            ]
        };
        ui.painter().add(egui::Shape::line(
            points
                .into_iter()
                .map(|offset| rect.center() + offset)
                .collect(),
            egui::Stroke::new(1.5, color),
        ));
    });
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_expanded(open));
    response.on_hover_text(label)
}

/// Leaves and branches reserve the same disclosure column.
pub fn disclosure_space(ui: &mut egui::Ui) {
    ui.allocate_exact_size(
        egui::vec2(DISCLOSURE_WIDTH, ui.spacing().interact_size.y),
        egui::Sense::hover(),
    );
}

/// The icon belongs to the selectable row, so the label and its right edge keep
/// one click target. Custom atoms are decorative and add no screen-reader label.
pub fn row(
    ui: &mut egui::Ui,
    selected: bool,
    name: &str,
    icon: Icon,
    strong: bool,
) -> egui::Response {
    let mut classes = egui::class::Classes::default();
    if selected {
        egui::class::HasClasses::add_class(&mut classes, egui::Button::CLASS_SELECTED);
    }
    let color = ui
        .widget_style::<egui::widget_style::ButtonStyle>(ui.next_auto_id(), &classes)
        .atom_layout
        .text_style
        .color;
    let icon_id = egui::IdSalt::new("navigation_icon");
    let label = egui::RichText::new(name);
    let label = if strong { label.strong() } else { label };
    let row = egui::Button::selectable(
        selected,
        (
            egui::Atom::custom(icon_id, egui::vec2(16.0, 16.0)),
            label,
            egui::Atom::grow(),
        ),
    )
    .min_size(egui::vec2(
        ui.available_width(),
        ui.spacing().interact_size.y,
    ))
    .truncate()
    .atom_ui(ui);
    if let Some(rect) = row.rect(icon_id) {
        paint_icon(ui, rect, icon, color);
    }
    row.response.on_hover_text(name)
}

/// Original monochrome vector marks, drawn in logical points at native DPI.
pub fn paint_icon(ui: &egui::Ui, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let origin = rect.center() - egui::vec2(8.0, 8.0);
    let point = |x, y| origin + egui::vec2(x, y);
    let stroke = egui::Stroke::new(1.2, color);
    let path = |points: &[(f32, f32)]| {
        ui.painter().add(egui::Shape::line(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            stroke,
        ));
    };
    match icon {
        Icon::Folder { open } => {
            path(&[
                (1.5, 13.0),
                (1.5, 3.0),
                (6.0, 3.0),
                (8.0, 5.0),
                (14.5, 5.0),
                (14.5, 7.0),
            ]);
            if open {
                path(&[
                    (1.5, 13.0),
                    (4.0, 7.0),
                    (15.0, 7.0),
                    (12.5, 13.0),
                    (1.5, 13.0),
                ]);
            } else {
                path(&[(14.5, 7.0), (14.5, 13.0), (1.5, 13.0)]);
            }
        }
        Icon::Device => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(point(1.5, 2.5), point(14.5, 11.5)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            path(&[(8.0, 11.5), (8.0, 14.0)]);
            path(&[(4.5, 14.0), (11.5, 14.0)]);
        }
        Icon::Library => {
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(point(1.5, 1.5), point(14.5, 14.5)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            path(&[(8.0, 2.0), (8.0, 14.0)]);
            path(&[(2.0, 8.0), (14.0, 8.0)]);
        }
        Icon::Unassigned => {
            path(&[
                (1.5, 9.0),
                (4.0, 3.5),
                (12.0, 3.5),
                (14.5, 9.0),
                (14.5, 13.0),
                (1.5, 13.0),
                (1.5, 9.0),
                (5.5, 9.0),
                (6.5, 11.0),
                (9.5, 11.0),
                (10.5, 9.0),
                (14.5, 9.0),
            ]);
        }
    }
}
