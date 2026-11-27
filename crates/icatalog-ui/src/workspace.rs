//! Local window arrangement only: no catalogue data or sync state is stored here.
use craft_ui::docking::{
    Action, Column, Group, Layout, Location, Node, Permissions, Placement, Side, Workspace, Zone,
};
use craft_ui::layout::{SplitAxis, SplitSize};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_BYTES: u64 = 64 * 1024;
const SAVE_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Panel {
    Browser,
    Folders,
    Collections,
    Details,
}

impl Panel {
    pub const AUXILIARY: [Self; 3] = [Self::Folders, Self::Collections, Self::Details];

    pub fn label(self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Folders => "Folders",
            Self::Collections => "Collections",
            Self::Details => "Details",
        }
    }

    pub fn menu_id(self) -> &'static str {
        match self {
            Self::Browser => "view.browser",
            Self::Folders => "view.folders",
            Self::Collections => "view.collections",
            Self::Details => "view.details",
        }
    }
}

pub(crate) fn permissions(panel: &Panel) -> Permissions {
    if *panel == Panel::Browser {
        Permissions {
            accepts_splits: true,
            ..Permissions::PROTECTED
        }
    } else {
        Permissions::default()
    }
}

fn default_layout() -> Layout<Panel> {
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedFirst(220.0),
            first: Box::new(Node::Dock {
                side: Side::Start,
                columns: vec![Column::new(
                    vec![
                        Group::new(vec![Panel::Folders], 0),
                        Group::new(vec![Panel::Collections], 0),
                    ],
                    220.0,
                )],
            }),
            second: Box::new(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedSecond(320.0),
                first: Box::new(Node::Tabs {
                    panels: vec![Panel::Browser],
                    active: 0,
                }),
                second: Box::new(Node::Dock {
                    side: Side::End,
                    columns: vec![Column::new(
                        vec![Group::new(vec![Panel::Details], 0)],
                        320.0,
                    )],
                }),
            }),
        }),
        floating: Vec::new(),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    workspace: Workspace<Panel>,
}

/// The default controller never accesses the filesystem (also used by headless tests).
pub(crate) struct Controller {
    workspace: Workspace<Panel>,
    path: Option<PathBuf>,
    changed_at: Option<Instant>,
    preserve_file: bool,
    warning: Option<String>,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            workspace: Workspace::new(default_layout()),
            path: None,
            changed_at: None,
            preserve_file: false,
            warning: None,
        }
    }
}

