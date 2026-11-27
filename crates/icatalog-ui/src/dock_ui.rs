//! Shared workspace chrome; catalogue actions remain on the existing command path.
use super::*;
use craft_ui::docking::{DockArea, DockContent, DockHeights, DockStyle, PanelLimits, Permissions};
use workspace::Panel;

fn area_id() -> egui::Id {
    egui::Id::unique("catalogue_workspace")
}

impl App {
    pub(crate) fn update_workspace_menu(&self) {
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.menu {
            let enabled = !self.workspace_blocked();
            for (panel, item) in &menu.panels {
                item.set_checked(self.workspace.visible(*panel));
                item.set_enabled(enabled);
            }
            menu.reset_layout.set_enabled(enabled);
        }
    }

    fn folders_panel(&mut self, ui: &mut egui::Ui) {
        // folder tree
        let mut sel = self.folder_sel.clone();
        if navigation::row(
            ui,
            matches!(sel, FolderSelection::All),
            "All",
            navigation::Icon::Library,
            false,
        )
        .clicked()
        {
            sel = FolderSelection::All;
        }
        if navigation::row(
            ui,
            matches!(sel, FolderSelection::Unassigned),
            "Unassigned",
            navigation::Icon::Unassigned,
            false,
        )
        .clicked()
        {
            sel = FolderSelection::Unassigned;
        }
        fn folder_device(
            f: &Folder,
            devices: &HashMap<i64, (String, String)>,
        ) -> (Option<String>, String) {
            match f.source_id.and_then(|s| devices.get(&s)) {
                Some((id, label)) => (Some(id.clone()), label.clone()),
                None => (None, "Other".to_string()),
            }
        }
        fn build_tree(
            ui: &mut egui::Ui,
            folders: &[Folder],
            parent: Option<i64>,
            state: &mut TreeState<'_>,
            depth: usize,
            devices: &HashMap<i64, (String, String)>,
        ) {
            if depth >= 128 {
                ui.label("Folder nesting limit reached");
                return;
            }
            let mut children: Vec<&Folder> =
                folders.iter().filter(|x| x.parent_id == parent).collect();
            if parent.is_none() {
                // Group sources under their device (This Mac, disc, share).
                children.sort_by(|a, b| {
                    let left_device = folder_device(a, devices);
                    let right_device = folder_device(b, devices);
                    left_device
                        .1
                        .cmp(&right_device.1)
                        .then_with(|| left_device.0.cmp(&right_device.0))
                        .then_with(|| {
                            a.path
                                .as_deref()
                                .unwrap_or(&a.name)
                                .cmp(b.path.as_deref().unwrap_or(&b.name))
                        })
                });
            }
            let mut last_device: Option<(Option<String>, String)> = None;
            for f in children {
                if parent.is_none() {
                    let (did, dev) = folder_device(f, devices);
                    if last_device.as_ref() != Some(&(did.clone(), dev.clone())) {
                        last_device = Some((did.clone(), dev.clone()));
                        ui.add_space(6.0);
                        let key = did.clone().unwrap_or_else(|| "Other".to_string());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if did.is_some() {
                                // Accordion: the arrow collapses the
                                // device; its name selects every source
                                // on that device.
                                let open = !state.collapsed_devices.contains(&key);
                                if navigation::disclosure(ui, open, &dev).clicked() {
                                    if open {
                                        state.collapsed_devices.insert(key.clone());
                                    } else {
                                        state.collapsed_devices.remove(&key);
                                    }
                                }
                                let is_sel = matches!(
                                    state.selected,
                                    FolderSelection::Device(d) if *d == key
                                );
                                if navigation::row(ui, is_sel, &dev, navigation::Icon::Device, true)
                                    .clicked()
                                {
                                    state.collapsed_devices.remove(&key);
                                    *state.selected = FolderSelection::Device(key.clone());
                                }
                            } else {
                                navigation::disclosure_space(ui);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 24.0),
                                    egui::Sense::hover(),
                                );
                                navigation::paint_icon(
                                    ui,
                                    rect,
                                    navigation::Icon::Device,
                                    ui.visuals().weak_text_color(),
                                );
                                ui.label(egui::RichText::new(&dev).strong());
                            }
                        });
                    }
                    if let Some(id) = &did {
                        if state.collapsed_devices.contains(id) {
                            continue;
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.add_space(navigation::INDENT * (depth + 1) as f32);
                    let has_children = folders.iter().any(|x| x.parent_id == Some(f.id));
                    if has_children {
                        let exp = state.expanded.contains(&f.id);
                        if navigation::disclosure(ui, exp, &f.name).clicked() {
                            if exp {
                                state.expanded.remove(&f.id);
                            } else {
                                state.expanded.insert(f.id);
                            }
                        }
                    } else {
                        navigation::disclosure_space(ui);
                    }
                    let is_sel =
                        matches!(state.selected, FolderSelection::Folder(id) if *id == f.id);
                    if navigation::row(
                        ui,
                        is_sel,
                        &f.name,
                        navigation::Icon::Folder {
                            open: has_children && state.expanded.contains(&f.id),
                        },
                        false,
                    )
                    .clicked()
                    {
                        *state.selected = FolderSelection::Folder(f.id);
                    }
                });
                if state.expanded.contains(&f.id) {
                    build_tree(ui, folders, Some(f.id), state, depth + 1, devices);
                }
            }
        }
        let devices: HashMap<i64, (String, String)> =
            self.sources.iter().map(|s| (s.id, s.device())).collect();
        let mut tree = TreeState {
            expanded: &mut self.expanded,
            collapsed_devices: &mut self.collapsed_devices,
            selected: &mut sel,
        };
        egui::ScrollArea::vertical()
            .id_salt("folder_tree")
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                build_tree(ui, &self.folders, None, &mut tree, 0, &devices);
            });
        if sel != self.folder_sel {
            self.folder_sel = sel;
            self.collection_anchor = None;
            self.dirty = true;
        }
    }

    pub(crate) fn workspace_blocked(&self) -> bool {
        self.worker.is_some()
            || self.pickers.pending()
            || self.create_collection.is_some()
            || self.sync_dialog.is_some()
            || self.conflict_dialog.is_some()
            || self.sync_decision.is_some()
            || self.preview.is_open()
            || self.sync_job.is_some()
            || self.recovering_snapshot
            || self.catalogue_error.is_some()
    }

    fn workspace_result(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.status = format!("Cannot change workspace: {error}");
        }
    }

    pub(crate) fn workspace_action(&mut self, id: &str) {
        if self.workspace_blocked() {
            return;
        }
        if id == "view.reset" {
            self.workspace.reset();
            return;
        }
        for panel in Panel::AUXILIARY {
            if id == panel.menu_id() {
                let result = self
                    .workspace
                    .set_visible(panel, !self.workspace.visible(panel));
                self.workspace_result(result);
                break;
            }
        }
    }

    pub(crate) fn workspace_menu(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.workspace_blocked(), |ui| {
            ui.menu_button("Panels", |ui| {
                for panel in Panel::AUXILIARY {
                    let mut visible = self.workspace.visible(panel);
                    if ui.checkbox(&mut visible, panel.label()).changed() {
                        let result = self.workspace.set_visible(panel, visible);
                        self.workspace_result(result);
                    }
                }
            });
            if ui.button("Reset Layout").clicked() {
                self.workspace.reset();
                ui.close();
            }
        });
    }

    pub(crate) fn show_workspace(&mut self, ui: &mut egui::Ui) -> (Option<Params>, Option<Params>) {
        let mut edit = None;
        let mut collection = None;
        egui::CentralPanel::default().show(ui, |ui| {
            let blocked = self.workspace_blocked();
            let asset_drag = egui::DragAndDrop::payload::<DraggedAsset>(ui.ctx()).is_some();
            if blocked {
                craft_ui::docking::cancel_drag::<Panel>(ui.ctx(), area_id());
            }
            if asset_drag {
                craft_ui::docking::route_app_drag(ui.ctx(), area_id());
            }
            let layout = self.workspace.layout().clone();
            let mut style = DockStyle::from_ui(ui);
            style.font = egui::TextStyle::Body.resolve(ui.style());
            style.tab_height = style.font.size + 14.0;
            style.min_tab_width = style.font.size * 2.5 + 24.0;
            // Quiet, recessed strips with an active tab continuous with its panel.
            style.tab_background = ui.visuals().faint_bg_color;
            style.active_background = ui.visuals().panel_fill;
            style.accent = ui.visuals().widgets.active.bg_stroke.color;
            let bounds = ui.max_rect();
            let mut content = Content {
                app: self,
                edit: &mut edit,
                collection: &mut collection,
                folder_height: ((bounds.height() - style.column_bar_height) * 0.4
                    - style.tab_height)
                    .max(100.0),
            };
            let output = DockArea::new(area_id())
                .float_bounds(bounds)
                .input_blocked(blocked)
                .locked(blocked || asset_drag)
                .show_content(ui, &layout, &style, &mut content);
            if !blocked && !asset_drag && !self.workspace_blocked() {
                let result = self.workspace.apply_all(output.actions);
                self.workspace_result(result);
                if let Some(error) = output.error {
                    self.status = format!("Cannot change workspace: {error}");
                }
            }
        });
        self.workspace.poll_save(ui.ctx());
        (edit, collection)
    }

    fn browser_panel(&mut self, ui: &mut egui::Ui) -> Option<Params> {
        // Keep the search visible when the browser narrows; overflow only view controls.
        let spacing = ui.spacing().item_spacing.x;
        let fit = craft_ui::layout::fit_toolbar(
            &[144.0, 75.0],
            (ui.available_width() - 255.0).max(0.0),
            spacing,
            28.0,
        );
        let full = fit.as_ref().is_some_and(|fit| fit.overflow.is_empty());
        ui.horizontal(|ui| {
            if full {
                self.view_controls(ui);
                ui.separator();
                self.search_controls(ui);
            } else {
                self.view_mode_controls(ui);
                let items = [craft_ui::menu::MenuItem::new(
                    "details",
                    if self.workspace.visible(Panel::Details) {
                        "Hide Details"
                    } else {
                        "Show Details"
                    },
                )];
                ui.add_enabled_ui(!self.workspace_blocked(), |ui| {
                    if craft_ui::menu::overflow(
                        ui,
                        &items,
                        &craft_ui::menu::MenuLabels {
                            more: "View options".into(),
                            ..Default::default()
                        },
                    )
                    .is_some()
                    {
                        self.workspace_action("view.details");
                    }
                });
            }
        });
        if !full {
            ui.horizontal(|ui| self.search_controls(ui));
        }
        ui.separator();
        ui.painter().rect_filled(
            ui.available_rect_before_wrap(),
            0.0,
            typography::canvas_fill(ui.visuals()),
        );
        match self.view_mode {
            ViewMode::Grid => self.grid(ui),
            ViewMode::List => self.list(ui),
        }
    }

    fn view_controls(&mut self, ui: &mut egui::Ui) {
        self.view_mode_controls(ui);
        let mut visible = self.workspace.visible(Panel::Details);
        if ui
            .add_enabled(
                !self.workspace_blocked(),
                egui::Button::new("Details").selected(visible),
            )
            .clicked()
        {
            visible = !visible;
            let result = self.workspace.set_visible(Panel::Details, visible);
            self.workspace_result(result);
        }
    }

    fn view_mode_controls(&mut self, ui: &mut egui::Ui) {
        for (mode, label) in [(ViewMode::Grid, "Grid"), (ViewMode::List, "List")] {
            let style = icon_button::toolbar_style(ui);
            let response =
                craft_ui::buttons::IconButton::new(label, egui::vec2(64.0, 24.0), &style)
                    .selected(self.view_mode == mode)
                    .show(ui, |ui, rect, color| {
                        let icon = egui::Rect::from_center_size(
                            egui::pos2(rect.left() + 12.0, rect.center().y),
                            egui::vec2(12.0, 12.0),
                        );
                        if mode == ViewMode::Grid {
                            for y in [0.0, 7.0] {
                                for x in [0.0, 7.0] {
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(
                                            icon.min + egui::vec2(x, y),
                                            egui::vec2(5.0, 5.0),
                                        ),
                                        0.0,
                                        color,
                                    );
                                }
                            }
                        } else {
                            for y in [1.0, 6.0, 11.0] {
                                ui.painter().line_segment(
                                    [
                                        icon.min + egui::vec2(0.0, y),
                                        icon.min + egui::vec2(12.0, y),
                                    ],
                                    egui::Stroke::new(1.5, color),
                                );
                            }
                        }
                        ui.painter().text(
                            egui::pos2(rect.left() + 24.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::TextStyle::Button.resolve(ui.style()),
                            color,
                        );
                    });
            if response.clicked() {
                self.view_mode = mode;
            }
            response.on_hover_text(format!("{label} view"));
        }
    }

    fn search_controls(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Go").clicked() {
                self.dirty = true;
            }
            if ui
                .add_enabled(!self.search.is_empty(), egui::Button::new("x").small())
                .clicked()
            {
                self.search.clear();
                self.dirty = true;
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search name or path")
                    .desired_width(ui.available_width().min(220.0)),
            );
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::text_edit(
                    ui.is_enabled(),
                    &self.search,
                    &self.search,
                    "Search name or path",
                );
                info.label = Some("Search name or path".into());
                info
            });
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.dirty = true;
            }
        });
    }
}

