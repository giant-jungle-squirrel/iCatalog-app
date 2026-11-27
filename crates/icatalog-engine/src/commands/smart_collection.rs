use crate::{Context, EngineError, Outcome, Params, Result};
use icatalog_catalog::SmartRules;

pub(super) const RULE_KEYS: &[&str] = &[
    "format",
    "captured-from",
    "captured-to",
    "camera",
    "lens",
    "min-rating",
    "max-rating",
    "label",
    "keyword",
    "text",
    "rules-json",
];

pub(super) fn parse(params: &Params) -> Result<SmartRules> {
    if let Some(json) = params.get("rules-json") {
        if RULE_KEYS
            .iter()
            .any(|key| *key != "rules-json" && params.get(key).is_some())
        {
            return Err(EngineError::BadParam(
                "use rules-json or individual rule options, not both".into(),
            ));
        }
        return SmartRules::from_json(json).map_err(|e| EngineError::BadParam(e.to_string()));
    }
    let text = |key| params.get(key).map(|s| s.trim().to_owned());
    let rules = SmartRules {
        format: text("format"),
        captured_from: text("captured-from"),
        captured_to: text("captured-to"),
        camera: text("camera"),
        lens: text("lens"),
        min_rating: params.get_i64("min-rating")?,
        max_rating: params.get_i64("max-rating")?,
        label: params.get_i64("label")?,
        keyword: params.get("keyword").map(str::to_owned),
        text: text("text"),
        ..Default::default()
    };
    rules
        .validate()
        .map_err(|e| EngineError::BadParam(e.to_string()))?;
    Ok(rules)
}

pub(super) fn run(ctx: &mut Context, params: &Params) -> Result<Outcome> {
    match params.require("action")? {
        "create-smart" => {
            let name = super::collection::name(params)?;
            let rules = parse(params)?;
            let id = ctx.catalog().insert_collection(name)?;
            ctx.catalog().set_smart_rules(id, &rules)?;
            Ok(Outcome::text(format!(
                "Created Smart Collection {id}: {name}"
            )))
        }
        "rules" | "set-rules" => {
            if let Some(output) = params.get("output") {
                if output != "json" {
                    return Err(EngineError::BadParam(
                        "collection rules output must be json (omit for readable rules)".into(),
                    ));
                }
            }
            let id = super::collection::require_id(ctx, params, "id")?;
            let Some(mut rules) = ctx.catalog().smart_rules(id)? else {
                return Err(EngineError::BadParam(
                    "this is a manual collection; create a Smart Collection to use rules".into(),
                ));
            };
            if params.require("action")? == "set-rules" {
                rules = parse(params)?;
                if params.get("name").is_some() {
                    let name = super::collection::name(params)?;
                    ctx.catalog().rename_collection(id, name)?;
                }
                ctx.catalog().set_smart_rules(id, &rules)?;
            }
            if params.get("output") == Some("json") {
                return Ok(Outcome::text(rules.to_json()?));
            }
            if rules.expression.is_some() {
                return Ok(Outcome::text(rules.summary()));
            }
            let pairs = [
                ("format", rules.format),
                ("captured-from", rules.captured_from),
                ("captured-to", rules.captured_to),
                ("camera", rules.camera),
                ("lens", rules.lens),
                ("min-rating", rules.min_rating.map(|v| v.to_string())),
                ("max-rating", rules.max_rating.map(|v| v.to_string())),
                ("label", rules.label.map(|v| v.to_string())),
                ("keyword", rules.keyword),
                ("text", rules.text),
            ];
            Ok(Outcome::table(
                vec!["rule".into(), "value".into()],
                pairs
                    .into_iter()
                    .filter_map(|(k, v)| v.map(|v| vec![k.into(), v]))
                    .collect(),
            ))
        }
        _ => Err(EngineError::BadParam(
            "invalid Smart Collection action".into(),
        )),
    }
}
