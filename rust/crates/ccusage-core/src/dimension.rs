use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal},
};

use serde_json::{Value, json};

use crate::{
    Align, Color, LoadedEntry, ModelBreakdown, Result, SimpleTable, USAGE_COMPACT_WIDTH_THRESHOLD,
    cli::{DimensionReportKind, SharedArgs, SortOrder},
    color, format_currency, format_number, home, print_box_title,
    print_missing_pricing_warnings_for_models, short_model_name, should_use_compact_layout,
    terminal_width,
};

const UNKNOWN: &str = "unknown";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DimensionAttribution {
    Exact,
    SessionSnapshot,
}

#[derive(Clone, Debug)]
pub struct DimensionRow {
    pub key: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub extra_total_tokens: u64,
    pub total_cost: f64,
    pub models_used: Vec<String>,
    pub model_breakdowns: Vec<ModelBreakdown>,
}

impl DimensionRow {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_tokens
            + self.cache_read_tokens
            + self.extra_total_tokens
    }
}

#[derive(Default)]
struct DimensionAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    extra_total_tokens: u64,
    total_cost: f64,
    breakdowns: BTreeMap<String, ModelBreakdown>,
}

impl DimensionAccumulator {
    fn add(&mut self, entry: &LoadedEntry, model: &str) {
        let usage = entry.data.message.usage;
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cache_creation_tokens += usage.cache_creation_token_count();
        self.cache_read_tokens += usage.cache_read_input_tokens;
        self.extra_total_tokens += entry.extra_total_tokens;
        self.total_cost += entry.cost;

        let breakdown =
            self.breakdowns
                .entry(model.to_string())
                .or_insert_with(|| ModelBreakdown {
                    model_name: model.to_string(),
                    ..ModelBreakdown::default()
                });
        breakdown.input_tokens += usage.input_tokens;
        breakdown.output_tokens += usage.output_tokens;
        breakdown.cache_creation_tokens += usage.cache_creation_token_count();
        breakdown.cache_read_tokens += usage.cache_read_input_tokens;
        breakdown.extra_total_tokens += entry.extra_total_tokens;
        breakdown.cost += entry.cost;
        breakdown.missing_pricing |= entry.missing_pricing_model.is_some();
    }

    fn into_row(self, key: String) -> DimensionRow {
        let mut model_breakdowns = self.breakdowns.into_values().collect::<Vec<_>>();
        model_breakdowns.sort_by(|a, b| {
            b.cost
                .total_cmp(&a.cost)
                .then_with(|| a.model_name.cmp(&b.model_name))
        });
        let mut models_used = model_breakdowns
            .iter()
            .map(|item| item.model_name.clone())
            .collect::<Vec<_>>();
        models_used.sort();
        DimensionRow {
            key,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation_tokens: self.cache_creation_tokens,
            cache_read_tokens: self.cache_read_tokens,
            extra_total_tokens: self.extra_total_tokens,
            total_cost: self.total_cost,
            models_used,
            model_breakdowns,
        }
    }
}

pub fn summarize_dimensions(
    entries: &[LoadedEntry],
    kind: DimensionReportKind,
    shared: &SharedArgs,
) -> Vec<DimensionRow> {
    let mut groups = BTreeMap::<String, DimensionAccumulator>::new();
    for entry in entries.iter().filter(|entry| in_date_range(entry, shared)) {
        let model = normalized_model(entry.model.as_deref());
        let key = match kind {
            DimensionReportKind::Model => model.clone(),
            DimensionReportKind::Workspace => normalized_workspace(&entry.workspace_path),
        };
        groups.entry(key).or_default().add(entry, &model);
    }
    let mut rows = groups
        .into_iter()
        .map(|(key, group)| group.into_row(key))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        let cost = a.total_cost.total_cmp(&b.total_cost);
        let cost = match shared.order {
            SortOrder::Asc => cost,
            SortOrder::Desc => cost.reverse(),
        };
        cost.then_with(|| a.key.cmp(&b.key))
    });
    rows
}

fn in_date_range(entry: &LoadedEntry, shared: &SharedArgs) -> bool {
    let date = entry.date.replace('-', "");
    shared.since.as_ref().is_none_or(|since| date >= *since)
        && shared.until.as_ref().is_none_or(|until| date <= *until)
}

fn normalized_model(model: Option<&str>) -> String {
    let model = model.map(str::trim).filter(|model| !model.is_empty());
    model
        .map(crate::model_aliases::resolve_model_name)
        .map(|model| model.into_owned())
        .unwrap_or_else(|| UNKNOWN.to_string())
}

fn normalized_workspace(workspace: &str) -> String {
    if workspace.trim().is_empty() {
        UNKNOWN.to_string()
    } else {
        workspace.to_string()
    }
}

