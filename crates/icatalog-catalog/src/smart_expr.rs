//! Typed, bounded Boolean expressions. JSON is a command transport, never SQL.
use crate::{CatalogError, Result, SmartRules};
use serde_json::{Map, Value};

pub(crate) const MAX_NODES: usize = 128;
pub(crate) const MAX_DEPTH: usize = 8;
pub(crate) const MAX_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartExpr {
    All(Vec<SmartExpr>),
    Any(Vec<SmartExpr>),
    Rule(SmartPredicate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartPredicate {
    Format(String),
    CapturedFrom(String),
    CapturedTo(String),
    Camera(String),
    Lens(String),
    MinRating(i64),
    MaxRating(i64),
    Label(i64),
    Keyword(String),
    Text(String),
}

pub(crate) fn invalid() -> CatalogError {
    CatalogError::InvalidQuery(
        "invalid Smart Collection expression (maximum 128 nodes, 8 group levels and 64 KiB)",
    )
}

impl SmartPredicate {
    pub fn field(&self) -> &'static str {
        match self {
            Self::Format(_) => "format",
            Self::CapturedFrom(_) => "captured-from",
            Self::CapturedTo(_) => "captured-to",
            Self::Camera(_) => "camera",
            Self::Lens(_) => "lens",
            Self::MinRating(_) => "min-rating",
            Self::MaxRating(_) => "max-rating",
            Self::Label(_) => "label",
            Self::Keyword(_) => "keyword",
            Self::Text(_) => "text",
        }
    }

    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Format(v)
            | Self::CapturedFrom(v)
            | Self::CapturedTo(v)
            | Self::Camera(v)
            | Self::Lens(v)
            | Self::Keyword(v)
            | Self::Text(v) => Some(v),
            _ => None,
        }
    }

    pub(crate) fn number(&self) -> Option<i64> {
        match self {
            Self::MinRating(v) | Self::MaxRating(v) | Self::Label(v) => Some(*v),
            _ => None,
        }
    }

    /// Exact compact JSON leaf size, including escaping, without allocating.
    pub(crate) fn json_len(&self) -> usize {
        let value_len = if let Some(text) = self.text() {
            text.bytes().fold(2usize, |size, byte| {
                size.saturating_add(match byte {
                    b'"' | b'\\' | b'\x08' | b'\t' | b'\n' | b'\x0c' | b'\r' => 2,
                    0..=31 => 6,
                    _ => 1,
                })
            })
        } else {
            self.number().map_or(4, |value| value.to_string().len())
        };
        // {"field":"FIELD","value":VALUE}
        21usize
            .saturating_add(self.field().len())
            .saturating_add(value_len)
    }

    pub(crate) fn from_parts(
        field: &str,
        text: Option<String>,
        number: Option<i64>,
    ) -> Result<Self> {
        let out = match (field, text, number) {
            ("format", Some(v), None) => Self::Format(v),
            ("captured-from", Some(v), None) => Self::CapturedFrom(v),
            ("captured-to", Some(v), None) => Self::CapturedTo(v),
            ("camera", Some(v), None) => Self::Camera(v),
            ("lens", Some(v), None) => Self::Lens(v),
            ("min-rating", None, Some(v)) => Self::MinRating(v),
            ("max-rating", None, Some(v)) => Self::MaxRating(v),
            ("label", None, Some(v)) => Self::Label(v),
            ("keyword", Some(v), None) => Self::Keyword(v),
            ("text", Some(v), None) => Self::Text(v),
            _ => return Err(invalid()),
        };
        out.validate()?;
        Ok(out)
    }

    pub fn validate(&self) -> Result<()> {
        // Each leaf retains the exact pre-existing field validation; bounds on
        // distinct leaves must never be compared across Boolean branches.
        let mut rules = SmartRules::default();
        self.assign(&mut rules);
        rules.validate()
    }

    fn assign(&self, r: &mut SmartRules) {
        match self {
            Self::Format(v) => r.format = Some(v.clone()),
            Self::CapturedFrom(v) => r.captured_from = Some(v.clone()),
            Self::CapturedTo(v) => r.captured_to = Some(v.clone()),
            Self::Camera(v) => r.camera = Some(v.clone()),
            Self::Lens(v) => r.lens = Some(v.clone()),
            Self::MinRating(v) => r.min_rating = Some(*v),
            Self::MaxRating(v) => r.max_rating = Some(*v),
            Self::Label(v) => r.label = Some(*v),
            Self::Keyword(v) => r.keyword = Some(v.clone()),
            Self::Text(v) => r.text = Some(v.clone()),
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Format(v) => {
                let r = SmartRules {
                    format: Some(v.clone()),
                    ..Default::default()
                };
                let formats: Vec<_> = r.formats().map(|v| format!("{v:?}")).collect();
                if formats.len() == 1 {
                    format!("Format is {}", formats.join(""))
                } else {
                    format!("Format is any of ({})", formats.join(", "))
                }
            }
            Self::CapturedFrom(v) => format!("Captured on or after {v:?}"),
            Self::CapturedTo(v) => format!("Captured on or before {v:?}"),
            Self::Camera(v) => format!("Camera contains {v:?}"),
            Self::Lens(v) => format!("Lens contains {v:?}"),
            Self::MinRating(v) => format!("Rating is at least {v} out of 5"),
            Self::MaxRating(v) => format!("Rating is at most {v} out of 5"),
            Self::Label(v) => format!("Label is {v}"),
            Self::Keyword(v) => format!("Keyword is {v:?}"),
            Self::Text(v) => format!("Filename or path contains {v:?}"),
        }
    }
}

