//! Bounded Smart Collection rules and legacy AND-field compatibility.
pub use crate::smart_expr::{SmartExpr, SmartPredicate};
pub(crate) use crate::smart_storage::{membership_sql, SMART_MATCH};
use crate::{CatalogError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmartRules {
    /// A grouped expression; cannot be combined with the legacy fields below.
    pub expression: Option<SmartExpr>,
    /// Any listed format, ignoring ASCII case; separate with commas, semicolons
    /// or spaces. Leading dots and JPG/JPE/TIF extension aliases are accepted.
    pub format: Option<String>,
    /// Inclusive local capture dates, YYYY-MM-DD. Missing dates do not match.
    pub captured_from: Option<String>,
    pub captured_to: Option<String>,
    /// Case-insensitive substring of camera make/model or lens.
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub min_rating: Option<i64>,
    pub max_rating: Option<i64>,
    pub label: Option<i64>,
    /// Exact keyword name, as in AssetQuery.
    pub keyword: Option<String>,
    /// Case-insensitive substring of filename or path.
    pub text: Option<String>,
}

impl SmartRules {
    /// Individual format alternatives, preserving their spelling for display.
    pub fn formats(&self) -> impl Iterator<Item = &str> {
        format_tokens(self.format.as_deref().unwrap_or_default())
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(expression) = &self.expression {
            if self.has_legacy_fields() {
                return Err(CatalogError::InvalidQuery(
                    "grouped rules cannot be combined with legacy fields",
                ));
            }
            return expression.validate();
        }
        let texts = [
            &self.format,
            &self.captured_from,
            &self.captured_to,
            &self.camera,
            &self.lens,
            &self.keyword,
            &self.text,
        ];
        if texts.iter().all(|t| t.is_none())
            && self.min_rating.is_none()
            && self.max_rating.is_none()
            && self.label.is_none()
        {
            return Err(CatalogError::InvalidQuery(
                "a Smart Collection needs at least one rule",
            ));
        }
        for text in texts.into_iter().flatten() {
            if text.trim().is_empty() || text.len() > 1024 || text.chars().any(char::is_control) {
                return Err(CatalogError::InvalidQuery(
                    "rule text must be 1-1024 bytes without control characters",
                ));
            }
        }
        if self.min_rating.is_some_and(|v| !(0..=5).contains(&v))
            || self.max_rating.is_some_and(|v| !(0..=5).contains(&v))
            || self.label.is_some_and(|v| !(0..=7).contains(&v))
        {
            return Err(CatalogError::InvalidQuery(
                "rating must be 0-5 and label must be 0-7",
            ));
        }
        if matches!((self.min_rating, self.max_rating), (Some(min), Some(max)) if min > max) {
            return Err(CatalogError::InvalidQuery(
                "minimum rating must not exceed maximum rating",
            ));
        }
        for date in [&self.captured_from, &self.captured_to]
            .into_iter()
            .flatten()
        {
            if !valid_date(date) {
                return Err(CatalogError::InvalidQuery(
                    "capture dates must be real dates in YYYY-MM-DD form",
                ));
            }
        }
        if matches!((&self.captured_from, &self.captured_to), (Some(from), Some(to)) if from > to) {
            return Err(CatalogError::InvalidQuery(
                "capture start date must not follow end date",
            ));
        }
        Ok(())
    }
}

fn format_tokens(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|value| !value.is_empty())
}

fn canonical_format(value: &str) -> &str {
    let value = value.trim_start_matches('.');
    if value.eq_ignore_ascii_case("jpg") || value.eq_ignore_ascii_case("jpe") {
        "JPEG"
    } else if value.eq_ignore_ascii_case("tif") {
        "TIFF"
    } else {
        value
    }
}

pub(crate) fn format_matches(asset: &str, rule: &str) -> bool {
    // Borrow SQLite text and bound work even for directly corrupted rule rows.
    rule.len() <= 1024
        && format_tokens(rule).any(|value| {
            !canonical_format(value).is_empty()
                && canonical_format(asset).eq_ignore_ascii_case(canonical_format(value))
        })
}

fn valid_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return false;
    }
    let parse =
        |range: std::ops::Range<usize>| value.get(range).and_then(|s| s.parse::<u32>().ok());
    let (Some(year), Some(month), Some(day)) = (parse(0..4), parse(5..7), parse(8..10)) else {
        return false;
    };
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        || year == 0
    {
        return false;
    }
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}