impl Controller {
    pub fn load_default() -> Self {
        match default_path() {
            Some(path) => Self::load(path),
            None => Self {
                warning: Some("Cannot find the application-data directory; the workspace will only be remembered for this session.".into()),
                ..Self::default()
            },
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let mut result = Self {
            path: Some(path.clone()),
            ..Self::default()
        };
        match read_workspace(&path) {
            Ok(Some(workspace)) => result.workspace = workspace,
            Ok(None) => {}
            Err(error) => {
                result.preserve_file = true;
                result.warning = Some(format!(
                    "Cannot restore workspace: {error}. Using the default layout. The saved file is preserved until View → Reset Layout."
                ));
            }
        }
        result
    }

    pub fn layout(&self) -> &Layout<Panel> {
        &self.workspace.layout
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    pub fn visible(&self, panel: Panel) -> bool {
        self.workspace.layout.contains(&panel)
    }

    #[cfg(test)]
    pub fn apply(&mut self, action: Action<Panel>) -> Result<(), String> {
        self.apply_all(vec![action])
    }

    pub fn apply_all(
        &mut self,
        actions: impl IntoIterator<Item = Action<Panel>>,
    ) -> Result<(), String> {
        let mut actions = actions.into_iter().peekable();
        if actions.peek().is_none() {
            return Ok(());
        }
        let mut proposed = self.workspace.clone();
        proposed
            .apply_all(actions, permissions)
            .map_err(|error| error.to_string())?;
        validate(&proposed)?;
        self.accept(proposed);
        Ok(())
    }

    pub fn set_visible(&mut self, panel: Panel, visible: bool) -> Result<(), String> {
        let mut proposed = self.workspace.clone();
        let restored = proposed
            .set_visible(panel, visible, Some(Panel::Browser), permissions)
            .map_err(|error| error.to_string())
            .and_then(|()| validate(&proposed));
        if let Err(error) = restored {
            if !visible || panel == Panel::Browser {
                return Err(error);
            }
            // The old anchor may no longer exist. Open beside the browser,
            // never as a tab over its protected contents.
            proposed = self.workspace.clone();
            proposed
                .apply(
                    Action::OpenAt {
                        panel,
                        anchor: Panel::Browser,
                        placement: Placement::Edge(if panel == Panel::Details {
                            Zone::Right
                        } else {
                            Zone::Left
                        }),
                    },
                    permissions,
                )
                .map_err(|error| error.to_string())?;
        }
        validate(&proposed)?;
        self.accept(proposed);
        Ok(())
    }

    fn accept(&mut self, proposed: Workspace<Panel>) {
        if self.workspace != proposed {
            self.workspace = proposed;
            self.changed_at = Some(Instant::now());
        }
    }

    pub fn reset(&mut self) {
        self.workspace = Workspace::new(default_layout());
        self.preserve_file = false;
        self.warning = None;
        self.changed_at = Some(Instant::now());
    }

    pub fn poll_save(&mut self, ctx: &egui::Context) {
        if self.preserve_file || self.path.is_none() {
            return;
        }
        if let Some(changed) = self.changed_at {
            let elapsed = changed.elapsed();
            if elapsed >= SAVE_DELAY {
                self.flush();
            } else {
                ctx.request_repaint_after(SAVE_DELAY.saturating_sub(elapsed));
            }
        }
    }

    /// Flush on orderly shutdown as well as after the debounce. A failure keeps the
    /// previous file intact and is retried on the next layout change, not every frame.
    pub fn flush(&mut self) {
        if self.preserve_file || self.changed_at.is_none() {
            return;
        }
        self.changed_at = None;
        let Some(path) = &self.path else { return };
        match save_workspace(path, &self.workspace) {
            Ok(()) => self.warning = None,
            Err(error) => self.warning = Some(format!("Cannot save workspace: {error}")),
        }
    }
}

fn default_path() -> Option<PathBuf> {
    // Native smoke bundles also set this in LSEnvironment, so Finder launches
    // cannot read or alter the user's real workspace preferences.
    if let Some(directory) = std::env::var_os("ICATALOG_UI_STATE_DIR") {
        return state_path(PathBuf::from(directory));
    }
    #[cfg(target_os = "macos")]
    let directory = std::env::home_dir()?.join("Library/Application Support/icatalog");
    #[cfg(target_os = "windows")]
    let directory = PathBuf::from(std::env::var_os("APPDATA")?).join("icatalog");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let directory = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local/share")))?
        .join("icatalog");
    state_path(directory)
}

fn state_path(directory: PathBuf) -> Option<PathBuf> {
    directory
        .is_absolute()
        .then(|| directory.join("workspace.json"))
}

fn read_workspace(path: &Path) -> Result<Option<Workspace<Panel>>, String> {
    // Inspect without following links or opening the file: opening a FIFO can
    // block startup indefinitely before the bounded reader gets a chance to run.
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("workspace settings must be a regular file of at most 64 KiB".into());
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("workspace file exceeds 64 KiB".into());
    }
    // serde_json's recursion limit stays enabled, before the recursive Craft model
    // is constructed. Craft subsequently checks its own layout-depth bound.
    let saved: Saved = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if saved.version != 1 {
        return Err("unsupported workspace file version".into());
    }
    validate(&saved.workspace)?;
    Ok(Some(saved.workspace))
}

fn validate(workspace: &Workspace<Panel>) -> Result<(), String> {
    workspace.validate().map_err(|error| error.to_string())?;
    if workspace.version == 0 {
        return Err("unsupported workspace model version".into());
    }
    if workspace.saved.is_some() || workspace.locked {
        return Err("unsupported saved or locked workspace".into());
    }
    if workspace
        .hidden
        .iter()
        .any(|(_, location)| !valid_location(location))
    {
        return Err("invalid hidden-panel placement".into());
    }
    let all: HashSet<Panel> = workspace
        .layout
        .panels()
        .into_iter()
        .copied()
        .chain(workspace.hidden.iter().map(|(panel, _)| *panel))
        .collect();
    if all.len() != 4 || !workspace.layout.contains(&Panel::Browser) {
        return Err("workspace must retain the browser and all three panels".into());
    }
    if workspace.layout.floating.iter().any(|window| {
        window.native.is_some()
            || window.monitor.is_some()
            || window.monitor_size.is_some()
            || window.contains(&Panel::Browser)
    }) {
        return Err("workspace panels must stay inside the application".into());
    }
    // A browser may only be a lone, permanently visible frame. Never accept a
    // file that puts it behind a tab, into a minimized dock, or into an accordion.
    let mut nodes: Vec<&Node<Panel>> = workspace.layout.root.iter().collect();
    let mut browser_frame = false;
    while let Some(node) = nodes.pop() {
        match node {
            Node::Split { first, second, .. } => {
                nodes.push(first);
                nodes.push(second);
            }
            Node::Tabs { panels, .. } if panels == &[Panel::Browser] => browser_frame = true,
            _ => {}
        }
    }
    if !browser_frame {
        return Err("the browser must remain a separate central frame".into());
    }
    Ok(())
}

fn valid_location(location: &Location<Panel>) -> bool {
    let extent = |value: f32| value.is_finite() && (0.0..=1_000_000.0).contains(&value);
    let positive = |value: f32| extent(value) && value > 0.0;
    let offset = |value: f32| value.is_finite() && value.abs() <= 1_000_000.0;
    let size = |size| match size {
        SplitSize::Ratio(value) => value.is_finite() && (0.0..=1.0).contains(&value),
        SplitSize::FixedFirst(value) | SplitSize::FixedSecond(value) => extent(value),
    };
    location
        .floating
        .is_none_or(|[x, y, w, h]| offset(x) && offset(y) && positive(w) && positive(h))
        && location.size.is_none_or(size)
        && location
            .stack
            .is_none_or(|(_, height)| height.is_none_or(extent))
        && location.sibling.len() <= 4
        && location.group.as_ref().is_none_or(|group| {
            group.height.is_none_or(extent)
                && group.flyout_width.is_none_or(positive)
                && group.flyout_offset.is_none_or(offset)
                && group.column.is_none_or(|column| {
                    positive(column.width) && column.iconic_width.is_none_or(positive)
                })
        })
        && match location.placement {
            Placement::Group { height, .. } => height.is_none_or(extent),
            Placement::SplitSized {
                size: inner, outer, ..
            } => size(inner) && size(outer),
            _ => true,
        }
}

fn save_workspace(path: &Path, workspace: &Workspace<Panel>) -> Result<(), String> {
    validate(workspace)?;
    let bytes = serde_json::to_vec_pretty(&Saved {
        version: 1,
        workspace: workspace.clone(),
    })
    .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("workspace file exceeds 64 KiB".into());
    }
    let parent = path.parent().ok_or("workspace path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_ui::docking::Placement;

    #[test]
    fn preference_directory_must_be_absolute() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        assert_eq!(
            state_path(directory.path().to_path_buf()),
            Some(directory.path().join("workspace.json"))
        );
        assert!(state_path(PathBuf::from("relative-state")).is_none());
        assert!(state_path(PathBuf::new()).is_none());
        Ok(())
    }

