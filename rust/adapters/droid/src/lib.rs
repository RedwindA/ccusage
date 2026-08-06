use ccusage_adapter_common::{
    collect_files_with_extension, filter_loaded_entries_by_date, read_files_parallel,
};
use ccusage_core::*;

mod loader;
mod parser;
mod paths;
mod report;

use crate::cli::{AgentCommandArgs, DimensionReportArgs, DimensionReportKind};
use crate::{PricingMap, Result, print_json_or_jq, print_usage_table, sort_summaries, wants_json};

pub use loader::load_entries;
pub(crate) use report::report_from_rows;
pub use report::summarize_entries;

pub fn run(args: AgentCommandArgs) -> Result<()> {
    let shared = args.shared;
    let pricing = PricingMap::load_with_overrides(
        shared.offline,
        crate::log_level() != Some(0),
        shared.pricing_overrides.iter(),
    );
    let mut entries = load_entries(&shared, &pricing)?;
    filter_loaded_entries_by_date(&mut entries, &shared);
    let mut rows = summarize_entries(&entries, args.kind)?;
    sort_summaries(&mut rows, &shared.order, ccusage_core::summary_period);
    if wants_json(&shared) {
        return print_json_or_jq(
            report_from_rows(&rows, args.kind),
            shared.jq.as_deref(),
            shared.no_cost,
        );
    }
    print_usage_table(
        "Droid Token Usage Report",
        ccusage_core::first_column(args.kind),
        &rows,
        &shared,
        false,
        None,
    )?;
    Ok(())
}

pub fn run_dimension(args: DimensionReportArgs) -> Result<()> {
    let shared = args.shared;
    let pricing = PricingMap::load_with_overrides(
        shared.offline,
        crate::log_level() != Some(0),
        shared.pricing_overrides.iter(),
    );
    let entries = load_entries(&shared, &pricing)?;
    let rows = summarize_dimensions(&entries, args.kind, &shared);
    if wants_json(&shared) {
        return print_json_or_jq(
            dimension_report_json(&rows, args.kind, DimensionAttribution::SessionSnapshot),
            shared.jq.as_deref(),
            shared.no_cost,
        );
    }
    let title = match args.kind {
        DimensionReportKind::Model => "Droid Usage by Model",
        DimensionReportKind::Workspace => "Droid Usage by Workspace",
    };
    print_dimension_table(
        title,
        &rows,
        args.kind,
        &shared,
        DimensionAttribution::SessionSnapshot,
    )
}
