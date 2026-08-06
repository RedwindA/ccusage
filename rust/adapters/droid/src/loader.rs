use std::{collections::HashSet, sync::Arc};

use jiff::tz::TimeZone as JiffTimeZone;

use super::{
    parser::{
        DroidCustomModels, DroidEntry, calculate_droid_cost, load_custom_models,
        load_settings_file, missing_droid_pricing,
    },
    paths::{discover_settings_files, droid_session_paths, factory_settings_path},
};
use crate::{
    LoadedEntry, PricingMap, Result, UsageEntry, UsageMessage, cli::SharedArgs, debug_log,
    format_date_tz, parse_tz, read_files_parallel,
};

pub fn load_entries(shared: &SharedArgs, pricing: &PricingMap) -> Result<Vec<LoadedEntry>> {
    crate::progress::track_usage_load(
        crate::progress::UsageLoadAgent("Droid"),
        shared.json,
        || load_entries_inner(shared, pricing),
    )
}

fn load_entries_inner(shared: &SharedArgs, pricing: &PricingMap) -> Result<Vec<LoadedEntry>> {
    let tz = parse_tz(shared.timezone.as_deref());
    let roots = droid_session_paths()?;
    let custom_models = load_factory_custom_models(shared);
    let mut files = discover_settings_files()?;
    files.sort();
    // Read files in parallel, reassembled in the original (sorted) file order so
    // the subsequent stable sort and reverse latest-wins dedup pick the same
    // snapshot per session as the single-threaded read.
    let loaded = read_files_parallel(&files, shared.single_thread, |file| {
        let encoded_parent = encoded_parent_workspace(file, &roots);
        load_settings_file(file, encoded_parent.as_deref(), &custom_models).unwrap_or_else(
            |error| {
                debug_log(
                    shared,
                    format!(
                        "Failed to read Droid settings file {}: {error}",
                        file.display()
                    ),
                );
                None
            },
        )
    });
    let mut parsed: Vec<DroidEntry> = loaded.into_iter().flatten().collect();
    parsed.sort_by_key(|entry| entry.timestamp);
    let mut seen_sessions = HashSet::new();
    let mut entries = Vec::new();
    for entry in parsed.into_iter().rev() {
        if !seen_sessions.insert(entry.session_id.clone()) {
            continue;
        }
        entries.push(to_loaded_entry(entry, tz.as_ref(), pricing));
    }
    Ok(entries)
}

fn load_factory_custom_models(shared: &SharedArgs) -> DroidCustomModels {
    let Some(path) = factory_settings_path() else {
        return DroidCustomModels::new();
    };
    load_custom_models(&path).unwrap_or_else(|error| {
        debug_log(
            shared,
            format!(
                "Failed to read Droid custom models from {}: {error}",
                path.display()
            ),
        );
        DroidCustomModels::new()
    })
}

fn encoded_parent_workspace(
    path: &std::path::Path,
    roots: &[std::path::PathBuf],
) -> Option<String> {
    let root = roots
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())?;
    let parent = path.parent()?;
    if parent == root {
        return None;
    }
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn to_loaded_entry(
    entry: DroidEntry,
    tz: Option<&JiffTimeZone>,
    pricing: &PricingMap,
) -> LoadedEntry {
    let cost = calculate_droid_cost(&entry, pricing);
    let missing_pricing_model = missing_droid_pricing(&entry, pricing);
    let data = UsageEntry {
        session_id: Some(entry.session_id.clone()),
        timestamp: entry.timestamp_text.clone(),
        version: None,
        message: UsageMessage {
            usage: entry.usage,
            model: Some(entry.model.clone()),
            id: Some(format!("droid:{}", entry.session_id)),
        },
        cost_usd: None,
        request_id: None,
        is_api_error_message: None,
        is_sidechain: None,
    };
    LoadedEntry {
        date: format_date_tz(entry.timestamp, tz),
        timestamp: entry.timestamp,
        project: Arc::from("droid"),
        session_id: Arc::from(entry.session_id.as_str()),
        project_path: Arc::from("Droid"),
        workspace_path: Arc::from(entry.workspace_path),
        cost,
        credits: None,
        extra_total_tokens: entry.reasoning_tokens,
        model: Some(entry.model),
        usage_limit_reset_time: None,
        missing_pricing_model,
        message_count: None,
        data,
    }
}

#[cfg(test)]
use super::report::{report_from_rows, summarize_entries};