impl SmartExpr {
    pub fn validate(&self) -> Result<()> {
        if matches!(self, Self::Rule(_)) {
            return Err(invalid());
        }
        let mut pending = vec![(self, 0usize)];
        let mut count = 0usize;
        let mut bytes = 0usize;
        while let Some((node, depth)) = pending.pop() {
            count = count.saturating_add(1);
            if count > MAX_NODES {
                return Err(invalid());
            }
            match node {
                Self::All(children) | Self::Any(children) => {
                    bytes = bytes
                        .saturating_add(10)
                        .saturating_add(children.len().saturating_sub(1));
                    if depth >= MAX_DEPTH
                        || children.is_empty()
                        || children.len() > MAX_NODES
                        || pending
                            .len()
                            .saturating_add(children.len())
                            .saturating_add(count)
                            > MAX_NODES
                    {
                        return Err(invalid());
                    }
                    pending.extend(children.iter().map(|c| (c, depth + 1)));
                }
                Self::Rule(rule) => {
                    rule.validate()?;
                    bytes = bytes.saturating_add(rule.json_len());
                }
            }
            if bytes > MAX_BYTES {
                return Err(invalid());
            }
        }
        Ok(())
    }

    fn json_value(&self) -> Value {
        let mut map = Map::new();
        match self {
            Self::All(children) | Self::Any(children) => {
                let name = if matches!(self, Self::All(_)) {
                    "all"
                } else {
                    "any"
                };
                map.insert(
                    name.into(),
                    Value::Array(children.iter().map(Self::json_value).collect()),
                );
            }
            Self::Rule(rule) => {
                map.insert("field".into(), rule.field().into());
                map.insert(
                    "value".into(),
                    match rule.text() {
                        Some(v) => Value::String(v.into()),
                        None => Value::from(rule.number()),
                    },
                );
            }
        }
        Value::Object(map)
    }

    fn read_json(value: Value, depth: usize, count: &mut usize) -> Result<Self> {
        *count = count.saturating_add(1);
        if *count > MAX_NODES {
            return Err(invalid());
        }
        let Value::Object(mut map) = value else {
            return Err(invalid());
        };
        if map.len() == 1 {
            let all = map.contains_key("all");
            let children = map
                .remove(if all { "all" } else { "any" })
                .ok_or_else(invalid)?;
            let Value::Array(children) = children else {
                return Err(invalid());
            };
            if children.is_empty() || children.len() > MAX_NODES || depth >= MAX_DEPTH {
                return Err(invalid());
            }
            let children = children
                .into_iter()
                .map(|v| Self::read_json(v, depth + 1, count))
                .collect::<Result<_>>()?;
            return Ok(if all {
                Self::All(children)
            } else {
                Self::Any(children)
            });
        }
        if map.len() != 2 {
            return Err(invalid());
        }
        let field = map.remove("field").ok_or_else(invalid)?;
        let field = field.as_str().ok_or_else(invalid)?;
        let value = map.remove("value").ok_or_else(invalid)?;
        let (text, number) = match value {
            Value::String(v) => (Some(v), None),
            Value::Number(v) => (None, Some(v.as_i64().ok_or_else(invalid)?)),
            _ => return Err(invalid()),
        };
        Ok(Self::Rule(SmartPredicate::from_parts(field, text, number)?))
    }

    fn summary(&self, nested: bool) -> String {
        match self {
            Self::Rule(rule) => rule.summary(),
            Self::All(children) | Self::Any(children) => {
                let separator = if matches!(self, Self::All(_)) {
                    " AND "
                } else {
                    " OR "
                };
                let text = children
                    .iter()
                    .map(|child| child.summary(true))
                    .collect::<Vec<_>>()
                    .join(separator);
                if nested {
                    format!("({text})")
                } else {
                    text
                }
            }
        }
    }
}