pub fn dimension_report_json(
    rows: &[DimensionRow],
    kind: DimensionReportKind,
    attribution: DimensionAttribution,
) -> Value {
    let items = rows
        .iter()
        .map(|row| dimension_row_json(row, kind, attribution))
        .collect::<Vec<_>>();
    let key = match kind {
        DimensionReportKind::Model => "models",
        DimensionReportKind::Workspace => "workspaces",
    };
    let mut value = json!({"totals": dimension_totals_json(rows)});
    value[key] = json!(items);
    value
}

fn dimension_row_json(
    row: &DimensionRow,
    kind: DimensionReportKind,
    attribution: DimensionAttribution,
) -> Value {
    let mut value = json!({
        "inputTokens": row.input_tokens,
        "outputTokens": row.output_tokens,
        "cacheCreationTokens": row.cache_creation_tokens,
        "cacheReadTokens": row.cache_read_tokens,
        "totalTokens": row.total_tokens(),
        "totalCost": row.total_cost,
    });
    let object = value.as_object_mut().expect("dimension row is an object");
    match kind {
        DimensionReportKind::Model => {
            object.insert("model".to_string(), json!(row.key));
            if attribution == DimensionAttribution::SessionSnapshot {
                object.insert("attribution".to_string(), json!("sessionSnapshot"));
            }
        }
        DimensionReportKind::Workspace => {
            object.insert("workspace".to_string(), json!(row.key));
            object.insert("modelsUsed".to_string(), json!(row.models_used));
            object.insert(
                "modelBreakdowns".to_string(),
                Value::Array(
                    row.model_breakdowns
                        .iter()
                        .map(|breakdown| breakdown_json(breakdown, attribution))
                        .collect(),
                ),
            );
        }
    }
    value
}

fn breakdown_json(breakdown: &ModelBreakdown, attribution: DimensionAttribution) -> Value {
    let mut value = serde_json::to_value(breakdown).expect("model breakdown serializes");
    if attribution == DimensionAttribution::SessionSnapshot {
        value
            .as_object_mut()
            .expect("model breakdown is an object")
            .insert("attribution".to_string(), json!("sessionSnapshot"));
    }
    value
}

fn dimension_totals_json(rows: &[DimensionRow]) -> Value {
    let input_tokens = rows.iter().map(|row| row.input_tokens).sum::<u64>();
    let output_tokens = rows.iter().map(|row| row.output_tokens).sum::<u64>();
    let cache_creation_tokens = rows
        .iter()
        .map(|row| row.cache_creation_tokens)
        .sum::<u64>();
    let cache_read_tokens = rows.iter().map(|row| row.cache_read_tokens).sum::<u64>();
    let extra_total_tokens = rows.iter().map(|row| row.extra_total_tokens).sum::<u64>();
    json!({
        "inputTokens": input_tokens,
        "outputTokens": output_tokens,
        "cacheCreationTokens": cache_creation_tokens,
        "cacheReadTokens": cache_read_tokens,
        "totalTokens": input_tokens + output_tokens + cache_creation_tokens + cache_read_tokens + extra_total_tokens,
        "totalCost": rows.iter().map(|row| row.total_cost).sum::<f64>(),
    })
}

