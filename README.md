# repo-template

Template for repositories that run a conventional-commit, PR-only workflow:
every change is a branch + squash-merged PR, PRs are auto-labeled by their
title prefix, and GitHub release notes are generated from merged PRs, grouped
by those labels, with a PR link and author on every line.

## Usage

1. **Use this template** → new repository (copies the files below).
2. Clone it and run `./setup.sh` — applies everything a template cannot copy:
   merge options, rulesets, labels.
3. Adjust the required status checks to your CI job names:
   `CHECKS="lint test" ./setup.sh` (default: `lint build`).

## What's inside

- `.github/workflows/label.yml` — labels each PR from its conventional title
  prefix (`feat:` → `feature`, `fix:` → `fix`, `docs:` → `documentation`, …)
  and re-labels when the title changes.
- `.github/release.yml` — groups auto-generated release notes by those labels;
  the `skip-changelog` label excludes a PR.
- `.github/workflows/pr-title.yml` — fails unless the PR title is a
  conventional commit; required by the ruleset `setup.sh` creates.
- `.github/workflows/ci.yml` — skeleton with the `lint` / `build` job names
  the required checks expect (replace the echo steps), plus a release job
  that publishes generated notes on a `v*` tag.
- `.github/dependabot.yml` — weekly action bumps as `ci(deps):` PRs; add an
  npm/gomod block once the repo has one.
- `.github/ISSUE_TEMPLATE/` — bug report and feature request forms, pre-labeled
  `bug` / `enhancement`.
- `.github/PULL_REQUEST_TEMPLATE.md` — deliberately minimal What/Why: with
  squash merges the PR description becomes the commit body, so no checklist
  boilerplate to leak into git history.
- `setup.sh` — one-shot settings bootstrap via `gh`:
  - squash-only merges, commit title = PR title, body = PR description,
    auto-merge, delete merged branches, DCO sign-off for web commits;
  - `main` ruleset: PR-only, linear history, no force-push or deletion,
    signed commits, required CI checks, review threads must be resolved;
  - `v*` tag ruleset: tags cannot be moved or deleted;
  - the changelog labels.

## Release flow

Create releases with `gh release create vX.Y.Z --generate-notes` (or an action
with `generate_release_notes: true`) — the notes come out grouped per
`.github/release.yml`.

Notes:

- Required check contexts are CI **job names**; renaming a job means updating
  the ruleset, or PRs will wait on a check that never reports.
- Rulesets have no bypass actors: nobody pushes past them, including the
  owner. The escape hatch is editing the ruleset.
