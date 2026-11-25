//! Deferred native pickers. Paths remain drafts until the existing command is dispatched.
use super::*;
use craft_dialogs::{Answer, Dialogs, FileDialog};

#[derive(Clone, Copy, Debug)]
pub(super) enum Intent {
    Import,
    Destination,
    SyncToken,
    SyncCa,
    ConflictToken,
    ConflictCa,
    PreviewToken,
    PreviewCa,
}

#[derive(Default)]
pub(super) struct Pickers {
    dialogs: Option<Dialogs<Intent>>,
    pending: bool,
}

impl Pickers {
    pub fn pending(&self) -> bool {
        self.pending
    }

    pub fn ask(&mut self, ctx: &egui::Context, intent: Intent) {
        if self.pending {
            if let Some(dialogs) = &self.dialogs {
                dialogs.raise();
            }
            return;
        }
        let dialogs = self.dialogs.get_or_insert_with(|| {
            let ctx = ctx.clone();
            Dialogs::new(std::sync::Arc::new(move || ctx.request_repaint()))
        });
        let dialog = match intent {
            Intent::Import => FileDialog::folder(),
            Intent::Destination => FileDialog::save()
                .filter(format!("{APP_NAME} catalogue"), &["icatalog", "sqlite"])
                .file_name("Library.icatalog"),
            _ => FileDialog::open(),
        };
        dialogs.ask(dialog, intent);
        self.pending = true;
    }

    pub fn show(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.pending {
            return;
        }
        // Keep egui controls inert while the native panel owns the interaction.
        egui::Modal::new(egui::Id::unique("native_picker_pending")).show(ctx, |ui| {
            ui.spinner();
            ui.label("Choose a file or cancel in the native dialog.");
        });
        if let Some(dialogs) = &mut self.dialogs {
            dialogs.show_pending(Some(frame));
        }
    }

    fn answers(&mut self) -> Vec<(Intent, Answer)> {
        let answers = self
            .dialogs
            .as_mut()
            .map(Dialogs::take_answers)
            .unwrap_or_default();
        if !answers.is_empty() {
            self.pending = false;
        }
        answers
    }
}

impl App {
    pub(super) fn dispatch_picker_answer(&mut self) {
        // Dispatch while this is still the catalogue that opened the picker.
        // Starting the worker also prevents a Finder open later in this frame
        // from switching the catalogue beneath an accepted import.
        if let Some(action) = self.poll_pickers() {
            self.run_action(action);
        }
    }

