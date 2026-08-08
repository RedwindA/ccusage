use crate::{LoadedEntry, PricingMap, Result, cli::SharedArgs};

use super::parser;

pub fn load_entries(shared: &SharedArgs) -> Result<Vec<LoadedEntry>> {
    load_entries_with_pricing(shared, None)
}

pub fn load_entries_with_pricing(
    shared: &SharedArgs,
    pricing: Option<&PricingMap>,
) -> Result<Vec<LoadedEntry>> {
    crate::progress::track_usage_load(crate::progress::UsageLoadAgent("Qwen"), shared.json, || {
        parser::load_entries(shared, pricing)
    })
}
