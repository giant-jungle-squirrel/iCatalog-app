//! The docked shell uses the same catalogue controls after panels move.
use super::*;
use craft_ui::docking::{Action as DockAction, Placement};
use workspace::Panel;

fn panel_tab<'a>(h: &'a Harness<'_, App>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get(By::new().label(label).role(egui::accesskit::Role::Tab))
}

#[test]
fn view_toolbar_icons_keep_keyboard_activation_and_catalogue_selection() {
    let fixture = Fixture::new();
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut h = fixture.harness_with_display([800.0, 600.0], 1.0, theme);
        h.get(
            By::new()
                .label("Red.bmp")
                .role(egui::accesskit::Role::Button),
        )
        .click();
        settle(&mut h);
        for (label, key, mode) in [
            ("List", egui::Key::Enter, ViewMode::List),
            ("Grid", egui::Key::Space, ViewMode::Grid),
        ] {
            h.get_by_label(label).focus();
            h.run_steps(2);
            h.key_press(key);
            settle(&mut h);
            assert!(h.state().view_mode == mode);
            assert_eq!(h.state().selection.primary, Some(1));
            assert!(h.get_by_label("Search name or path").rect().is_positive());
        }
    }
}

fn float_panel(h: &mut Harness<'_, App>, panel: Panel, rect: [f32; 4]) {
    h.state_mut()
        .workspace
        .apply_all(vec![DockAction::Float { panel, rect }])
        .unwrap();
    settle(h);
}

#[test]
fn newly_selected_asset_and_inspector_annotations_match_on_the_release_frame() {
    let fixture = Fixture::new();
    let mut ctx = open_catalogue(&fixture.path()).unwrap();
    for (id, rating, keyword) in [(1, 1, "Red keyword"), (2, 5, "Blue keyword")] {
        let mut params = Params::new();
        params
            .set("id", id.to_string())
            .set("rating", rating.to_string())
            .set("keyword-exact", keyword);
        dispatch_guarded(&mut ctx, "edit", &params).unwrap();
    }
    let generation = ctx.catalog().catalogue().unwrap().unwrap().generation;
    for mode in ["Grid", "List"] {
        let mut h = fixture.harness([1100.0, 720.0]);
        h.get_by_label(mode).click();
        settle(&mut h);
        h.get(
            By::new()
                .label("Red.bmp")
                .role(egui::accesskit::Role::Button),
        )
        .click();
        settle(&mut h);
        assert_eq!(
            h.get_by_label("Rating").value().as_deref(),
            Some("1 out of 5")
        );
        assert!(h.query_by_label("Red keyword").is_some());
        let target = asset_rect(&h, "Blue.bmp").center();
        h.hover_at(target);
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        // No settling frame: the inspector painted by the release frame must
        // already correspond to the browser's new selection.
        assert_eq!(h.state().selection.primary, Some(2));
        assert_eq!(
            h.get_by_label("Rating").value().as_deref(),
            Some("5 out of 5")
        );
        assert!(h.query_by_label("Blue keyword").is_some());
        assert!(h.query_by_label("Red keyword").is_none());
        assert_eq!(
            ctx.catalog().catalogue().unwrap().unwrap().generation,
            generation
        );
    }
}

#[test]
fn default_panels_and_details_visibility_survive_grid_list_switches() {
    let fixture = Fixture::new();
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        for scale in [1.0, 1.5, 2.0] {
            let mut h = fixture.harness_with_display([800.0, 600.0], scale, theme);
            for panel in Panel::AUXILIARY {
                assert!(h.state().workspace.visible(panel));
                assert!(panel_tab(&h, panel.label()).rect().is_positive());
            }
            h.get_by_label("List").click();
            settle(&mut h);
            assert!(h.state().workspace.visible(Panel::Details));
            h.state_mut().workspace_action("view.details");
            settle(&mut h);
            assert!(!h.state().workspace.visible(Panel::Details));
            h.get_by_label("Grid").click();
            settle(&mut h);
            assert!(!h.state().workspace.visible(Panel::Details));
            h.state_mut().workspace_action("view.details");
            settle(&mut h);
            assert!(panel_tab(&h, "Details").rect().is_positive());
        }
    }
}