    #[test]
    fn round_trip_retains_floating_tabs_and_hidden_restore(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("workspace.json");
        let mut controller = Controller::load(path.clone());
        controller.apply(Action::Float {
            panel: Panel::Collections,
            rect: [40.0, 60.0, 250.0, 360.0],
        })?;
        controller.apply(Action::Move {
            panel: Panel::Details,
            anchor: Panel::Collections,
            placement: Placement::Tab { before: None },
        })?;
        controller.set_visible(Panel::Folders, false)?;
        controller.flush();
        assert!(controller.warning().is_none());
        let mut restored = Controller::load(path);
        assert!(restored.warning().is_none());
        assert_eq!(restored.workspace, controller.workspace);
        restored.set_visible(Panel::Folders, true)?;
        assert!(restored.visible(Panel::Folders));
        assert_eq!(restored.layout().floating.len(), 1);
        Ok(())
    }

    #[test]
    fn protected_browser_rejects_close_float_and_tab_merge() {
        let mut controller = Controller::default();
        let original = controller.layout().clone();
        assert!(controller.set_visible(Panel::Browser, false).is_err());
        assert!(controller
            .apply(Action::Float {
                panel: Panel::Browser,
                rect: [0.0, 0.0, 300.0, 300.0],
            })
            .is_err());
        assert!(controller
            .apply(Action::Move {
                panel: Panel::Details,
                anchor: Panel::Browser,
                placement: Placement::Tab { before: None },
            })
            .is_err());
        assert_eq!(controller.layout(), &original);
    }

