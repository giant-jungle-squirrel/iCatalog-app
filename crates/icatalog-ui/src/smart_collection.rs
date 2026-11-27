//! Presentation and parameter transport only; rule validation lives below UI.
use eframe::egui;
use icatalog_catalog::SmartRules;
use icatalog_engine::{Outcome, Params};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct RuleInputs {
    root: DraftNode,
    next_id: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct DraftNode {
    id: u64,
    kind: DraftKind,
}

#[derive(Clone, PartialEq, Eq)]
enum DraftKind {
    Group {
        all: bool,
        children: Vec<DraftNode>,
    },
    Rule {
        field: Field,
        text: String,
        rating: Option<i64>,
    },
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Field {
    #[default]
    Format,
    CapturedFrom,
    CapturedTo,
    Camera,
    Lens,
    MinRating,
    MaxRating,
    Label,
    Keyword,
    Text,
}

impl Field {
    const ALL: [Self; 10] = [
        Self::Format,
        Self::CapturedFrom,
        Self::CapturedTo,
        Self::Camera,
        Self::Lens,
        Self::MinRating,
        Self::MaxRating,
        Self::Label,
        Self::Keyword,
        Self::Text,
    ];

    fn caption(self) -> &'static str {
        match self {
            Self::Format => "Format",
            Self::CapturedFrom => "Captured From",
            Self::CapturedTo => "Captured To",
            Self::Camera => "Camera Contains",
            Self::Lens => "Lens Contains",
            Self::MinRating => "Minimum Rating",
            Self::MaxRating => "Maximum Rating",
            Self::Label => "Label Number",
            Self::Keyword => "Keyword",
            Self::Text => "Filename or Path Contains",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::CapturedFrom => "captured-from",
            Self::CapturedTo => "captured-to",
            Self::Camera => "camera",
            Self::Lens => "lens",
            Self::MinRating => "min-rating",
            Self::MaxRating => "max-rating",
            Self::Label => "label",
            Self::Keyword => "keyword",
            Self::Text => "text",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Format => "PTG, REB, CLIP, …",
            Self::CapturedFrom | Self::CapturedTo => "YYYY-MM-DD (inclusive)",
            Self::Camera => "Camera make or model",
            Self::Lens => "Lens name",
            Self::Label => "0–7",
            Self::Keyword => "Exact keyword name",
            Self::Text => "Filename or path contains",
            Self::MinRating | Self::MaxRating => "",
        }
    }
}

impl Default for RuleInputs {
    fn default() -> Self {
        Self {
            root: DraftNode {
                id: 0,
                kind: DraftKind::Group {
                    all: true,
                    children: Vec::new(),
                },
            },
            next_id: 1,
        }
    }
}

/// One background read at a time. Changed inputs immediately hide old results;
/// an older in-flight result can never overwrite the current draft's preview.
#[derive(Default)]
pub(super) struct RulePreview {
    inputs: Option<RuleInputs>,
    running_inputs: Option<RuleInputs>,
    worker: Option<super::worker::Worker>,
    changed_at: Option<Instant>,
    result: Option<Result<(String, String), String>>,
}

impl RulePreview {
    #[cfg(test)]
    pub(super) fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub(super) fn is_pending(&self) -> bool {
        self.result.is_none()
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, path: &std::path::Path, inputs: &RuleInputs) {
        if self.inputs.as_ref() != Some(inputs) {
            self.inputs = Some(inputs.clone());
            self.changed_at = Some(Instant::now());
            self.result = None;
        }
        if let Some(result) = self.worker.as_mut().and_then(super::worker::Worker::poll) {
            self.worker = None;
            if self.running_inputs.as_ref() == Some(inputs) {
                self.result = Some(result.map_err(|e| e.to_string()).and_then(|outcome| {
                    if let Outcome::Text(text) = outcome {
                        if let Some((count, summary)) = text.split_once('\n') {
                            return Ok((count.to_owned(), summary.to_owned()));
                        }
                    }
                    Err("Cannot read rule preview response".into())
                }));
            }
        }
        if self.worker.is_none()
            && self.is_pending()
            && self
                .changed_at
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(200))
        {
            let mut params = Params::new();
            params.set("action", "preview-smart");
            inputs.apply(&mut params);
            match super::worker::Worker::start(path.to_owned(), "collection".into(), params) {
                Ok(worker) => {
                    self.worker = Some(worker);
                    self.running_inputs = Some(inputs.clone());
                }
                Err(error) => self.result = Some(Err(format!("Cannot preview rules: {error}"))),
            }
        }
        ui.separator();
        // Bound long summaries/errors so every field and the buttons remain
        // reachable in an 800x600 window. The full text is selectable/scrollable.
        egui::ScrollArea::vertical()
            .id_salt("rule_preview")
            .max_height(64.0)
            .min_scrolled_height(64.0)
            .auto_shrink([false, false])
            .show(ui, |ui| match &self.result {
                Some(Ok((count, summary))) => {
                    ui.strong(count);
                    ui.add(egui::Label::new(summary).wrap().selectable(true));
                }
                Some(Err(error)) => {
                    ui.label("Preview unavailable");
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                None => {
                    ui.label("Checking matches…");
                }
            });
        ui.label("Across the whole catalogue. Groups control how rules combine.");
        if self.is_pending() {
            ui.ctx().request_repaint_after(Duration::from_millis(25));
        }
    }
}

impl RuleInputs {
    pub(super) fn from_rules(rules: &SmartRules) -> Self {
        let mut next_id = 0;
        let root = DraftNode::from_expression(rules.expression(), &mut next_id);
        Self { root, next_id }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("Combine rules with All (AND) or Any (OR).");
        let mut remaining = 128_usize.saturating_sub(self.root.node_count());
        egui::ScrollArea::both()
            .id_salt("smart_rule_editor")
            .max_height((ui.ctx().content_rect().height() - 335.0).max(100.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.root
                    .show(ui, &mut self.next_id, &mut remaining, 1, "1");
            });
    }

    pub(super) fn apply(&self, params: &mut Params) {
        params.set("rules-json", self.root.json().to_string());
    }
}

impl DraftNode {
    fn next_id(next: &mut u64) -> u64 {
        let id = *next;
        *next = next.saturating_add(1);
        id
    }

    fn from_expression(expression: icatalog_catalog::SmartExpr, next: &mut u64) -> Self {
        use icatalog_catalog::{SmartExpr, SmartPredicate};
        let id = Self::next_id(next);
        let kind = match expression {
            SmartExpr::All(children) => DraftKind::Group {
                all: true,
                children: children
                    .into_iter()
                    .map(|child| Self::from_expression(child, next))
                    .collect(),
            },
            SmartExpr::Any(children) => DraftKind::Group {
                all: false,
                children: children
                    .into_iter()
                    .map(|child| Self::from_expression(child, next))
                    .collect(),
            },
            SmartExpr::Rule(predicate) => {
                let (field, text, rating) = match predicate {
                    SmartPredicate::Format(v) => (Field::Format, v, None),
                    SmartPredicate::CapturedFrom(v) => (Field::CapturedFrom, v, None),
                    SmartPredicate::CapturedTo(v) => (Field::CapturedTo, v, None),
                    SmartPredicate::Camera(v) => (Field::Camera, v, None),
                    SmartPredicate::Lens(v) => (Field::Lens, v, None),
                    SmartPredicate::MinRating(v) => (Field::MinRating, String::new(), Some(v)),
                    SmartPredicate::MaxRating(v) => (Field::MaxRating, String::new(), Some(v)),
                    SmartPredicate::Label(v) => (Field::Label, v.to_string(), None),
                    SmartPredicate::Keyword(v) => (Field::Keyword, v, None),
                    SmartPredicate::Text(v) => (Field::Text, v, None),
                };
                DraftKind::Rule {
                    field,
                    text,
                    rating,
                }
            }
        };
        Self { id, kind }
    }

    fn node_count(&self) -> usize {
        match &self.kind {
            DraftKind::Group { children, .. } => children
                .iter()
                .fold(1_usize, |sum, child| sum.saturating_add(child.node_count())),
            DraftKind::Rule { .. } => 1,
        }
    }

    fn json(&self) -> serde_json::Value {
        use serde_json::{json, Value};
        match &self.kind {
            DraftKind::Group { all, children } => {
                let children: Vec<_> = children.iter().map(Self::json).collect();
                if *all {
                    json!({"all": children})
                } else {
                    json!({"any": children})
                }
            }
            DraftKind::Rule {
                field,
                text,
                rating,
            } => {
                let value = if matches!(field, Field::MinRating | Field::MaxRating) {
                    rating.map_or(Value::Null, |value| json!(value))
                } else if *field == Field::Label {
                    text.trim()
                        .parse::<i64>()
                        .map_or_else(|_| json!(text), |value| json!(value))
                } else {
                    // JSON-created substring rules may intentionally include
                    // spaces. Opening and saving them must not change meaning.
                    json!(text)
                };
                json!({"field": field.key(), "value": value})
            }
        }
    }

    // These bounds mirror the command's limits solely to keep the editor usable.
    // Validation and matching remain in the catalogue and engine.
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        next: &mut u64,
        remaining: &mut usize,
        depth: usize,
        path: &str,
    ) -> bool {
        ui.push_id(self.id, |ui| match &mut self.kind {
            DraftKind::Group { all, children } => {
                let mut remove = false;
                egui::Frame::group(ui.style()).inner_margin(8).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Match");
                        let response = egui::ComboBox::from_id_salt("operator")
                            .selected_text(if *all { "All (AND)" } else { "Any (OR)" })
                            .width(112.0)
                            .show_ui(ui, |ui| {
                                for (value, label) in [(true, "All (AND)"), (false, "Any (OR)")] {
                                    ui.selectable_value(all, value, label).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), format!("Use {label}")));
                                }
                            }).response;
                        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, ui.is_enabled(), format!("Match group {path}")));
                        ui.label("of these rules");
                        if depth > 1 {
                            remove = ui.button("Remove Group").on_hover_text("Remove this group and its rules").clicked();
                        }
                    });
                    let mut removed = None;
                    for (index, child) in children.iter_mut().enumerate() {
                        if child.show(ui, next, remaining, depth.saturating_add(usize::from(matches!(child.kind, DraftKind::Group { .. }))), &format!("{path}.{}", index.saturating_add(1))) {
                            removed = Some(index);
                        }
                    }
                    if let Some(index) = removed {
                        if let Some(child) = children.get(index) { *remaining = remaining.saturating_add(child.node_count()); }
                        children.remove(index);
                    }
                    ui.horizontal(|ui| {
                        let add_rule = ui.add_enabled(*remaining > 0, egui::Button::new("Add Rule"));
                        if depth > 1 { add_rule.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), format!("Add Rule to group {path}"))); }
                        if add_rule.clicked() {
                            children.push(Self { id: Self::next_id(next), kind: DraftKind::Rule { field: Field::Format, text: String::new(), rating: None } });
                            *remaining = remaining.saturating_sub(1);
                        }
                        let add_group = ui.add_enabled(*remaining > 0 && depth < 8, egui::Button::new("Add Group"));
                        if depth > 1 { add_group.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), format!("Add Group to group {path}"))); }
                        if add_group.clicked() {
                            children.push(Self { id: Self::next_id(next), kind: DraftKind::Group { all: true, children: Vec::new() } });
                            *remaining = remaining.saturating_sub(1);
                        }
                    });
                    let bounds = ui.min_rect();
                    ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                        node.set_role(egui::accesskit::Role::Group);
                        node.set_label(format!("Rule group {path}"));
                        node.set_bounds(egui::accesskit::Rect { x0: f64::from(bounds.left()), y0: f64::from(bounds.top()), x1: f64::from(bounds.right()), y1: f64::from(bounds.bottom()) });
                    });
                });
                remove
            }
            DraftKind::Rule { field, text, rating } => {
                let mut remove = false;
                ui.horizontal_wrapped(|ui| {
                    let before = *field;
                    let response = egui::ComboBox::from_id_salt("field")
                        .selected_text(field.caption()).width(190.0)
                        .show_ui(ui, |ui| {
                            for candidate in Field::ALL {
                                ui.selectable_value(field, candidate, candidate.caption()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), format!("Use {}", candidate.caption())));
                            }
                        }).response;
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, ui.is_enabled(), format!("Field for rule {path}")));
                    if *field != before { text.clear(); *rating = None; }
                    if matches!(field, Field::MinRating | Field::MaxRating) {
                        super::rating::editor(ui, rating, field.caption());
                    } else {
                        let response = ui.add(egui::TextEdit::singleline(text)
                            .hint_text(field.hint()).char_limit(1024).desired_width(180.0));
                        response.widget_info(|| {
                            let mut info = egui::WidgetInfo::text_edit(ui.is_enabled(), text.as_str(), text.as_str(), field.caption());
                            info.label = Some(field.caption().into());
                            info
                        });
                        if *field == Field::Format {
                            response.on_hover_text("Match any listed format or extension. Separate with commas, semicolons or spaces.");
                        }
                    }
                    let response = ui.button("−").on_hover_text("Remove this rule");
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), format!("Remove rule {path}")));
                    remove = response.clicked();
                });
                remove
            }
        }).inner
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn exact_keyword_input_keeps_its_spaces() {
        let rules = SmartRules {
            keyword: Some(" travel ".into()),
            ..Default::default()
        };
        let inputs = RuleInputs::from_rules(&rules);
        let mut params = Params::new();
        inputs.apply(&mut params);
        assert_eq!(
            SmartRules::from_json(params.get("rules-json").unwrap()).unwrap(),
            rules
        );
    }

    #[test]
    fn incomplete_rows_and_empty_groups_are_transported_for_validation() {
        let empty = RuleInputs::default();
        assert_eq!(empty.root.json(), serde_json::json!({"all": []}));
        let node = DraftNode {
            id: 1,
            kind: DraftKind::Rule {
                field: Field::MinRating,
                text: String::new(),
                rating: None,
            },
        };
        assert_eq!(
            node.json(),
            serde_json::json!({"field":"min-rating", "value":null})
        );
    }

    #[test]
    fn reopening_json_rules_preserves_significant_text_spaces() {
        let rules = SmartRules::from_json(
            r#"{"any":[{"field":"text","value":" a "},{"field":"camera","value":" model "},{"field":"lens","value":" 50 "},{"field":"format","value":" PTG "},{"field":"keyword","value":" painting "}]}"#,
        ).unwrap();
        let inputs = RuleInputs::from_rules(&rules);
        let mut params = Params::new();
        inputs.apply(&mut params);
        assert_eq!(
            SmartRules::from_json(params.get("rules-json").unwrap()).unwrap(),
            rules
        );
    }
}