pub fn print_dimension_table(
    title: &str,
    rows: &[DimensionRow],
    kind: DimensionReportKind,
    shared: &SharedArgs,
    attribution: DimensionAttribution,
) -> Result<()> {
    if rows.is_empty() {
        eprintln!("No usage data found.");
        return Ok(());
    }
    let width = terminal_width();
    let compact = should_use_compact_layout(
        shared,
        io::stdout().is_terminal(),
        width,
        USAGE_COMPACT_WIDTH_THRESHOLD,
    );
    print_box_title(title, shared);
    let first = match kind {
        DimensionReportKind::Model => "Model",
        DimensionReportKind::Workspace => "Workspace",
    };
    let mut headers = if compact {
        vec![first, "Models", "Input", "Output", "Cost (USD)"]
    } else {
        vec![
            first,
            "Models",
            "Input",
            "Output",
            "Cache Create",
            "Cache Read",
            "Total Tokens",
            "Cost (USD)",
        ]
    };
    let mut aligns = vec![Align::Left, Align::Right, Align::Right, Align::Right];
    if compact {
        aligns.push(Align::Right);
    } else {
        aligns.extend([Align::Right, Align::Right, Align::Right, Align::Right]);
    }
    if kind == DimensionReportKind::Model {
        headers.remove(1);
        aligns.remove(1);
    }
    if shared.no_cost {
        headers.pop();
        aligns.pop();
    }
    let mut table =
        SimpleTable::new(headers, aligns, crate::terminal_style(shared)).with_terminal_width(width);
    for row in rows {
        let label = match kind {
            DimensionReportKind::Model => short_model_name(&row.key),
            DimensionReportKind::Workspace => abbreviated_workspace(&row.key),
        };
        let models = match kind {
            DimensionReportKind::Model => String::new(),
            DimensionReportKind::Workspace => row.models_used.len().to_string(),
        };
        table.push(table_values(label, models, row, kind, compact, shared));
        if kind == DimensionReportKind::Workspace && shared.breakdown {
            for breakdown in &row.model_breakdowns {
                table.push(breakdown_values(breakdown, compact, shared));
            }
        }
    }
    table.separator();
    let totals = dimension_totals_json(rows);
    let total = DimensionRow {
        key: "Total".to_string(),
        input_tokens: totals["inputTokens"].as_u64().unwrap_or_default(),
        output_tokens: totals["outputTokens"].as_u64().unwrap_or_default(),
        cache_creation_tokens: totals["cacheCreationTokens"].as_u64().unwrap_or_default(),
        cache_read_tokens: totals["cacheReadTokens"].as_u64().unwrap_or_default(),
        extra_total_tokens: totals["totalTokens"].as_u64().unwrap_or_default()
            - totals["inputTokens"].as_u64().unwrap_or_default()
            - totals["outputTokens"].as_u64().unwrap_or_default()
            - totals["cacheCreationTokens"].as_u64().unwrap_or_default()
            - totals["cacheReadTokens"].as_u64().unwrap_or_default(),
        total_cost: totals["totalCost"].as_f64().unwrap_or_default(),
        models_used: Vec::new(),
        model_breakdowns: Vec::new(),
    };
    let mut total_values = table_values(
        "Total".to_string(),
        String::new(),
        &total,
        kind,
        compact,
        shared,
    );
    for value in &mut total_values {
        if !value.is_empty() {
            *value = color(shared, &*value, Color::Yellow);
        }
    }
    table.push(total_values);
    table.print()?;

    let missing = rows
        .iter()
        .flat_map(|row| &row.model_breakdowns)
        .filter(|item| item.missing_pricing)
        .map(|item| item.model_name.as_str())
        .collect::<BTreeSet<_>>();
    print_missing_pricing_warnings_for_models(missing, shared.offline);
    if attribution == DimensionAttribution::SessionSnapshot {
        eprintln!(
            "NOTE  Droid model attribution uses the latest model snapshot for each session; workspace totals remain exact session totals."
        );
    }
    if compact {
        eprintln!("\nRunning in Compact Mode");
        eprintln!("Expand terminal width to see cache metrics and total tokens");
    }
    Ok(())
}

fn table_values(
    label: String,
    models: String,
    row: &DimensionRow,
    kind: DimensionReportKind,
    compact: bool,
    shared: &SharedArgs,
) -> Vec<String> {
    let mut values = if compact {
        vec![
            label,
            models,
            format_number(row.input_tokens),
            format_number(row.output_tokens),
            format_currency(row.total_cost),
        ]
    } else {
        vec![
            label,
            models,
            format_number(row.input_tokens),
            format_number(row.output_tokens),
            format_number(row.cache_creation_tokens),
            format_number(row.cache_read_tokens),
            format_number(row.total_tokens()),
            format_currency(row.total_cost),
        ]
    };
    if kind == DimensionReportKind::Model {
        values.remove(1);
    }
    if shared.no_cost {
        values.pop();
    }
    values
}

fn breakdown_values(row: &ModelBreakdown, compact: bool, shared: &SharedArgs) -> Vec<String> {
    let total = row.input_tokens
        + row.output_tokens
        + row.cache_creation_tokens
        + row.cache_read_tokens
        + row.extra_total_tokens;
    let mut values = if compact {
        vec![
            format!("  └─ {}", short_model_name(&row.model_name)),
            String::new(),
            format_number(row.input_tokens),
            format_number(row.output_tokens),
            format_currency(row.cost),
        ]
    } else {
        vec![
            format!("  └─ {}", short_model_name(&row.model_name)),
            String::new(),
            format_number(row.input_tokens),
            format_number(row.output_tokens),
            format_number(row.cache_creation_tokens),
            format_number(row.cache_read_tokens),
            format_number(total),
            format_currency(row.cost),
        ]
    };
    if shared.no_cost {
        values.pop();
    }
    values
        .into_iter()
        .map(|value| color(shared, value, Color::Grey))
        .collect()
}