struct Content<'a> {
    app: &'a mut App,
    edit: &'a mut Option<Params>,
    collection: &'a mut Option<Params>,
    folder_height: f32,
}
impl DockContent<Panel> for Content<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &Panel) {
        // Leave docking resize handles outside content hit targets.
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| match panel {
                Panel::Folders => self.app.folders_panel(ui),
                Panel::Collections => {
                    if let Some(command) = self.app.collections_panel(ui) {
                        *self.collection = Some(command);
                    }
                }
                Panel::Browser => {
                    if let Some(command) = self.app.browser_panel(ui) {
                        *self.collection = Some(command);
                    }
                }
                Panel::Details => {
                    // Dock order is user-controlled: Browser may have changed
                    // selection earlier in this frame.
                    self.app.ensure_detail();
                    if let Some(id) = self.app.selection.primary {
                        self.app.ensure_texture(ui.ctx(), id);
                    }
                    craft_ui::layout::Section::new(egui::Frame::NONE.inner_margin(6)).show(
                        ui,
                        |ui| {
                            spacing::inspector(ui);
                            egui::ScrollArea::vertical()
                                .id_salt("details_content")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add_enabled_ui(self.app.can_edit(), |ui| {
                                        *self.edit = self.app.details_panel(ui);
                                    });
                                });
                        },
                    );
                }
            });
    }
    fn logical_label(&self, panel: &Panel) -> String {
        panel.label().into()
    }
    fn permissions(&self, panel: &Panel) -> Permissions {
        workspace::permissions(panel)
    }
    fn limits(&self, panel: &Panel) -> PanelLimits {
        PanelLimits {
            min: match panel {
                Panel::Browser => egui::vec2(260.0, 140.0),
                Panel::Details => egui::vec2(220.0, 100.0),
                _ => egui::vec2(160.0, 100.0),
            },
            ..Default::default()
        }
    }
    fn dock_heights(&self, panel: &Panel) -> Option<DockHeights> {
        match panel {
            Panel::Folders => Some(DockHeights::new(self.folder_height).compact(100.0)),
            Panel::Collections => Some(DockHeights::new(240.0).fill(100.0)),
            _ => None,
        }
    }
    fn floating_size(&self, panel: &Panel) -> Option<egui::Vec2> {
        Some(if *panel == Panel::Details {
            egui::vec2(320.0, 500.0)
        } else {
            egui::vec2(260.0, 360.0)
        })
    }
    fn allow_native(&self, _panel: &Panel) -> bool {
        false
    }
}