    pub(super) fn poll_pickers(&mut self) -> Option<Action> {
        let mut action = None;
        let answers = self.pickers.answers();
        if !answers.is_empty() {
            self.set_menu_enabled(
                self.worker.is_none()
                    && self.create_collection.is_none()
                    && self.sync_dialog.is_none()
                    && !self.preview.is_open(),
            );
        }
        for (intent, answer) in answers {
            match answer {
                Answer::Cancelled => {}
                Answer::Paths(paths) => {
                    let Some(path) = paths.into_iter().next() else {
                        continue;
                    };
                    match intent {
                        Intent::Import if self.can_edit() && !self.replica => {
                            action = Some(Action::Import(path));
                        }
                        Intent::Destination | Intent::SyncToken | Intent::SyncCa => {
                            if let Some(dialog) = &mut self.sync_dialog {
                                if !dialog.pending {
                                    match intent {
                                        Intent::Destination => dialog.destination = Some(path),
                                        Intent::SyncToken => dialog.token_file = Some(path),
                                        Intent::SyncCa => dialog.ca_file = Some(path),
                                        _ => {}
                                    }
                                    dialog.error = None;
                                    dialog.result = None;
                                }
                            }
                        }
                        Intent::ConflictToken | Intent::ConflictCa => {
                            if let Some(dialog) = &mut self.conflict_dialog {
                                if self.worker.is_none() {
                                    match intent {
                                        Intent::ConflictToken => dialog.token_file = Some(path),
                                        Intent::ConflictCa => dialog.ca_file = Some(path),
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Intent::PreviewToken | Intent::PreviewCa => {
                            self.preview
                                .picker_result(matches!(intent, Intent::PreviewToken), Ok(path));
                        }
                        Intent::Import => {}
                    }
                }
                Answer::Unavailable(error) => {
                    let message = format!("Cannot show file dialog: {error}");
                    match intent {
                        Intent::Destination | Intent::SyncToken | Intent::SyncCa => {
                            if let Some(dialog) = &mut self.sync_dialog {
                                dialog.error = Some(message.clone());
                            }
                        }
                        Intent::ConflictToken | Intent::ConflictCa => {
                            if let Some(dialog) = &mut self.conflict_dialog {
                                dialog.error = Some(message.clone());
                            }
                        }
                        Intent::PreviewToken | Intent::PreviewCa => self.preview.picker_result(
                            matches!(intent, Intent::PreviewToken),
                            Err(message.clone()),
                        ),
                        _ => {}
                    }
                    self.status = message;
                }
                Answer::Message(_) => {}
            }
        }
        action
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut ctx = Context::open_in_memory().unwrap();
        dispatch_guarded(&mut ctx, "init", &Params::new()).unwrap();
        let (_, receiver) = std::sync::mpsc::channel();
        App::with_menu(ctx, ":memory:".into(), None, receiver)
    }

    fn answer(app: &mut App, intent: Intent, answer: Answer) {
        app.pickers.ask(&egui::Context::default(), intent);
        let dialogs = app.pickers.dialogs.as_mut().unwrap();
        dialogs.set_override(Some(Box::new(move |_| answer.clone())));
        dialogs.show_pending::<eframe::Frame>(None);
    }

    #[test]
    fn queued_security_choices_preserve_drafts_on_cancel_and_report_failure() {
        let mut app = app();
        app.open_sync_dialog(true);
        app.sync_dialog.as_mut().unwrap().token_file = Some("original.token".into());
        answer(&mut app, Intent::SyncToken, Answer::Cancelled);
        assert!(app.pickers.pending());
        assert!(!app.can_edit());
        assert!(!app.close_allowed());
        assert!(app.poll_pickers().is_none());
        assert!(!app.pickers.pending());
        assert_eq!(
            app.sync_dialog.as_ref().unwrap().token_file,
            Some("original.token".into())
        );
        answer(
            &mut app,
            Intent::Destination,
            Answer::Unavailable("synthetic failure".into()),
        );
        app.poll_pickers();
        assert!(app.sync_dialog.as_ref().unwrap().destination.is_none());
        assert!(app
            .sync_dialog
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("synthetic failure"));
        answer(
            &mut app,
            Intent::SyncToken,
            Answer::Paths(vec!["replacement.token".into()]),
        );
        assert_eq!(
            app.sync_dialog.as_ref().unwrap().token_file,
            Some("original.token".into())
        );
        app.poll_pickers();
        assert_eq!(
            app.sync_dialog.as_ref().unwrap().token_file,
            Some("replacement.token".into())
        );
        assert!(app.sync_dialog.as_ref().unwrap().error.is_none());
    }

    #[test]
    fn queued_import_rechecks_access_and_cancel_never_dispatches() {
        let mut app = app();
        answer(&mut app, Intent::Import, Answer::Cancelled);
        assert!(app.poll_pickers().is_none());
        answer(
            &mut app,
            Intent::Import,
            Answer::Paths(vec!["/synthetic".into()]),
        );
        app.replica = true;
        assert!(app.poll_pickers().is_none());
        app.replica = false;
        answer(
            &mut app,
            Intent::Import,
            Answer::Paths(vec!["/synthetic".into()]),
        );
        assert!(
            matches!(app.poll_pickers(), Some(Action::Import(path)) if path == std::path::Path::new("/synthetic"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accepted_import_is_dispatched_before_same_frame_finder_open() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.icatalog");
        let other = dir.path().join("other.icatalog");
        let ctx = open_catalogue(&original).unwrap();
        drop(open_catalogue(&other).unwrap());
        let (_, receiver) = std::sync::mpsc::channel();
        let mut app = App::with_menu(ctx, original.clone(), None, receiver);
        answer(
            &mut app,
            Intent::Import,
            Answer::Paths(vec![dir.path().into()]),
        );
        app.dispatch_picker_answer();
        assert!(app.worker.is_some());
        app.open_finder_catalogue(other);
        assert_eq!(app.path, original);
        assert!(app.status.contains("Finish the current command or draft"));
    }

    #[test]
    fn only_one_picker_is_queued_and_late_answers_do_not_reopen_a_dialog() {
        let mut app = app();
        app.open_sync_dialog(true);
        let ctx = egui::Context::default();
        app.pickers.ask(&ctx, Intent::SyncToken);
        app.pickers.ask(&ctx, Intent::SyncCa);
        assert_eq!(app.pickers.dialogs.as_ref().unwrap().queued(), 1);
        app.sync_dialog = None;
        let dialogs = app.pickers.dialogs.as_mut().unwrap();
        dialogs.set_override(Some(Box::new(|_| {
            Answer::Paths(vec!["/synthetic/token".into()])
        })));
        dialogs.show_pending::<eframe::Frame>(None);
        app.poll_pickers();
        assert!(app.sync_dialog.is_none());
        assert!(!app.pickers.pending());
    }
}
