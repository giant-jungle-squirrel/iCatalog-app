use super::*;

#[test]
fn joined_search_controls_fit_and_submit_clear_with_pointer_and_keyboard() {
    let fixture = Fixture::new();
    for size in [[800.0, 600.0], [1100.0, 720.0]] {
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            for scale in [1.0, 1.5, 2.0] {
                let mut h = fixture.harness_with_display(size, scale, theme);
                assert!(h.query_by_label("Clear search").is_none());
                let field = h.get_by_label("Search name or path").rect();
                let go = h.get_by_label("Go").rect();
                assert!((field.right() - go.left()).abs() < 1.0);
                assert!((field.top() - go.top()).abs() < 1.0);
                assert!((field.bottom() - go.bottom()).abs() < 1.0);
                assert!(h.ctx.viewport_rect().contains_rect(field.union(go)));
                h.get_by_label("Search name or path").click();
                h.get_by_label("Search name or path").type_text("Blue");
                settle(&mut h);
                let clear = h.get_by_label("Clear search").rect();
                assert!(field.contains_rect(clear));
                assert!(clear.left() > field.left());
                h.hover_at(clear.center());
                h.run_steps(2);
                assert_eq!(
                    h.output().platform_output.cursor_icon,
                    egui::CursorIcon::PointingHand
                );
                h.hover_at(egui::pos2(field.left() + 8.0, field.center().y));
                h.run_steps(2);
                assert_eq!(
                    h.output().platform_output.cursor_icon,
                    egui::CursorIcon::Text
                );
                h.get_by_label("Go").click();
                settle(&mut h);
                assert_eq!(h.state().assets.len(), 1);
                assert_eq!(h.state().assets[0].name, "Blue.bmp");
                h.get_by_label("Clear search").click();
                settle(&mut h);
                assert!(h.state().search.is_empty());
                assert_eq!(h.state().assets.len(), 3);
                assert!(h.query_by_label("Clear search").is_none());
                // Clear returns focus to the field: typing needs no extra click.
                h.event(egui::Event::Text("Red".into()));
                h.key_press(egui::Key::Enter);
                settle(&mut h);
                assert_eq!(h.state().assets.len(), 1);
                assert_eq!(h.state().assets[0].name, "Red.bmp");
                h.get_by_label("Clear search").focus();
                h.run_steps(2);
                h.key_press(egui::Key::Space);
                settle(&mut h);
                assert!(h.state().search.is_empty());
                assert_eq!(h.state().assets.len(), 3);
                h.event(egui::Event::Text(
                    "a long folder/path with enough text to scroll horizontally".into(),
                ));
                settle(&mut h);
                assert_eq!(h.get_by_label("Search name or path").rect(), field);
                assert_eq!(h.get_by_label("Go").rect(), go);
                assert!(field.contains_rect(h.get_by_label("Clear search").rect()));
                h.get_by_label("Clear search").click();
                settle(&mut h);
                assert!(h.state().search.is_empty());
            }
        }
    }
}