    #[test]
    fn all_hidden_panels_restore_and_missing_anchors_fall_back_beside_browser(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut controller = Controller::default();
        controller.apply(Action::Float {
            panel: Panel::Details,
            rect: [50.0, 50.0, 300.0, 400.0],
        })?;
        controller.apply(Action::Move {
            panel: Panel::Folders,
            anchor: Panel::Details,
            placement: Placement::Tab { before: None },
        })?;
        for panel in Panel::AUXILIARY {
            controller.set_visible(panel, false)?;
        }
        assert_eq!(controller.layout().panels(), vec![&Panel::Browser]);
        for panel in Panel::AUXILIARY {
            controller.set_visible(panel, true)?;
            assert!(controller.visible(panel));
        }
        controller.set_visible(Panel::Details, false)?;
        if let Some((_, location)) = controller.workspace.hidden.first_mut() {
            location.anchor = Some(Panel::Browser);
            location.placement = Placement::Tab { before: None };
            location.floating = None;
            location.group = None;
        }
        controller.set_visible(Panel::Details, true)?;
        validate(&controller.workspace)?;
        assert!(controller.visible(Panel::Details));
        Ok(())
    }

    #[test]
    fn hostile_settings_are_preserved_until_explicit_reset(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("workspace.json");
        let valid = serde_json::to_value(Saved {
            version: 1,
            workspace: Workspace::new(default_layout()),
        })?;
        let mut future = valid.clone();
        future["version"] = 2.into();
        let mut invalid = valid.clone();
        invalid["workspace"]["layout"]["root"] = serde_json::Value::Null;
        let mut newer_library = valid;
        newer_library["workspace"]["version"] = 999.into();
        let mut invalid_hidden = Controller::default();
        invalid_hidden.set_visible(Panel::Details, false)?;
        if let Some((_, location)) = invalid_hidden.workspace.hidden.first_mut() {
            location.floating = Some([0.0, 0.0, -1.0, 1.0]);
        }
        let leaf = r#"{"Tabs":{"panels":["browser"],"active":0}}"#;
        let mut deep = leaf.to_owned();
        for _ in 0..150 {
            deep = format!(
                r#"{{"Split":{{"axis":"Horizontal","size":{{"Ratio":0.5}},"first":{deep},"second":{leaf}}}}}"#
            );
        }
        let deeply_nested_layout = format!(
            r#"{{"version":1,"workspace":{{"version":2,"layout":{{"root":{deep},"floating":[]}},"hidden":[]}}}}"#
        );
        for bytes in [
            b"not json".to_vec(),
            vec![b' '; MAX_BYTES as usize + 1],
            format!("{}0{}", "[".repeat(256), "]".repeat(256)).into_bytes(),
            serde_json::to_vec(&future)?,
            serde_json::to_vec(&invalid)?,
            serde_json::to_vec(&newer_library)?,
            serde_json::to_vec(&Saved {
                version: 1,
                workspace: invalid_hidden.workspace,
            })?,
            deeply_nested_layout.into_bytes(),
        ] {
            std::fs::write(&path, &bytes)?;
            let mut controller = Controller::load(path.clone());
            assert!(controller.warning().is_some());
            assert!(controller.visible(Panel::Browser));
            controller.set_visible(Panel::Details, false)?;
            controller.flush();
            assert_eq!(std::fs::read(&path)?, bytes);
            controller.reset();
            controller.flush();
            assert!(controller.warning().is_none());
            assert!(Controller::load(path.clone()).warning().is_none());
        }
        Ok(())
    }

