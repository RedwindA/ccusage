mod loader;
mod report;
mod types;
mod users;

use ccusage_adapter_codex::CodexGroup;
#[cfg(test)]
use ccusage_adapter_codex::CodexModelUsage;
use ccusage_adapter_common::filter_loaded_entries_by_date;
use ccusage_core::*;

mod adapter {
    pub use ccusage_adapter_amp as amp;
    pub use ccusage_adapter_antigravity as antigravity;
    pub use ccusage_adapter_claude as claude;
    pub use ccusage_adapter_codebuff as codebuff;
    pub use ccusage_adapter_codex as codex;
    pub use ccusage_adapter_copilot as copilot;
    pub use ccusage_adapter_droid as droid;
    pub use ccusage_adapter_gemini as gemini;
    pub use ccusage_adapter_goose as goose;
    pub use ccusage_adapter_grok as grok;
    pub use ccusage_adapter_hermes as hermes;
    pub use ccusage_adapter_kilo as kilo;
    pub use ccusage_adapter_kimi as kimi;
    pub use ccusage_adapter_openclaw as openclaw;
    pub use ccusage_adapter_opencode as opencode;
    pub use ccusage_adapter_pi as pi;
    pub use ccusage_adapter_qwen as qwen;
    pub use ccusage_adapter_zcode as zcode;
}

use crate::{
    Result,
    cli::{AgentCommandArgs, AgentReportKind},
    print_json_or_jq, wants_json,
};

pub fn run(args: AgentCommandArgs) -> Result<()> {
    let kind = args.kind;
    let shared = args.shared;
    let include_agents = args.by_agent;
    let users = args
        .all_users
        .then(users::discover_system_users)
        .transpose()?;
    let mut startup_warnings = args
        .all_users
        .then(|| ignored_custom_source_warning(&shared))
        .flatten()
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(sections) = args.sections {
        let sections = requested_sections(kind, sections);
        let result = if let Some(users) = users.as_deref() {
            loader::load_sections_for_users(&sections, &shared, users)?
        } else {
            loader::load_sections(&sections, &shared)?
        };
        startup_warnings.extend(result.warnings.iter().cloned());
        print_warnings(startup_warnings);
        if wants_json(&shared) {
            return report::print_sections_report_json(
                &result.sections,
                kind,
                include_agents,
                shared.jq.as_deref(),
                shared.no_cost,
            );
        }
        for (section_kind, rows) in &result.sections {
            report::print_table(
                rows,
                *section_kind,
                &shared,
                result.detected_agents_for(*section_kind),
            )?;
        }
        return Ok(());
    }
    let result = if let Some(users) = users.as_deref() {
        loader::load_rows_for_users(kind, &shared, users)?
    } else {
        loader::load_rows(kind, &shared)?
    };
    startup_warnings.extend(result.warnings);
    print_warnings(startup_warnings);
    if wants_json(&shared) {
        let output = report::report_json_with_agents(&result.rows, kind, include_agents);
        return print_json_or_jq(output, shared.jq.as_deref(), shared.no_cost);
    }
    report::print_table(&result.rows, kind, &shared, &result.detected_agents)
}

fn ignored_custom_source_warning(shared: &crate::cli::SharedArgs) -> Option<String> {
    const PATH_ENV_VARS: &[&str] = &[
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "OPENCODE_DATA_DIR",
        "AMP_DATA_DIR",
        "DROID_SESSIONS_DIR",
        "CODEBUFF_DATA_DIR",
        "HERMES_HOME",
        "PI_AGENT_DIR",
        "GOOSE_PATH_ROOT",
        "OPENCLAW_DIR",
        "KILO_DATA_DIR",
        "COPILOT_HOME",
        "COPILOT_OTEL_FILE_EXPORTER_PATH",
        "GEMINI_DATA_DIR",
        "ANTIGRAVITY_DATA_DIR",
        "KIMI_DATA_DIR",
        "QWEN_DATA_DIR",
        "ZCODE_HOME",
        "XDG_CONFIG_HOME",
    ];
    let mut ignored = PATH_ENV_VARS
        .iter()
        .filter(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        .copied()
        .collect::<Vec<_>>();
    if !shared.pi_stores.is_empty() {
        ignored.push("pi.stores[]");
    }
    (!ignored.is_empty()).then(|| {
        format!(
            "Warning: --all-users ignored custom data sources: {}",
            ignored.join(", ")
        )
    })
}

fn print_warnings(warnings: Vec<String>) {
    for warning in warnings
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        eprintln!("{warning}");
    }
}

fn requested_sections(
    command_kind: AgentReportKind,
    sections: Vec<AgentReportKind>,
) -> Vec<AgentReportKind> {
    let mut requested = vec![command_kind];
    for section in [
        AgentReportKind::Daily,
        AgentReportKind::Weekly,
        AgentReportKind::Monthly,
        AgentReportKind::Session,
    ] {
        if section != command_kind && sections.contains(&section) {
            requested.push(section);
        }
    }
    requested
}

#[cfg(test)]
use loader::{
    aggregate_rows, codex_group_row, load_agent_rows_for_users, load_agent_rows_parallel,
    load_rows, load_rows_for_users, load_sections,
};
#[cfg(test)]
use report::{
    all_report_title, all_table_columns, all_table_columns_with_users, all_table_row, report_json,
    report_json_with_agents, sections_report_json,
};
#[cfg(test)]
use types::{AgentLoadSpec, AgentRows, AllRow};

#[cfg(test)]
mod tests;
