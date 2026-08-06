# Commands, Dependencies, Validation, Releases

## Commands

`just` is the single entry point (`just --list`). Each workspace package owns a
justfile imported as a module, so package recipes are `just <module>::<recipe>`
and `just <module>::--list` lists one module. Whole-repo jobs the Nix flake owns
(`fmt`, `check`, `schema`) stay at the root.

`just fmt` mutates files, so run it before the read-only checks.

## Adding A Dependency Or Tool

`comma` and `nix run` are fine for one-off investigation, but anything used
repeatedly belongs in the repo: system and dev-shell CLIs in `flake.nix`
(`nix/dev-shell.nix` is the current list), JS/TS tooling and scripts in
`package.json`. Land the matching lockfile update — `flake.lock`,
`pnpm-lock.yaml` — in the same commit so the addition stays independently
revertable.

## Validation

Git hooks and CI cover the standard path: `.pre-commit-config.yaml` is generated
from `nix/git-hooks.nix` and shows exactly which hook runs at which stage.

Run `just typecheck` and `just test` yourself when the change touches behavior,
types, or package code, or when the hooks and CI do not cover the edited files.
Narrower package recipes are useful while iterating; finish with the root ones.

## Releases

Every push to `main` runs `.github/workflows/release.yaml`, builds all native
packages, and publishes them to npm. The workflow derives a unique prerelease
version from the next patch version plus the GitHub run ID and attempt, then
updates the `latest` dist-tag. Publishing requires npm Trusted Publishing to
authorize this repository and workflow.
