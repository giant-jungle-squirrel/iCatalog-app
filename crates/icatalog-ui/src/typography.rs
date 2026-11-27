//! Neutral desktop surfaces and readable typography for the catalogue workspace.
//! Logical sizes remain independent of monitor DPI; eframe supplies native scale.
use craft_ui::theme::WidgetPalette;
use eframe::egui;

/// Keep the viewing surface distinct from the surrounding panels in either theme.
pub(crate) fn canvas_fill(visuals: &egui::Visuals) -> egui::Color32 {
    egui::Color32::from_gray(if visuals.dark_mode { 28 } else { 207 })
}

/// A quiet backing for thumbnails, without tinting the user's artwork.
pub(crate) fn tile_fill(visuals: &egui::Visuals) -> egui::Color32 {
    egui::Color32::from_gray(if visuals.dark_mode { 36 } else { 218 })
}

struct Palette {
    chrome: u8,
    field: u8,
    button: u8,
    hover: u8,
    pressed: u8,
    text: u8,
    secondary: u8,
    border: u8,
    hover_border: u8,
    focus: u8,
    selection: u8,
    selected_text: u8,
}

impl Palette {
    fn for_theme(dark: bool) -> Self {
        if dark {
            Self {
                chrome: 45,
                field: 34,
                button: 36,
                hover: 58,
                pressed: 63,
                text: 226,
                secondary: 188,
                border: 76,
                hover_border: 128,
                focus: 188,
                selection: 85,
                selected_text: 245,
            }
        } else {
            Self {
                chrome: 230,
                field: 246,
                button: 242,
                hover: 210,
                pressed: 196,
                text: 28,
                secondary: 56,
                border: 176,
                hover_border: 130,
                focus: 92,
                selection: 180,
                selected_text: 20,
            }
        }
    }
}

pub fn configure(ctx: &egui::Context) {
    // Foreign filenames may contain characters unavailable on this machine.
    // Render a placeholder instead of treating missing font coverage as a crash.
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    ctx.all_styles_mut(|style| {
        for (role, size) in [
            (egui::TextStyle::Body, 15.0),
            (egui::TextStyle::Button, 15.0),
            (egui::TextStyle::Small, 13.0),
            (egui::TextStyle::Monospace, 14.0),
            (egui::TextStyle::Heading, 20.0),
        ] {
            if let Some(font) = style.text_styles.get_mut(&role) {
                font.size = size;
            }
        }
        style.spacing.interact_size.y = 24.0;
        // Keep the established catalogue browsing density across egui upgrades.
        style.spacing.button_padding = egui::vec2(4.0, 1.0);
        let visuals = &mut style.visuals;
        // Hint glyphs to physical pixels and avoid fractional glyph variants,
        // which can soften small text. Preserve theme-specific atlas gamma.
        visuals.text_options.font_hinting = true;
        visuals.text_options.subpixel_binning = false;
        let palette = Palette::for_theme(visuals.dark_mode);
        let gray = egui::Color32::from_gray;
        visuals.panel_fill = gray(palette.chrome);
        visuals.window_fill = gray(palette.chrome);
        visuals.extreme_bg_color = gray(palette.field);
        visuals.text_edit_bg_color = Some(gray(palette.field));
        visuals.code_bg_color = gray(palette.field);
        visuals.faint_bg_color = tile_fill(visuals);
        visuals.weak_text_color = Some(gray(palette.secondary));
        visuals.window_corner_radius = egui::CornerRadius::same(6);
        visuals.menu_corner_radius = egui::CornerRadius::same(3);
        visuals.window_stroke = egui::Stroke::new(1.0, gray(palette.border));
        WidgetPalette {
            noninteractive: gray(palette.chrome),
            inactive: gray(palette.button),
            hovered: gray(palette.hover),
            active: gray(palette.pressed),
            open: gray(palette.pressed),
        }
        .apply_to(
            &mut visuals.widgets,
            egui::CornerRadius::same(3),
            egui::Stroke::new(1.0, gray(palette.text)),
        );
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_stroke = egui::Stroke::new(1.0, gray(palette.border));
        }
        // Unselected toolbar and navigation controls remain unobtrusive;
        // hover and keyboard focus still have a visible boundary.
        visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        visuals.widgets.hovered.bg_stroke.color = gray(palette.hover_border);
        visuals.widgets.active.bg_stroke.color = gray(palette.focus);
        visuals.widgets.open.bg_stroke.color = gray(palette.hover_border);
        visuals.selection.bg_fill = gray(palette.selection);
        visuals.selection.stroke = egui::Stroke::new(1.0, gray(palette.selected_text));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
        let luminance = |color: egui::Color32| {
            let linear = |channel: u8| {
                let value = f32::from(channel) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn neutral_surfaces_distinguish_fields_buttons_and_selected_content() {
        let ctx = egui::Context::default();
        configure(&ctx);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let style = ctx.style_of(theme);
            let visuals = &style.visuals;
            for color in [
                visuals.panel_fill,
                visuals.window_fill,
                visuals.extreme_bg_color,
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.active.bg_fill,
                visuals.widgets.open.bg_fill,
                visuals.selection.bg_fill,
                canvas_fill(visuals),
                tile_fill(visuals),
            ] {
                assert_eq!(color.r(), color.g());
                assert_eq!(color.g(), color.b());
                assert_eq!(color.a(), 255);
            }
            assert_ne!(visuals.panel_fill, canvas_fill(visuals));
            assert_ne!(canvas_fill(visuals), tile_fill(visuals));
            assert_ne!(visuals.extreme_bg_color, visuals.panel_fill);
            assert_ne!(
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.hovered.bg_fill
            );
            assert_ne!(
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.active.bg_fill
            );
            assert_ne!(visuals.selection.bg_fill, visuals.panel_fill);
            // The same stroke marks focused fields, toolbar buttons and dock
            // targets, including buttons that are already selected or hovered.
            for background in [
                visuals.extreme_bg_color,
                visuals.panel_fill,
                visuals.selection.bg_fill,
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.active.bg_fill,
                tile_fill(visuals),
            ] {
                assert!(contrast(visuals.widgets.active.bg_stroke.color, background) >= 3.0);
            }
            assert_eq!(
                style
                    .text_styles
                    .get(&egui::TextStyle::Body)
                    .map(|font| font.size),
                Some(15.0)
            );
            assert_eq!(style.spacing.interact_size.y, 24.0);
        }
    }

    #[test]
    fn normal_secondary_and_selected_text_have_readable_contrast_in_both_themes() {
        let ctx = egui::Context::default();
        configure(&ctx);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let style = ctx.style_of(theme);
            let visuals = &style.visuals;
            for background in [
                visuals.panel_fill,
                visuals.window_fill,
                visuals.extreme_bg_color,
                visuals.widgets.inactive.bg_fill,
                visuals.widgets.hovered.bg_fill,
                visuals.widgets.active.bg_fill,
                visuals.widgets.open.bg_fill,
                canvas_fill(visuals),
                tile_fill(visuals),
            ] {
                for foreground in [visuals.text_color(), visuals.weak_text_color()] {
                    assert!(
                        contrast(foreground, background) >= 4.5,
                        "{theme:?}: {foreground:?} on {background:?}"
                    );
                }
            }
            assert!(contrast(visuals.selection.stroke.color, visuals.selection.bg_fill) >= 4.5);
        }
    }
}