#[cfg(test)]
mod tests {
    use ccusage_test_support::{EnvVarGuard, EnvVarsGuard, fs_fixture};
    use serde_json::json;

    use super::super::{
        parser::{normalize_droid_model_name, parse_token_usage},
        paths::DROID_SESSIONS_DIR_ENV,
    };
    use super::*;
    use crate::{
        TokenUsageRaw, UsageEntry, UsageMessage, cli::AgentReportKind, parse_ts_timestamp,
    };
    #[test]
    fn normalizes_droid_model_names() {
        assert_eq!(
            normalize_droid_model_name("custom:Claude-Opus-4.5-Thinking-[Anthropic]-0"),
            "claude-opus-4-5-thinking-0"
        );
        assert_eq!(
            normalize_droid_model_name("Claude-Sonnet-4-[Anthropic]"),
            "claude-sonnet-4"
        );
        assert_eq!(
            normalize_droid_model_name("gemini-2.5-pro"),
            "gemini-2-5-pro"
        );
    }

    #[test]
    fn falls_back_to_total_tokens_when_droid_parts_are_missing() {
        let usage = parse_token_usage(Some(&serde_json::json!({
            "totalTokens": 456
        })))
        .unwrap();

        assert_eq!(usage.output_tokens, 456);
        assert_eq!(usage.thinking_tokens, 0);
    }

    #[test]
    fn loads_usage_from_droid_settings_files() {
        let fixture = fs_fixture!({
            "session-a.settings.json": r#"{
                "cwd": "/workspace/settings",
                "model": "Claude-Sonnet-4-[Anthropic]",
                "providerLock": "anthropic",
                "providerLockTimestamp": "2026-05-01T01:02:03.000Z",
                "tokenUsage": {
                    "inputTokens": 100,
                    "outputTokens": 50,
                    "cacheCreationTokens": 20,
                    "cacheReadTokens": 10,
                    "thinkingTokens": 5
                }
            }"#,
            "zero.settings.json": r#"{"model":"gpt-5","tokenUsage":{"inputTokens":0}}"#,
        });
        let _cleanup = EnvVarGuard::set(DROID_SESSIONS_DIR_ENV, fixture.root());

