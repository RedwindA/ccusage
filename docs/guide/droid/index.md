# Droid Data Source (Experimental)

> Droid support is experimental. Expect breaking changes while both ccusage and Droid continue to evolve.

ccusage can read local Droid session settings files as one of its supported data sources, using the same daily, monthly, and session report views as the rest of ccusage.

It also provides focused model and workspace reports:

```bash
ccusage droid model
ccusage droid workspace --breakdown
```

## Focused Views

::: code-group

```bash [bunx (Recommended)]
bunx @redwind/ccusage droid --help
```

```bash [npx]
npx @redwind/ccusage@latest droid --help
```

```bash [pnpm]
pnpm dlx @redwind/ccusage droid --help
```

:::

## Data Source

The CLI reads Droid session settings JSON files from `DROID_SESSIONS_DIR` (defaults to `~/.factory/sessions`). `DROID_SESSIONS_DIR` can be one directory or a comma-separated list of directories. It also reads `~/.factory/settings.json` to resolve BYOK custom model IDs to their underlying model names for pricing.

```bash
DROID_SESSIONS_DIR="$HOME/.factory/sessions,/backup/factory/sessions" ccusage droid session
```

```text
~/.factory/
├── settings.json
└── sessions/
    └── **/*.settings.json
```

## Report Views

| Focused view              | Description                                  | See also                                            |
| ------------------------- | -------------------------------------------- | --------------------------------------------------- |
| `ccusage droid daily`     | Aggregate usage by date                      | [Daily Usage](/guide/daily-reports)                 |
| `ccusage droid monthly`   | Aggregate usage by month                     | [Monthly Usage](/guide/monthly-reports)             |
| `ccusage droid session`   | Group usage by Droid session                 | [Session Usage](/guide/session-reports)             |
| `ccusage droid model`     | Aggregate session snapshots by current model | [Model & Workspace](/guide/model-workspace-reports) |
| `ccusage droid workspace` | Aggregate usage by session workspace         | [Model & Workspace](/guide/model-workspace-reports) |

These views support `--json` for structured output, `--compact` for narrow terminals, and `--offline` for cached pricing data.

## What Gets Calculated

- **Token usage** - Droid settings files provide input, output, cache creation, cache read, and thinking token counts.
- **Reasoning tokens** - Thinking tokens are included in total tokens and output-side cost estimation.
- **Pricing** - Costs are calculated from LiteLLM pricing data for the recorded model and provider. For BYOK models, Droid records a generated `customModels[].id` in each session; ccusage resolves it through `~/.factory/settings.json` and prices the corresponding `customModels[].model`.
- **Workspace** - The loader prefers settings `cwd`, then sibling JSONL `session_start.cwd`, then the encoded parent directory.
- **Model attribution** - Droid stores cumulative session usage with the current model snapshot. Model details are therefore marked `sessionSnapshot`; workspace totals themselves remain exact.

## Environment Variables

| Variable             | Description                                                                                     |
| -------------------- | ----------------------------------------------------------------------------------------------- |
| `DROID_SESSIONS_DIR` | Override the sessions directory, or comma-separated sessions directories, containing Droid data |
| `LOG_LEVEL`          | Adjust verbosity (0 silent ... 5 trace)                                                         |

## Troubleshooting

::: details No Droid usage data found
Ensure the data directory exists at `~/.factory/sessions/` and contains `*.settings.json` files. Set `DROID_SESSIONS_DIR` if your Droid data lives elsewhere or in multiple archive roots.
:::

::: details Costs showing as $0.00
If a model is not in LiteLLM's database, the cost will be $0.00. For a BYOK model, also verify that its session model ID still exists in `~/.factory/settings.json`. [Open an issue](https://github.com/ccusage/ccusage/issues/new) to request alias support.
:::