    #[test]
    fn invalid_loaded_panel_permissions_are_rejected() {
        let mut workspace = Workspace::new(default_layout());
        workspace.layout.root = Some(Node::Tabs {
            panels: vec![
                Panel::Browser,
                Panel::Folders,
                Panel::Collections,
                Panel::Details,
            ],
            active: 1,
        });
        assert!(validate(&workspace).is_err());
        workspace.layout.root = Some(Node::Tabs {
            panels: vec![Panel::Folders, Panel::Collections, Panel::Details],
            active: 0,
        });
        workspace
            .layout
            .floating
            .push(craft_ui::docking::Floating::single(
                vec![Panel::Browser],
                0,
                [0.0, 0.0, 300.0, 300.0],
            ));
        assert!(validate(&workspace).is_err());
        workspace = Workspace::new(default_layout());
        assert!(workspace
            .apply(
                Action::Float {
                    panel: Panel::Details,
                    rect: [0.0, 0.0, 300.0, 300.0],
                },
                permissions,
            )
            .is_ok());
        if let Some(window) = workspace.layout.floating.first_mut() {
            window.native = Some([0.0, 0.0, 300.0, 300.0]);
        }
        assert!(validate(&workspace).is_err());
    }

    #[test]
    fn failed_write_reports_error_and_preserves_destination(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join("workspace.json");
        std::fs::create_dir(&destination)?;
        std::fs::write(destination.join("keep"), "untouched")?;
        let mut controller = Controller::load(destination.clone());
        controller.reset();
        controller.flush();
        assert!(controller
            .warning()
            .is_some_and(|error| error.starts_with("Cannot save")));
        assert_eq!(
            std::fs::read_to_string(destination.join("keep"))?,
            "untouched"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fifo_and_symlink_settings_are_rejected_before_open() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::os::unix::fs::{symlink, FileTypeExt};

        let directory = tempfile::tempdir()?;
        let fifo = directory.path().join("fifo");
        let status = std::process::Command::new("mkfifo").arg(&fifo).status()?;
        assert!(status.success(), "mkfifo fixture creation failed");
        let link = directory.path().join("workspace.json");
        symlink(&fifo, &link)?;
        for path in [&fifo, &link] {
            let mut controller = Controller::load(path.clone());
            assert!(controller
                .warning()
                .is_some_and(|warning| warning.contains("regular file")));
            controller.set_visible(Panel::Details, false)?;
            controller.flush();
        }
        assert!(std::fs::symlink_metadata(&fifo)?.file_type().is_fifo());
        assert!(std::fs::symlink_metadata(&link)?.file_type().is_symlink());
        // Reset may replace the settings link, but must never touch its target.
        let mut controller = Controller::load(link.clone());
        controller.reset();
        controller.flush();
        assert!(controller.warning().is_none());
        assert!(std::fs::symlink_metadata(&link)?.is_file());
        assert!(std::fs::symlink_metadata(&fifo)?.file_type().is_fifo());
        Ok(())
    }

    #[test]
    fn pending_change_is_debounced() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("workspace.json");
        let mut controller = Controller::load(path.clone());
        controller.set_visible(Panel::Details, false)?;
        controller.poll_save(&egui::Context::default());
        assert!(!path.exists());
        controller.changed_at = Instant::now().checked_sub(SAVE_DELAY);
        controller.poll_save(&egui::Context::default());
        assert!(path.exists());
        assert!(!Controller::load(path).visible(Panel::Details));
        Ok(())
    }
}