        let pricing = PricingMap::load_embedded();
        let shared = SharedArgs {
            timezone: Some("UTC".to_string()),
            ..SharedArgs::default()
        };
        let entries = load_entries(&shared, &pricing).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, "2026-05-01");
        assert_eq!(entries[0].session_id.as_ref(), "session-a");
        assert_eq!(entries[0].model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(entries[0].data.message.usage.input_tokens, 100);
        assert_eq!(entries[0].data.message.usage.output_tokens, 50);
        assert_eq!(
            entries[0].data.message.usage.cache_creation_input_tokens,
            20
        );
        assert_eq!(entries[0].data.message.usage.cache_read_input_tokens, 10);
        assert_eq!(entries[0].extra_total_tokens, 5);
        assert_eq!(entries[0].workspace_path.as_ref(), "/workspace/settings");
    }

    #[test]
    fn resolves_byok_model_ids_from_factory_settings() {
        let fixture = fs_fixture!({
            ".factory/settings.json": r#"{
                "customModels": [{
                    "model": "gpt-5.6-sol",
                    "id": "custom:GPT-5.6-Sol-Plus-0",
                    "provider": "openai"
                }]
            }"#,
            ".factory/sessions/session-byok.settings.json": r#"{
                "model": "custom:GPT-5.6-Sol-Plus-0",
                "providerLockTimestamp": "2026-05-01T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 1000, "outputTokens": 500}
            }"#,
        });
        let _environment = EnvVarsGuard::set_many([
            ("HOME", Some(fixture.root().as_os_str().to_owned())),
            (
                DROID_SESSIONS_DIR_ENV,
                Some(fixture.path(".factory/sessions").into_os_string()),
            ),
        ]);
        let shared = SharedArgs::default();
        let pricing = PricingMap::load_embedded();

        let entries = load_entries(&shared, &pricing).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model.as_deref(), Some("gpt-5-6-sol"));
        assert!(entries[0].cost > 0.0);
        assert_eq!(entries[0].missing_pricing_model, None);
    }

    #[test]
    fn falls_back_to_sidecar_jsonl_model() {
        let fixture = fs_fixture!({
            "session-b.settings.json": r#"{
                "providerLock": "anthropic",
                "providerLockTimestamp": "2026-05-02T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 10, "outputTokens": 20}
            }"#,
            "session-b.jsonl": r#"{"content":"Model: Claude Opus 4.5 Thinking [Anthropic]"}"#,
        });
        let _cleanup = EnvVarGuard::set(DROID_SESSIONS_DIR_ENV, fixture.root());

        let pricing = PricingMap::load_embedded();
        let entries = load_entries(&SharedArgs::default(), &pricing).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].data.message.model.as_deref(),
            Some("claude-opus-4-5-thinking")
        );
    }

    #[test]
    fn loads_workspace_from_session_start_then_encoded_parent() {
        let fixture = fs_fixture!({
            "encoded-workspace/session-sidecar.settings.json": r#"{
                "model": "gpt-5",
                "providerLockTimestamp": "2026-05-02T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 10, "outputTokens": 20}
            }"#,
            "encoded-workspace/session-sidecar.jsonl": r#"{"type":"session_start","cwd":"/workspace/from-sidecar"}"#,
            "encoded-fallback/session-parent.settings.json": r#"{
                "model": "gpt-5",
                "providerLockTimestamp": "2026-05-03T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 20, "outputTokens": 30}
            }"#,
        });
        let _cleanup = EnvVarGuard::set(DROID_SESSIONS_DIR_ENV, fixture.root());

        let entries = load_entries(&SharedArgs::default(), &PricingMap::load_embedded()).unwrap();
        let by_session = entries
            .iter()
            .map(|entry| (entry.session_id.as_ref(), entry.workspace_path.as_ref()))
            .collect::<std::collections::BTreeMap<_, _>>();

        assert_eq!(by_session["session-sidecar"], "/workspace/from-sidecar");
        assert_eq!(by_session["session-parent"], "encoded-fallback");
    }

    #[test]
    fn keeps_latest_snapshot_for_duplicate_session_ids() {
        let fixture = fs_fixture!({
            "archive/session-c.settings.json": r#"{
                "model": "gpt-5",
                "providerLock": "openai",
                "providerLockTimestamp": "2026-05-01T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 10, "outputTokens": 20}
            }"#,
            "session-c.settings.json": r#"{
                "model": "gpt-5",
                "providerLock": "openai",
                "providerLockTimestamp": "2026-05-02T01:02:03.000Z",
                "tokenUsage": {"inputTokens": 100, "outputTokens": 200}
            }"#,
        });
        let _cleanup = EnvVarGuard::set(DROID_SESSIONS_DIR_ENV, fixture.root());

        let pricing = PricingMap::load_embedded();
        let entries = load_entries(&SharedArgs::default(), &pricing).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id.as_ref(), "session-c");
        assert_eq!(entries[0].data.message.usage.input_tokens, 100);
        assert_eq!(entries[0].data.message.usage.output_tokens, 200);
    }

    #[test]
    fn report_total_includes_thinking_tokens() {
        let entry = LoadedEntry {
            data: UsageEntry {
                session_id: Some("session-a".to_string()),
                timestamp: "2026-05-01T01:02:03.000Z".to_string(),
                version: None,
                message: UsageMessage {
                    usage: TokenUsageRaw {
                        input_tokens: 100,
                        output_tokens: 50,
                        cache_creation_input_tokens: 20,
                        cache_read_input_tokens: 10,
                        speed: None,
                        cache_creation: None,
                    },
                    model: Some("claude-sonnet-4".to_string()),
                    id: Some("droid:session-a".to_string()),
                },
                cost_usd: None,
                request_id: None,
                is_api_error_message: None,
                is_sidechain: None,
            },
            timestamp: parse_ts_timestamp("2026-05-01T01:02:03.000Z").unwrap(),
            date: "2026-05-01".to_string(),
            project: Arc::from("droid"),
            session_id: Arc::from("session-a"),
            project_path: Arc::from("Droid"),
            workspace_path: Arc::from("unknown"),
            cost: 0.0,
            credits: None,
            extra_total_tokens: 5,
            model: Some("claude-sonnet-4".to_string()),
            usage_limit_reset_time: None,
            missing_pricing_model: None,
            message_count: None,
        };
        let rows = summarize_entries(&[entry], AgentReportKind::Daily).unwrap();
        let report = report_from_rows(&rows, AgentReportKind::Daily);

        assert_eq!(report["daily"][0]["totalTokens"], json!(185));
        assert_eq!(report["totals"]["totalTokens"], json!(185));
    }
}