impl SmartRules {
    pub(crate) fn has_legacy_fields(&self) -> bool {
        self.format.is_some()
            || self.captured_from.is_some()
            || self.captured_to.is_some()
            || self.camera.is_some()
            || self.lens.is_some()
            || self.min_rating.is_some()
            || self.max_rating.is_some()
            || self.label.is_some()
            || self.keyword.is_some()
            || self.text.is_some()
    }

    /// The root group. Call validate first for values constructed by hand.
    pub fn expression(&self) -> SmartExpr {
        if let Some(expr) = &self.expression {
            return expr.clone();
        }
        let mut rules = Vec::new();
        for rule in [
            self.format.clone().map(SmartPredicate::Format),
            self.captured_from.clone().map(SmartPredicate::CapturedFrom),
            self.captured_to.clone().map(SmartPredicate::CapturedTo),
            self.camera.clone().map(SmartPredicate::Camera),
            self.lens.clone().map(SmartPredicate::Lens),
            self.min_rating.map(SmartPredicate::MinRating),
            self.max_rating.map(SmartPredicate::MaxRating),
            self.label.map(SmartPredicate::Label),
            self.keyword.clone().map(SmartPredicate::Keyword),
            self.text.clone().map(SmartPredicate::Text),
        ]
        .into_iter()
        .flatten()
        {
            rules.push(SmartExpr::Rule(rule));
        }
        SmartExpr::All(rules)
    }

    pub fn canonical_expr(&self) -> Result<SmartExpr> {
        self.validate()?;
        Ok(Self::from_expression(self.expression())?.expression())
    }

    pub fn from_expression(expression: SmartExpr) -> Result<Self> {
        expression.validate()?;
        let rules = Self {
            expression: Some(expression),
            ..Default::default()
        };
        let encoded = rules.to_json()?;
        if encoded.len() > MAX_BYTES {
            return Err(invalid());
        }
        Ok(rules.legacy_rules().unwrap_or(rules))
    }

    pub fn from_expr(expression: SmartExpr) -> Result<Self> {
        Self::from_expression(expression)
    }

    /// Legacy encoding is used only when its flat fields retain exact meaning.
    pub fn legacy_rules(&self) -> Option<Self> {
        if self.expression.is_none() {
            return Some(self.clone());
        }
        if self.has_legacy_fields() {
            return None;
        }
        let Some(SmartExpr::All(children)) = &self.expression else {
            return None;
        };
        let mut fields = std::collections::BTreeSet::new();
        let mut rules = Self::default();
        for child in children {
            let SmartExpr::Rule(rule) = child else {
                return None;
            };
            if !fields.insert(rule.field()) {
                return None;
            }
            rule.assign(&mut rules);
        }
        rules.validate().ok()?;
        Some(rules)
    }

    pub fn is_legacy(&self) -> bool {
        self.legacy_rules().is_some()
    }

    pub fn from_json(text: &str) -> Result<Self> {
        if text.len() > MAX_BYTES {
            return Err(invalid());
        }
        let Json(value) = serde_json::from_str(text).map_err(|_| invalid())?;
        let expression = SmartExpr::read_json(value, 0, &mut 0)?;
        Self::from_expression(expression)
    }

    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        let value = self.expression().json_value();
        let text = serde_json::to_string(&value).map_err(|_| invalid())?;
        if text.len() > MAX_BYTES {
            return Err(invalid());
        }
        Ok(text)
    }

    pub fn summary(&self) -> String {
        if self.validate().is_err() {
            return "Invalid Smart Collection rules".into();
        }
        self.expression().summary(false)
    }
}

// serde_json::Value accepts repeated map keys. Reject them before a later key
// can silently replace a condition supplied by a user or remote catalogue.
struct Json(Value);
impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Json;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded Smart Collection expression")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Json, M::Error> {
                let mut out = Map::new();
                while let Some((key, Json(value))) = map.next_entry::<String, Json>()? {
                    if out.len() >= 2 || out.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom(
                            "unknown or repeated expression key",
                        ));
                    }
                }
                Ok(Json(Value::Object(out)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Json, A::Error> {
                let mut out = Vec::new();
                while let Some(Json(value)) = sequence.next_element()? {
                    if out.len() >= MAX_NODES {
                        return Err(serde::de::Error::custom("too many rules"));
                    }
                    out.push(value);
                }
                Ok(Json(Value::Array(out)))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Json, E> {
                if value.len() > 1024 {
                    return Err(E::custom("rule text exceeds 1024 bytes"));
                }
                Ok(Json(Value::String(value.into())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Json, E> {
                self.visit_str(&value)
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Json, E> {
                Ok(Json(Value::from(value)))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Json, E> {
                Ok(Json(Value::from(value)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