#[test]
fn tabbing_navigation_keeps_selection_and_catalogue_unchanged() {
    let fixture = Fixture::new();
    let ctx = open_catalogue(&fixture.path()).unwrap();
    let album = ctx.catalog().insert_collection("Album").unwrap();
    let generation = ctx.catalog().catalogue().unwrap().unwrap().generation;
    let mut h = fixture.harness([1100.0, 720.0]);
    h.get(
        By::new()
            .label("Red.bmp")
            .role(egui::accesskit::Role::Button),
    )
    .click();
    settle(&mut h);
    h.state_mut()
        .workspace
        .apply_all(vec![DockAction::Move {
            panel: Panel::Collections,
            anchor: Panel::Folders,
            placement: Placement::Tab { before: None },
        }])
        .unwrap();
    settle(&mut h);
    panel_tab(&h, "Collections").click();
    settle(&mut h);
    assert!(h.get_by_label("Album").rect().is_positive());
    assert_eq!(h.state().selection.primary, Some(1));
    panel_tab(&h, "Folders").click();
    settle(&mut h);
    h.get(By::new().label("Other").role(egui::accesskit::Role::Button))
        .click();
    settle(&mut h);
    assert_eq!(
        h.state().assets.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![3]
    );
    assert_eq!(
        ctx.catalog().catalogue().unwrap().unwrap().generation,
        generation
    );
    assert!(ctx.catalog().collection_assets(album).unwrap().is_empty());
}

#[test]
fn view_menu_hides_restores_and_resets_panels() {
    let fixture = Fixture::new();
    let mut h = fixture.harness([1100.0, 720.0]);
    let original = h.state().workspace.layout().clone();
    for expected in [false, true] {
        h.get_by_label("View").click();
        h.run_steps(2);
        h.get(
            By::new()
                .label("Panels")
                .role(egui::accesskit::Role::Button),
        )
        .click();
        h.run_steps(2);
        h.get(
            By::new()
                .label("Folders")
                .role(egui::accesskit::Role::CheckBox),
        )
        .click();
        h.run_steps(2);
        assert_eq!(h.state().workspace.visible(Panel::Folders), expected);
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        h.key_press(egui::Key::Escape);
        settle(&mut h);
    }
    assert_eq!(h.state().workspace.layout(), &original);
    float_panel(&mut h, Panel::Details, [600.0, 150.0, 320.0, 400.0]);
    h.get_by_label("View").click();
    h.run_steps(2);
    h.get_by_label("Reset Layout").click();
    settle(&mut h);
    assert_eq!(h.state().workspace.layout(), &original);
}

#[test]
fn pending_work_and_modals_block_workspace_menu_actions() {
    let fixture = Fixture::new();
    let mut h = fixture.harness([1100.0, 720.0]);
    float_panel(&mut h, Panel::Details, [600.0, 150.0, 320.0, 400.0]);
    let before = h.state().workspace.layout().clone();
    h.state_mut().create_collection = Some(CreateCollectionDialog::default());
    h.state_mut().workspace_action("view.folders");
    h.state_mut().workspace_action("view.reset");
    assert_eq!(h.state().workspace.layout(), &before);
    h.state_mut().create_collection = None;
    let (release, receive) = std::sync::mpsc::channel();
    h.state_mut().worker = Some(worker::Worker::blocked_for_test(receive));
    h.state_mut().workspace_action("view.collections");
    h.state_mut().workspace_action("view.reset");
    assert_eq!(h.state().workspace.layout(), &before);
    release.send(()).unwrap();
    settle(&mut h);
    h.state_mut().workspace_action("view.reset");
    settle(&mut h);
    assert!(h.state().workspace.layout().floating.is_empty());
}

#[test]
fn panel_context_menu_and_pointer_drag_control_the_shared_floating_window() {
    let fixture = Fixture::new();
    let mut h = fixture.harness([1100.0, 720.0]);
    panel_tab(&h, "Collections").click_secondary();
    h.run_steps(3);
    h.get_by_label("Float Panel").click();
    settle(&mut h);
    let before = h.state().workspace.layout().floating.first().unwrap().rect;
    let source = h.get_by_label("Move panel window").rect().center();
    let target = source + egui::vec2(100.0, 30.0);
    h.hover_at(source);
    h.run_steps(1);
    h.drag_at(source);
    h.run_steps(1);
    h.hover_at(target);
    h.run_steps(3);
    release_at(&mut h, target);
    let window = h.state().workspace.layout().floating.first().unwrap();
    assert!(window.contains(&Panel::Collections));
    assert_ne!(window.rect, before);
    assert!(h.get_by_label("New Collection…").rect().is_positive());
}

