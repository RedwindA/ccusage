# Push Reference

This personal fork uses direct pushes to `main` for rapid development. Confirm
the current branch and worktree state before pushing:

```bash
git branch --show-current
git status --short
```

Commit completed, validated work on `main` by default. Use a feature branch and
PR only when the user explicitly requests one.

If work was already committed on a feature branch, fetch `origin/main`, verify
that the update is a fast-forward, then fast-forward local `main` before
pushing. Stop and ask instead of merging or force-pushing when histories have
diverged.

Check for an upstream:

```bash
git rev-parse --abbrev-ref --symbolic-full-name @{u}
```

- The `main` upstream exists: `git push`.
- The `main` upstream is missing: ask the user before running
  `git push -u origin main`, and skip the push if they decline.

A push to `main` starts the prerelease publishing workflow. Push only completed,
validated changes; see the `development` skill's `references/commands.md` for
the release behavior.
