//! Application styling for craft-ui's accessible icon controls.
use craft_ui::buttons::{ButtonVisuals, IconButtonStyle};
use eframe::egui;

/// Flat toolbar controls with a persistent selected surface and keyboard focus.
pub fn toolbar_style(ui: &egui::Ui) -> IconButtonStyle {
    let widgets = &ui.visuals().widgets;
    let state = |visuals: &egui::style::WidgetVisuals| ButtonVisuals {
        fill: visuals.bg_fill,
        stroke: egui::Stroke::NONE,
        icon: visuals.fg_stroke.color,
    };
    IconButtonStyle {
        normal: ButtonVisuals {
            fill: egui::Color32::TRANSPARENT,
            ..state(&widgets.inactive)
        },
        hovered: state(&widgets.hovered),
        pressed: Some(state(&widgets.active)),
        selected: ButtonVisuals {
            fill: ui.visuals().selection.bg_fill,
            stroke: egui::Stroke::NONE,
            icon: ui.visuals().selection.stroke.color,
        },
        corner_radius: widgets.inactive.corner_radius,
        focus_stroke: widgets.active.bg_stroke,
    }
}

/// Preserve the shell's unframed icon appearance while sharing pointer,
/// keyboard, accessibility and disabled-state behavior with craft-ui.
pub fn unframed_style(ui: &egui::Ui) -> IconButtonStyle {
    let widgets = &ui.visuals().widgets;
    let state = |visuals: &egui::style::WidgetVisuals| ButtonVisuals {
        fill: egui::Color32::TRANSPARENT,
        stroke: egui::Stroke::NONE,
        icon: visuals.fg_stroke.color,
    };
    IconButtonStyle {
        normal: state(&widgets.inactive),
        hovered: state(&widgets.hovered),
        pressed: Some(state(&widgets.active)),
        selected: state(&widgets.inactive),
        corner_radius: widgets.inactive.corner_radius,
        focus_stroke: widgets.hovered.bg_stroke,
    }
}