#[test]
fn asset_drop_into_floating_collections_uses_existing_membership_command() {
    for mode in ["Grid", "List"] {
        let fixture = Fixture::new();
        let ctx = open_catalogue(&fixture.path()).unwrap();
        let album = ctx.catalog().insert_collection("Floating album").unwrap();
        let mut h = fixture.harness([1100.0, 720.0]);
        h.get_by_label(mode).click();
        settle(&mut h);
        // Place the collection below the first asset row, leaving its drag
        // source and the browser toolbar unobstructed.
        float_panel(&mut h, Panel::Collections, [450.0, 400.0, 260.0, 240.0]);
        let before = h.state().workspace.layout().clone();
        let target = h.get_by_label("Floating album").rect().center();
        drag_to(&mut h, "Blue.bmp", target);
        assert!(ctx.catalog().collection_assets(album).unwrap().is_empty());
        release_at(&mut h, target);
        assert_eq!(ctx.catalog().collection_assets(album).unwrap(), vec![2]);
        assert_eq!(h.state().workspace.layout(), &before);
        assert_eq!(ctx.catalog().asset(2).unwrap().unwrap().folder_id, Some(1));
        let reopened = open_catalogue(&fixture.path()).unwrap();
        assert_eq!(
            reopened.catalog().collection_assets(album).unwrap(),
            vec![2]
        );
    }
}

#[test]
fn keyboard_activates_docked_tabs_and_resizes_the_navigation_split() {
    let fixture = Fixture::new();
    let ctx = open_catalogue(&fixture.path()).unwrap();
    ctx.catalog().insert_collection("Keyboard album").unwrap();
    let mut h = fixture.harness([1100.0, 720.0]);
    let original_top = panel_tab(&h, "Collections").rect().top();
    h.query_all(By::new().label("Resize panels"))
        .find(|node| node.rect().width() > node.rect().height())
        .unwrap()
        .focus();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowDown);
    settle(&mut h);
    assert!(panel_tab(&h, "Collections").rect().top() > original_top);
    h.state_mut()
        .workspace
        .apply_all([DockAction::Move {
            panel: Panel::Collections,
            anchor: Panel::Folders,
            placement: Placement::Tab { before: None },
        }])
        .unwrap();
    settle(&mut h);
    for (panel, key, contents) in [
        ("Folders", egui::Key::Enter, "All"),
        ("Collections", egui::Key::Space, "Keyboard album"),
    ] {
        panel_tab(&h, panel).focus();
        h.run_steps(2);
        h.key_press(key);
        settle(&mut h);
        assert!(h.get_by_label(contents).rect().is_positive());
    }
}

#[test]
fn default_navigation_height_gives_folders_two_fifths_of_the_dock() {
    let fixture = Fixture::new();
    for height in [600.0, 900.0] {
        let h = fixture.harness([1100.0, height]);
        let folders_top = panel_tab(&h, "Folders").rect().top();
        let collections_top = panel_tab(&h, "Collections").rect().top();
        let dock_bottom = h.get_by_label("Ready").rect().top();
        // Tabs and the column drag bar occupy a few points of the dock;
        // measure the rendered groups rather than a saved preferred height.
        let ratio = (collections_top - folders_top) / (dock_bottom - folders_top);
        assert!((ratio - 0.4).abs() < 0.03, "height {height}: {ratio}");
    }
}

#[test]
fn restored_floating_controls_remain_reachable_after_window_shrinks() {
    let fixture = Fixture::new();
    let settings = fixture.dir.path().join("workspace.json");
    let mut h = fixture.harness([1400.0, 1000.0]);
    h.state_mut().workspace = workspace::Controller::load(settings.clone());
    float_panel(&mut h, Panel::Collections, [1000.0, 650.0, 300.0, 300.0]);
    h.state_mut().workspace.flush();
    assert!(h.state().workspace.warning().is_none());
    h.state_mut().workspace = workspace::Controller::load(settings);
    h.set_size(egui::vec2(800.0, 600.0));
    settle(&mut h);
    for label in ["Move panel window", "Close panel window"] {
        let rect = h.get_by_label(label).rect();
        assert!(rect.is_positive());
        assert!(
            h.ctx.viewport_rect().contains_rect(rect),
            "{label}: {rect:?}"
        );
        assert!(rect.top() > h.get_by_label("View").rect().bottom());
        assert!(rect.bottom() < h.get_by_label("Ready").rect().top());
    }
    h.get_by_label("Close panel window").click();
    settle(&mut h);
    assert!(!h.state().workspace.visible(Panel::Collections));
    h.state_mut().workspace_action("view.collections");
    settle(&mut h);
    assert!(h.get_by_label("New Collection…").rect().is_positive());
}
