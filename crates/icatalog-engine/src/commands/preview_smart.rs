//! Read-only preview of unsaved Smart Collection rules.
use crate::{Context, Outcome, Params, Result};

pub(super) fn run(ctx: &Context, params: &Params) -> Result<Outcome> {
    let rules = super::smart_collection::parse(params)?;
    let count = ctx.catalog().count_smart_matches(&rules)?;
    let noun = if count == 1 { "item" } else { "items" };
    Ok(Outcome::text(format!(
        "{count} matching {noun}\n{}",
        rules.summary()
    )))
}
