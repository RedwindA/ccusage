# Model and Workspace Reports

Model and workspace reports answer where usage and cost accumulate after applying an optional date range. They are available for Claude Code, Codex, and Droid:

```bash
ccusage claude model
ccusage claude workspace --breakdown
ccusage codex model --speed auto
ccusage codex workspace --json
ccusage droid model
ccusage droid workspace --breakdown
```

These are focused reports only. There is no top-level `ccusage model` or `ccusage workspace` command, and other data sources continue to provide their existing time and session reports.

## Filtering and Ordering

`--since` and `--until` filter records before aggregation. Rows are sorted by cost in descending order by default; use `--order asc` for the inverse order. Unlike daily, weekly, and monthly reports, dimension reports reject `--last` because their rows are not calendar periods.

The reports also accept the shared JSON/JQ, timezone, cost mode, offline, no-cost, compact, and single-thread options. Codex additionally accepts `--speed auto|standard|fast`.

## Workspace Identity

A workspace is the complete working directory recorded in the source log. ccusage does not resolve a Git root and does not merge directories with the same basename. JSON preserves the full value. Tables shorten only the current home-directory prefix to `~`.

Missing or empty models and workspaces are grouped under `unknown`, so report totals always reconcile with the included records.

Use `--breakdown` to expand workspace table rows with their model details. Workspace JSON always contains `modelsUsed` and `modelBreakdowns`, regardless of this flag.

## JSON Shapes

Model reports return model rows and totals:

```json
{
	"models": [
		{
			"model": "claude-sonnet-4-20250514",
			"inputTokens": 100,
			"outputTokens": 25,
			"cacheCreationTokens": 10,
			"cacheReadTokens": 50,
			"totalTokens": 185,
			"totalCost": 0.01
		}
	],
	"totals": {}
}
```

Workspace reports return full workspace keys and model breakdowns:

```json
{
	"workspaces": [
		{
			"workspace": "/home/me/src/project",
			"modelsUsed": ["gpt-5"],
			"modelBreakdowns": [
				{
					"modelName": "gpt-5",
					"inputTokens": 100,
					"outputTokens": 25,
					"cacheCreationTokens": 0,
					"cacheReadTokens": 50,
					"cost": 0.01
				}
			]
		}
	],
	"totals": {}
}
```

`--no-cost` recursively removes `totalCost` and `cost` from these objects. See [JSON Output](/guide/json-output) for automation examples.

## Source Attribution

- Claude Code reads the record-level `cwd`, falling back to the existing project-directory metadata when it is absent.
- Codex reads `session_meta.payload.cwd` without changing the public token event shape. Model, replay, long-context, and speed pricing behavior is identical to existing Codex reports. See [Codex session metadata](https://github.com/openai/codex/discussions/12668).
- Droid workspace totals come from the exact latest cumulative session snapshot. The current model is also a session snapshot, so model rows and workspace model details include `"attribution": "sessionSnapshot"`; tables print the same caveat once. Workspace lookup prefers settings `cwd`, then the sibling JSONL `session_start.cwd`, then the encoded parent directory. See [Factory session-start hooks](https://docs.factory.ai/guides/hooks/session-automation).

## Configuration

Each new report can be configured under its source namespace:

```json
{
	"claude": {
		"commands": {
			"workspace": { "breakdown": true }
		}
	},
	"codex": {
		"commands": {
			"model": { "speed": "fast", "order": "asc" }
		}
	},
	"droid": {
		"commands": {
			"workspace": { "json": true }
		}
	}
}
```

See [Configuration Files](/guide/config-files) for precedence and schema setup.