fn abbreviated_workspace(workspace: &str) -> String {
    let Some(home) = home::home_dir() else {
        return workspace.to_string();
    };
    let Some(home) = home.to_str() else {
        return workspace.to_string();
    };
    if workspace == home {
        return "~".to_string();
    }
    if let Some(rest) = workspace.strip_prefix(home)
        && matches!(rest.as_bytes().first(), Some(b'/') | Some(b'\\'))
    {
        return format!("~{rest}");
    }
    workspace.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{TimestampMs, TokenUsageRaw, UsageEntry, UsageMessage};

    fn entry(workspace: &str, model: Option<&str>, cost: f64, date: &str) -> LoadedEntry {
        LoadedEntry {
            data: UsageEntry {
                session_id: Some("session".to_string()),
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                version: None,
                message: UsageMessage {
                    usage: TokenUsageRaw {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_creation_input_tokens: 2,
                        cache_read_input_tokens: 1,
                        ..TokenUsageRaw::default()
                    },
                    model: model.map(str::to_string),
                    id: None,
                },
                cost_usd: None,
                request_id: None,
                is_api_error_message: None,
                is_sidechain: None,
            },
            timestamp: TimestampMs::from_millis(0),
            date: date.to_string(),
            project: Arc::from("project"),
            session_id: Arc::from("session"),
            project_path: Arc::from("legacy-project"),
            workspace_path: Arc::from(workspace),
            cost,
            extra_total_tokens: 0,
            credits: None,
            message_count: None,
            model: model.map(str::to_string),
            usage_limit_reset_time: None,
            missing_pricing_model: None,
        }
    }

    #[test]
    fn aggregates_after_date_filter_and_sorts_by_cost() {
        let entries = vec![
            entry("/work/a", Some("model-a"), 1.0, "2026-01-01"),
            entry("/work/a", Some("model-b"), 4.0, "2026-01-02"),
            entry("/work/b", None, 2.0, "2026-01-02"),
        ];
        let shared = SharedArgs {
            since: Some("20260102".to_string()),
            order: SortOrder::Desc,
            ..SharedArgs::default()
        };
        let rows = summarize_dimensions(&entries, DimensionReportKind::Workspace, &shared);
        assert_eq!(
            rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
            ["/work/a", "/work/b"]
        );
        assert_eq!(rows[1].models_used, ["unknown"]);
        assert_eq!(rows.iter().map(|row| row.total_cost).sum::<f64>(), 6.0);
    }

    #[test]
    fn shapes_droid_snapshot_attribution() {
        let rows = summarize_dimensions(
            &[entry("/work/a", Some("model-a"), 1.0, "2026-01-02")],
            DimensionReportKind::Workspace,
            &SharedArgs::default(),
        );
        let value = dimension_report_json(
            &rows,
            DimensionReportKind::Workspace,
            DimensionAttribution::SessionSnapshot,
        );
        assert_eq!(
            value["workspaces"][0]["modelBreakdowns"][0]["attribution"],
            "sessionSnapshot"
        );
    }

    #[test]
    fn no_cost_strips_dimension_costs_recursively() {
        let rows = summarize_dimensions(
            &[entry("/work/a", Some("model-a"), 1.0, "2026-01-02")],
            DimensionReportKind::Workspace,
            &SharedArgs::default(),
        );
        let mut value = dimension_report_json(
            &rows,
            DimensionReportKind::Workspace,
            DimensionAttribution::Exact,
        );

        crate::output::strip_cost_json(&mut value);

        assert!(value["workspaces"][0].get("totalCost").is_none());
        assert!(
            value["workspaces"][0]["modelBreakdowns"][0]
                .get("cost")
                .is_none()
        );
        assert!(value["totals"].get("totalCost").is_none());
    }

    #[test]
    fn dimension_table_values_cover_full_compact_and_breakdown_layouts() {
        let rows = summarize_dimensions(
            &[entry("/work/a", Some("model-a"), 1.0, "2026-01-02")],
            DimensionReportKind::Workspace,
            &SharedArgs::default(),
        );
        let row = &rows[0];

        assert_eq!(
            table_values(
                row.key.clone(),
                "1".to_string(),
                row,
                DimensionReportKind::Workspace,
                false,
                &SharedArgs::default(),
            )
            .len(),
            8
        );
        assert_eq!(
            table_values(
                row.key.clone(),
                "1".to_string(),
                row,
                DimensionReportKind::Workspace,
                true,
                &SharedArgs::default(),
            )
            .len(),
            5
        );
        assert_eq!(
            table_values(
                row.key.clone(),
                String::new(),
                row,
                DimensionReportKind::Model,
                false,
                &SharedArgs::default(),
            )
            .len(),
            7
        );
        assert_eq!(
            breakdown_values(&row.model_breakdowns[0], false, &SharedArgs::default()).len(),
            8
        );
    }
}
