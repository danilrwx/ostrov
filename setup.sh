#!/usr/bin/env bash
# Applies the repository settings a GitHub template cannot copy: merge
# options, branch/tag rulesets and changelog labels. Run once from a clone
# of the new repository (requires an authenticated `gh`).
#
# Usage:
#   ./setup.sh                    # repo inferred from the current directory
#   ./setup.sh OWNER/REPO
#   CHECKS="lint test" ./setup.sh # CI checks to require besides the PR title
#                                 # check (default: lint build)
set -euo pipefail

repo="${1:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
checks="${CHECKS:-lint build} conventional-title"
echo "==> $repo (required checks: $checks)"

echo "==> merge settings"
gh api -X PATCH "repos/$repo" \
  -F allow_squash_merge=true \
  -F allow_merge_commit=false \
  -F allow_rebase_merge=false \
  -F allow_auto_merge=true \
  -F delete_branch_on_merge=true \
  -F web_commit_signoff_required=true \
  -f squash_merge_commit_title=PR_TITLE \
  -f squash_merge_commit_message=PR_BODY >/dev/null

echo "==> ruleset: main"
# context = the CI job name, integration 15368 = GitHub Actions
checks_json=$(for c in $checks; do
  printf '{"context":"%s","integration_id":15368},' "$c"
done)
gh api -X POST "repos/$repo/rulesets" --input - >/dev/null <<EOF
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "rules": [
    { "type": "non_fast_forward" },
    { "type": "required_linear_history" },
    { "type": "required_signatures" },
    { "type": "deletion" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": true,
        "required_reviewers": [],
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true,
        "allowed_merge_methods": ["squash"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "do_not_enforce_on_create": false,
        "required_status_checks": [${checks_json%,}]
      }
    }
  ]
}
EOF

echo "==> ruleset: release-tags (v*)"
gh api -X POST "repos/$repo/rulesets" --input - >/dev/null <<'EOF'
{
  "name": "release-tags",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
  "rules": [
    { "type": "update" },
    { "type": "deletion" },
    { "type": "non_fast_forward" }
  ]
}
EOF

echo "==> labels"
while IFS='|' read -r name color desc; do
  gh label create "$name" -R "$repo" -c "$color" -d "$desc" --force
done <<'EOF'
feature|#0e8a16|New feature (feat:)
fix|#d73a4a|Bug fix (fix:)
performance|#ffd33d|Performance (perf:)
refactor|#a2eeef|Refactoring (refactor:)
test|#0075ca|Tests (test:)
build|#6f42c1|Build & CI (build:, ci:)
chore|#cfd3d7|Chores (chore:, style:)
skip-changelog|#ffffff|Exclude from release notes
EOF

echo "==> done"
